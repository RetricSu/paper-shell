//! Cloud Backup built-in plugin: backup and sync documents to self-hosted cloud storage.
//!
//! Supports self-hosted WebDAV or standard HTTP servers (such as Nextcloud, ownCloud,
//! Alist, Synology NAS, Apache/Nginx WebDAV, Caddy, etc.).
//!
//! Provides optional client-side end-to-end encryption using AES-256-GCM with
//! PBKDF2-HMAC-SHA256 key derivation.
//!
//! Sync logic is completely manual (no background or automatic sync):
//! users trigger backup or restore explicitly through buttons in the UI.

use crate::plugin::{Plugin, PluginContext, PluginError, PluginMetadata};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Local;
use pbkdf2::pbkdf2_hmac;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::Duration;
use url::Url;
use zeroize::Zeroizing;

const LEGACY_PBKDF2_ROUNDS: u32 = 100_000;
const PBKDF2_ROUNDS: u32 = 600_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

/// Characters that must be percent-encoded in a URL path segment.
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'%')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'|');

/// Persistent configuration for the Cloud Backup plugin.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudBackupConfig {
    /// Server URL (e.g. WebDAV endpoint: https://example.com/dav/papershell/ or http://192.168.1.100:8080/backups/)
    pub server_url: String,
    /// Username for HTTP Basic Auth (optional)
    pub username: String,
    /// Session-only password or Bearer Token. Ignore legacy plaintext on disk.
    #[serde(skip)]
    pub password: Zeroizing<String>,
    /// Remote filename template (e.g. "{stem}_{doc_id}.backup" or "{filename}.backup")
    pub remote_filename: String,
    /// Whether end-to-end encryption is enabled
    pub enable_encryption: bool,
    /// Session-only passphrase, never serialized or loaded from legacy config.
    #[serde(skip)]
    pub encryption_password: Zeroizing<String>,
    /// Last backup timestamp (for UI display)
    pub last_backup_time: Option<String>,
    /// Last backup status message
    pub last_backup_status: Option<String>,
}

impl std::fmt::Debug for CloudBackupConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudBackupConfig")
            .field("credentials", &"[REDACTED]")
            .field("enable_encryption", &self.enable_encryption)
            .finish_non_exhaustive()
    }
}

impl Default for CloudBackupConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            username: String::new(),
            password: Zeroizing::new(String::new()),
            remote_filename: "{stem}_{doc_id}.backup".to_string(),
            enable_encryption: false,
            encryption_password: Zeroizing::new(String::new()),
            last_backup_time: None,
            last_backup_status: None,
        }
    }
}

/// The inner document payload contained within a backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupPayload {
    pub version: u32,
    pub doc_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub filename: String,
    pub content: String,
    pub timestamp: String,
    pub word_count: usize,
}

/// The envelope sent to or received from the cloud storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEnvelope {
    pub version: u32,
    pub doc_id: String,
    pub encrypted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub salt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ciphertext: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<BackupPayload>,
}

/// The Cloud Backup plugin implementation.
pub struct CloudBackupPlugin {
    config: CloudBackupConfig,
}

impl CloudBackupPlugin {
    pub fn new(config: CloudBackupConfig) -> Self {
        Self { config }
    }
}

impl Plugin for CloudBackupPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "cloud_backup".to_string(),
            name: "云端备份".to_string(),
            description: "自建云端同步与备份（支持 WebDAV / HTTP），可选端到端加密，手动点击同步"
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: "Paper Shell".to_string(),
            source: crate::plugin::PluginSource::Builtin,
            enabled: true,
            has_settings: true,
            dir: None,
        }
    }

    fn run(&self, ctx: &PluginContext) -> Result<String, PluginError> {
        let server_url = self.config.server_url.trim();
        if server_url.is_empty() {
            return Err(PluginError::Config(
                "未设置云端备份服务器地址。请在插件菜单中打开「配置云端备份」进行设置。"
                    .to_string(),
            ));
        }

        let identity =
            DocumentIdentity::new(ctx.file_path.as_deref(), &uuid::Uuid::new_v4().to_string());

        let target_url = resolve_target_url(
            server_url,
            &self.config.remote_filename,
            &identity.stem,
            &identity.doc_id,
            &identity.filename,
        )?;

        let result = upload_backup(&self.config, &target_url, &identity, &ctx.content)?;

        Ok(result)
    }
}

/// Computes a deterministic 16-hex-character document id from its source path,
/// or a caller-provided unique draft id if unsaved.
pub fn compute_doc_id(source_path: Option<&str>, draft_id: &str) -> String {
    if let Some(path) = source_path {
        let hash = xxhash_rust::xxh64::xxh64(path.as_bytes(), 0);
        format!("{:016x}", hash)
    } else {
        format!("draft-{}", draft_id)
    }
}

/// Identifies a document for cloud backup purposes, differentiating files with
/// identical names in different directories and unsaved drafts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentIdentity {
    pub filename: String,
    pub stem: String,
    pub doc_id: String,
    pub source_path: Option<String>,
}

impl DocumentIdentity {
    pub fn new(path: Option<&std::path::Path>, draft_id: &str) -> Self {
        if let Some(p) = path {
            let filename = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("未命名.txt")
                .to_string();
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string();
            let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            let path_str = canonical.to_string_lossy();
            let doc_id = compute_doc_id(Some(&path_str), draft_id);
            Self {
                filename,
                stem,
                doc_id,
                source_path: Some(path_str.to_string()),
            }
        } else {
            Self {
                filename: "未命名文档.md".to_string(),
                stem: "untitled".to_string(),
                doc_id: compute_doc_id(None, draft_id),
                source_path: None,
            }
        }
    }
}

/// Resolves the full URL to upload/download a backup file using proper URL parsing
/// and path-segment percent-encoding.
pub fn resolve_target_url(
    server_url: &str,
    template: &str,
    stem: &str,
    doc_id: &str,
    filename: &str,
) -> Result<String, PluginError> {
    let server_url = server_url.trim();
    if server_url.is_empty() {
        return Err(PluginError::Config("云端服务地址为空".to_string()));
    }

    let template = template.trim();
    let template = if template.is_empty() {
        "{stem}_{doc_id}.backup"
    } else {
        template
    };

    let resolved_name = template
        .replace("{doc_id}", doc_id)
        .replace("{stem}", stem)
        .replace("{filename}", filename);

    let mut base_url = validate_endpoint(server_url)?;

    if !base_url.path().ends_with('/') {
        let new_path = format!("{}/", base_url.path());
        base_url.set_path(&new_path);
    }

    let encoded_segment = utf8_percent_encode(&resolved_name, PATH_SEGMENT_ENCODE_SET).to_string();
    let target = base_url
        .join(&encoded_segment)
        .map_err(|e| PluginError::Config(format!("无法构建目标 URL: {}", e)))?;

    Ok(target.to_string())
}

/// Only literal loopback hosts are exempt; private LAN addresses still use plaintext.
pub fn insecure_http_endpoint(endpoint: &str) -> bool {
    Url::parse(endpoint.trim()).is_ok_and(|url| {
        url.scheme() == "http"
            && !match url.host() {
                Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                None => false,
            }
    })
}

fn validate_endpoint(endpoint: &str) -> Result<Url, PluginError> {
    let url = Url::parse(endpoint.trim())
        .map_err(|_| PluginError::Config("无效的服务器 URL".to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(PluginError::Config(
            "请使用 HTTP(S) 目录地址，不含用户名、密码、查询参数或片段；凭据请填写在密码栏。"
                .to_string(),
        ));
    }
    Ok(url)
}

/// Encrypts a payload into a `BackupEnvelope` using the specified password.
pub fn encrypt_payload(
    payload: &BackupPayload,
    password: &str,
) -> Result<BackupEnvelope, PluginError> {
    if password.trim().is_empty() {
        return Err(PluginError::Config(
            "启用了端到端加密，但未提供加密密码。".to_string(),
        ));
    }

    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt)
        .map_err(|e| PluginError::Execution(format!("生成随机盐值失败: {}", e)))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce_bytes)
        .map_err(|e| PluginError::Execution(format!("生成随机 Nonce 失败: {}", e)))?;

    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, PBKDF2_ROUNDS, &mut *key);

    let cipher = Aes256Gcm::new_from_slice(&*key)
        .map_err(|e| PluginError::Execution(format!("初始化 AES-GCM 密码器失败: {}", e)))?;

    let plaintext = Zeroizing::new(
        serde_json::to_vec(payload)
            .map_err(|e| PluginError::Execution(format!("序列化备份数据失败: {}", e)))?,
    );

    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|e| PluginError::Execution(format!("构造 Nonce 失败: {}", e)))?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|e| PluginError::Execution(format!("加密数据失败: {}", e)))?;

    Ok(BackupEnvelope {
        version: 2,
        doc_id: payload.doc_id.clone(),
        encrypted: true,
        salt: Some(BASE64.encode(salt)),
        nonce: Some(BASE64.encode(nonce_bytes)),
        ciphertext: Some(BASE64.encode(ciphertext)),
        payload: None,
    })
}

/// Decrypts a `BackupEnvelope` into a `BackupPayload` using the specified password.
pub fn decrypt_envelope(
    envelope: &BackupEnvelope,
    password: &str,
) -> Result<BackupPayload, PluginError> {
    if !matches!(envelope.version, 1 | 2) {
        return Err(PluginError::Execution(format!(
            "不支持的备份信封版本: {}",
            envelope.version
        )));
    }

    if !envelope.encrypted {
        let payload = envelope
            .payload
            .clone()
            .ok_or_else(|| PluginError::Execution("备份信封中缺少数据有效载荷".to_string()))?;
        if payload.version != 1 {
            return Err(PluginError::Execution(format!(
                "不支持的备份载荷版本: {}",
                payload.version
            )));
        }
        if payload.doc_id != envelope.doc_id {
            return Err(PluginError::Execution(
                "备份信封与载荷的文档标识不一致，数据可能已损坏".to_string(),
            ));
        }
        return Ok(payload);
    }

    if password.trim().is_empty() {
        return Err(PluginError::Config(
            "该云端备份已加密，请输入加密密码后恢复".to_string(),
        ));
    }

    let salt_b64 = envelope
        .salt
        .as_ref()
        .ok_or_else(|| PluginError::Execution("加密备份信封中缺少盐值 (salt)".to_string()))?;
    let nonce_b64 = envelope
        .nonce
        .as_ref()
        .ok_or_else(|| PluginError::Execution("加密备份信封中缺少随机数 (nonce)".to_string()))?;
    let ciphertext_b64 = envelope
        .ciphertext
        .as_ref()
        .ok_or_else(|| PluginError::Execution("加密备份信封中缺少密文 (ciphertext)".to_string()))?;

    let salt = BASE64
        .decode(salt_b64)
        .map_err(|e| PluginError::Execution(format!("无效的 Base64 盐值: {}", e)))?;
    let nonce_bytes = BASE64
        .decode(nonce_b64)
        .map_err(|e| PluginError::Execution(format!("无效的 Base64 Nonce: {}", e)))?;
    let ciphertext = BASE64
        .decode(ciphertext_b64)
        .map_err(|e| PluginError::Execution(format!("无效的 Base64 密文: {}", e)))?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err(PluginError::Execution(format!(
            "Nonce 长度非法: 期望 {} 字节，实际 {} 字节",
            NONCE_LEN,
            nonce_bytes.len()
        )));
    }

    let mut key = Zeroizing::new([0u8; 32]);
    if salt.len() != SALT_LEN {
        return Err(PluginError::Execution("盐值长度非法".to_string()));
    }
    let rounds = if envelope.version == 1 {
        LEGACY_PBKDF2_ROUNDS
    } else {
        PBKDF2_ROUNDS
    };
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, rounds, &mut *key);

    let cipher = Aes256Gcm::new_from_slice(&*key)
        .map_err(|e| PluginError::Execution(format!("初始化 AES-GCM 密码器失败: {}", e)))?;

    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|e| PluginError::Execution(format!("构造 Nonce 失败: {}", e)))?;
    let decrypted = Zeroizing::new(cipher.decrypt(&nonce, ciphertext.as_ref()).map_err(|_| {
        PluginError::Execution("解密失败：密码错误或云端备份数据已损坏".to_string())
    })?);

    let payload: BackupPayload = serde_json::from_slice(&decrypted)
        .map_err(|e| PluginError::Execution(format!("反序列化解密数据失败: {}", e)))?;

    if payload.version != 1 {
        return Err(PluginError::Execution(format!(
            "不支持的备份载荷版本: {}",
            payload.version
        )));
    }

    if payload.doc_id != envelope.doc_id {
        return Err(PluginError::Execution(
            "备份信封与载荷的文档标识不一致，数据可能已损坏".to_string(),
        ));
    }

    Ok(payload)
}

/// Creates a new HTTP agent with timeout settings.
fn build_http_agent() -> ureq::Agent {
    let config = ureq::config::Config::builder()
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(25)))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Applies authorization headers to a request if credentials are configured.
fn apply_auth<B>(
    mut req: ureq::RequestBuilder<B>,
    username: &str,
    password: &str,
) -> ureq::RequestBuilder<B> {
    let username = username.trim();

    if !username.is_empty() {
        let auth_str = Zeroizing::new(format!("{}:{}", username, password));
        let encoded = Zeroizing::new(BASE64.encode(auth_str.as_bytes()));
        req = req.header("Authorization", format!("Basic {}", encoded.as_str()));
    } else if !password.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", password));
    }
    req
}

/// Tests connection and accessibility of the server URL.
pub fn test_connection(config: &CloudBackupConfig) -> Result<String, PluginError> {
    let url = config.server_url.trim();
    if url.is_empty() {
        return Err(PluginError::Config("云端服务器地址为空".to_string()));
    }

    validate_endpoint(url)?;
    let agent = build_http_agent();
    let req = agent.head(url);
    let req = apply_auth(req, &config.username, &config.password);

    match req.call() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            Ok(format!("连接成功！服务器响应 HTTP {}", status))
        }
        Err(e) => {
            // Some WebDAV servers reject HEAD with 405 Method Not Allowed, fallback to GET
            let req_get = agent.get(url);
            let req_get = apply_auth(req_get, &config.username, &config.password);
            match req_get.call() {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    Ok(format!("连接成功！服务器响应 HTTP {}", status))
                }
                Err(err) => Err(PluginError::Execution(format!(
                    "连接失败: {} (HEAD 错误: {})",
                    err, e
                ))),
            }
        }
    }
}

/// Uploads the given document content as a backup to the cloud.
/// Checks for overwrite conflicts if the remote destination already exists with a different `doc_id`.
pub fn upload_backup(
    config: &CloudBackupConfig,
    target_url: &str,
    identity: &DocumentIdentity,
    content: &str,
) -> Result<String, PluginError> {
    let agent = build_http_agent();

    validate_endpoint(target_url)?;
    // A conditional PUT closes the GET/PUT race on servers implementing HTTP preconditions.
    // Fail closed for unavailable objects and for objects without a strong validator.
    let check_req = apply_auth(agent.get(target_url), &config.username, &config.password);
    let mut existing = match check_req.call() {
        Ok(resp) if resp.status().is_success() => Some(resp),
        Err(ureq::Error::StatusCode(404)) => None,
        _ => {
            return Err(PluginError::Execution(
                "无法安全检查云端目标，已拒绝覆盖".to_string(),
            ));
        }
    };
    let mut etag = None;
    if let Some(resp) = existing.as_mut() {
        etag = resp
            .headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            .map(str::to_owned);
        let body_str = Zeroizing::new(
            resp.body_mut()
                .read_to_string()
                .map_err(|e| PluginError::Execution(format!("读取已有云端备份失败: {}", e)))?,
        );
        let existing_envelope: BackupEnvelope = serde_json::from_str(&body_str).map_err(|_| {
            PluginError::Execution(
                "覆盖冲突警告：目标地址已存在无法识别的内容，已拒绝覆盖。请更换远程文件名。"
                    .to_string(),
            )
        })?;
        if existing_envelope.doc_id != identity.doc_id {
            return Err(PluginError::Execution(format!(
                "覆盖冲突警告：云端目标已存在且属于不同文档 (远端标识: {})，已阻止静默覆盖。请更换远程文件名。",
                existing_envelope.doc_id
            )));
        }
        if etag.is_none() {
            return Err(PluginError::Execution("服务器未提供强 ETag，无法安全覆盖已有备份；请更换文件名或使用支持条件请求的 WebDAV 服务。".to_string()));
        }
    }

    let word_count = content.chars().count();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let payload = BackupPayload {
        version: 1,
        doc_id: identity.doc_id.clone(),
        // Absolute paths are local identity inputs, never remote metadata.
        source_path: None,
        filename: identity.filename.clone(),
        content: content.to_string(),
        timestamp: timestamp.clone(),
        word_count,
    };

    let envelope = if config.enable_encryption {
        encrypt_payload(&payload, &config.encryption_password)?
    } else {
        BackupEnvelope {
            version: 1,
            doc_id: payload.doc_id.clone(),
            encrypted: false,
            salt: None,
            nonce: None,
            ciphertext: None,
            payload: Some(payload),
        }
    };

    let body_bytes = Zeroizing::new(
        serde_json::to_vec_pretty(&envelope)
            .map_err(|e| PluginError::Execution(format!("序列化备份信封失败: {}", e)))?,
    );

    let req = agent.put(target_url);
    let req = apply_auth(req, &config.username, &config.password);
    let req = req.header("Content-Type", "application/json");
    let req = if let Some(etag) = etag {
        req.header("If-Match", etag)
    } else {
        req.header("If-None-Match", "*")
    };

    let response = req.send(body_bytes.as_slice()).map_err(|e| match e {
        ureq::Error::StatusCode(412) => PluginError::Execution(
            "覆盖冲突：云端备份在检查后已变化，请重新拉取或更换远程文件名。".to_string(),
        ),
        other => PluginError::Execution(format!("上传到云端失败 ({}): {}", target_url, other)),
    })?;

    let status = response.status();
    if status.is_success() {
        let enc_label = if config.enable_encryption {
            "已加密"
        } else {
            "未加密"
        };
        Ok(format!(
            "云端备份成功！\n目标: {}\n时间: {}\n大小: {} 字节 ({})",
            target_url,
            timestamp,
            body_bytes.len(),
            enc_label
        ))
    } else {
        Err(PluginError::Execution(format!(
            "服务器返回状态异常: HTTP {}",
            status
        )))
    }
}

/// Downloads and restores a backup from the cloud with strict format validation.
pub fn download_backup(
    config: &CloudBackupConfig,
    target_url: &str,
) -> Result<BackupPayload, PluginError> {
    validate_endpoint(target_url)?;
    let agent = build_http_agent();
    let req = agent.get(target_url);
    let req = apply_auth(req, &config.username, &config.password);

    let mut response = req.call().map_err(|e| {
        PluginError::Execution(format!("从云端拉取备份失败 ({}): {}", target_url, e))
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(PluginError::Execution(format!(
            "服务器返回状态异常: HTTP {}",
            status
        )));
    }

    let body_str = Zeroizing::new(
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| PluginError::Execution(format!("读取云端响应数据失败: {}", e)))?,
    );

    // Strictly validate Paper Shell BackupEnvelope
    let envelope: BackupEnvelope = serde_json::from_str(&body_str).map_err(|e| {
        PluginError::Execution(format!(
            "云端响应不是有效的 Paper Shell 备份格式（请检查文件路径或服务地址是否正确）: {}",
            e
        ))
    })?;

    decrypt_envelope(&envelope, &config.encryption_password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn credentials_are_session_only_and_debug_is_redacted() {
        let config = CloudBackupConfig {
            password: Zeroizing::new("webdav-secret".into()),
            encryption_password: Zeroizing::new("master-secret".into()),
            ..Default::default()
        };
        for encoded in [
            toml::to_string(&config).unwrap(),
            serde_json::to_string(&config).unwrap(),
            format!("{config:?}"),
        ] {
            assert!(!encoded.contains("webdav-secret"));
            assert!(!encoded.contains("master-secret"));
        }
        let legacy: CloudBackupConfig = toml::from_str(
            "password = 'webdav-secret'\nencryption_password = 'master-secret'\nenable_encryption = true"
        ).unwrap();
        assert!(legacy.password.is_empty());
        assert!(legacy.encryption_password.is_empty());
        assert!(legacy.enable_encryption);
        let saved = toml::to_string(&legacy).unwrap();
        assert!(!saved.contains("password"));
    }

    #[test]
    fn transport_warning_and_endpoint_validation() {
        for endpoint in [
            "http://example.com/dav",
            "http://192.168.1.1/",
            "http://localhost.evil/",
            "http://[::ffff:192.168.1.1]/",
        ] {
            assert!(insecure_http_endpoint(endpoint), "{endpoint}");
        }
        for endpoint in [
            "https://example.com/",
            "http://localhost/",
            "http://127.0.0.2/",
            "http://[::1]/",
        ] {
            assert!(!insecure_http_endpoint(endpoint), "{endpoint}");
            assert!(validate_endpoint(endpoint).is_ok());
        }
        for endpoint in [
            "ftp://example.com/",
            "https://user:secret@example.com/",
            "https://example.com/?token=secret",
            "https://example.com/#secret",
        ] {
            assert!(validate_endpoint(endpoint).is_err());
        }
    }

    #[test]
    fn legacy_v1_encrypted_backup_remains_readable() {
        // Construct the original v1 format independently of the v2 writer.
        let payload = BackupPayload {
            version: 1,
            doc_id: "legacy".into(),
            source_path: Some("/Users/old/doc.md".into()),
            filename: "doc.md".into(),
            content: "old content".into(),
            timestamp: "old".into(),
            word_count: 11,
        };
        let salt = [7u8; SALT_LEN];
        let nonce_bytes = [9u8; NONCE_LEN];
        let mut key = Zeroizing::new([0u8; 32]);
        pbkdf2_hmac::<Sha256>(b"old password", &salt, 100_000, &mut *key);
        let cipher = Aes256Gcm::new_from_slice(&*key).unwrap();
        let nonce = Nonce::try_from(nonce_bytes.as_slice()).unwrap();
        let ciphertext = cipher
            .encrypt(&nonce, serde_json::to_vec(&payload).unwrap().as_slice())
            .unwrap();
        let envelope = BackupEnvelope {
            version: 1,
            doc_id: payload.doc_id.clone(),
            encrypted: true,
            salt: Some(BASE64.encode(salt)),
            nonce: Some(BASE64.encode(nonce_bytes)),
            ciphertext: Some(BASE64.encode(ciphertext)),
            payload: None,
        };
        assert_eq!(
            decrypt_envelope(&envelope, "old password").unwrap(),
            payload
        );
    }

    // A small HTTP fixture exercising actual GET and conditional PUT wire behavior.
    fn mock_upload(
        get_status: &str,
        get_body: &str,
        etag: Option<&str>,
        expected_condition: Option<&str>,
        put_status: &str,
    ) -> (String, std::thread::JoinHandle<Option<BackupEnvelope>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/backup", listener.local_addr().unwrap());
        let get_response = format!(
            "HTTP/1.1 {get_status}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{get_body}",
            get_body.len(),
            etag.map(|tag| format!("ETag: {tag}\r\n"))
                .unwrap_or_default()
        );
        let expected_condition = expected_condition.map(str::to_owned);
        let put_status = put_status.to_string();
        let server = std::thread::spawn(move || {
            let read_headers = |stream: &mut std::net::TcpStream| {
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                String::from_utf8(headers).unwrap()
            };
            let (mut stream, _) = listener.accept().unwrap();
            assert!(read_headers(&mut stream).starts_with("GET "));
            stream.write_all(get_response.as_bytes()).unwrap();
            drop(stream);
            let condition = expected_condition?;
            let (mut stream, _) = listener.accept().unwrap();
            let headers = read_headers(&mut stream).to_ascii_lowercase();
            assert!(headers.starts_with("put "));
            assert!(
                headers.contains(&condition.to_ascii_lowercase()),
                "{headers}"
            );
            let length: usize = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 {put_status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .unwrap();
            Some(serde_json::from_slice(&body).unwrap())
        });
        (url, server)
    }

    #[test]
    fn conditional_create_redacts_source_path() {
        let (url, server) = mock_upload(
            "404 Not Found",
            "",
            None,
            Some("if-none-match: *"),
            "201 Created",
        );
        let identity = DocumentIdentity::new(
            Some(Path::new("/Users/private-name/private-folder/doc.md")),
            "draft",
        );
        upload_backup(
            &CloudBackupConfig::default(),
            &url,
            &identity,
            "plain document",
        )
        .unwrap();
        let envelope = server.join().unwrap().unwrap();
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(!json.contains("private-name"));
        assert!(!json.contains("private-folder"));
        assert!(!json.contains("source_path"));
        assert_eq!(envelope.payload.unwrap().content, "plain document");
    }

    #[test]
    fn conditional_put_rejects_racing_create_and_update() {
        let identity = DocumentIdentity::new(None, "test");
        let existing = serde_json::to_string(&BackupEnvelope {
            version: 1,
            doc_id: identity.doc_id.clone(),
            encrypted: false,
            salt: None,
            nonce: None,
            ciphertext: None,
            payload: None,
        })
        .unwrap();
        for (status, body, etag, condition) in [
            ("404 Not Found", "", None, "if-none-match: *"),
            (
                "200 OK",
                existing.as_str(),
                Some("\"revision-1\""),
                "if-match: \"revision-1\"",
            ),
        ] {
            let (url, server) = mock_upload(
                status,
                body,
                etag,
                Some(condition),
                "412 Precondition Failed",
            );
            assert!(
                upload_backup(&CloudBackupConfig::default(), &url, &identity, "content").is_err()
            );
            assert!(server.join().unwrap().is_some());
        }
    }

    #[test]
    fn conditional_update_succeeds_with_strong_etag() {
        let identity = DocumentIdentity::new(None, "update");
        let existing = serde_json::to_string(&BackupEnvelope {
            version: 1,
            doc_id: identity.doc_id.clone(),
            encrypted: false,
            salt: None,
            nonce: None,
            ciphertext: None,
            payload: None,
        })
        .unwrap();
        let (url, server) = mock_upload(
            "200 OK",
            &existing,
            Some("\"revision-1\""),
            Some("if-match: \"revision-1\""),
            "200 OK",
        );
        upload_backup(&CloudBackupConfig::default(), &url, &identity, "updated").unwrap();
        assert_eq!(
            server.join().unwrap().unwrap().payload.unwrap().content,
            "updated"
        );
    }

    #[test]
    fn upload_fails_closed_without_strong_validator_or_successful_check() {
        let identity = DocumentIdentity::new(None, "test");
        let existing = serde_json::to_string(&BackupEnvelope {
            version: 1,
            doc_id: identity.doc_id.clone(),
            encrypted: false,
            salt: None,
            nonce: None,
            ciphertext: None,
            payload: None,
        })
        .unwrap();
        for (status, etag) in [
            ("200 OK", None),
            ("200 OK", Some("W/\"weak\"")),
            ("302 Found", None),
            ("403 Forbidden", None),
            ("500 Internal Server Error", None),
        ] {
            let (url, server) = mock_upload(status, &existing, etag, None, "200 OK");
            assert!(
                upload_backup(&CloudBackupConfig::default(), &url, &identity, "content").is_err()
            );
            assert!(server.join().unwrap().is_none());
        }
    }

    #[test]
    fn document_identity_separates_same_names_and_unsaved_drafts() {
        let first = DocumentIdentity::new(Some(Path::new("/tmp/a/report.md")), "draft-a");
        let same = DocumentIdentity::new(Some(Path::new("/tmp/a/report.md")), "draft-b");
        let other = DocumentIdentity::new(Some(Path::new("/tmp/b/report.md")), "draft-a");
        assert_eq!(first.filename, other.filename);
        assert_eq!(first.doc_id, same.doc_id);
        assert_ne!(first.doc_id, other.doc_id);

        let draft_a = DocumentIdentity::new(None, "unique-a");
        let draft_b = DocumentIdentity::new(None, "unique-b");
        assert_ne!(draft_a.doc_id, draft_b.doc_id);
        assert!(draft_a.doc_id.starts_with("draft-"));
    }

    #[test]
    fn test_encryption_and_decryption_roundtrip() {
        let payload = BackupPayload {
            version: 1,
            doc_id: "doc123".to_string(),
            source_path: Some("/tmp/my_article.md".to_string()),
            filename: "my_article.md".to_string(),
            content: "这是加密的测试正文。\nHello, encrypted world!".to_string(),
            timestamp: "2025-11-01 12:00:00".to_string(),
            word_count: 36,
        };

        let password = "super_secret_password_123!";
        let envelope = encrypt_payload(&payload, password).unwrap();
        assert!(envelope.encrypted);
        assert_eq!(envelope.version, 2);
        assert!(envelope.salt.is_some());
        assert!(envelope.nonce.is_some());
        assert!(envelope.ciphertext.is_some());
        assert!(envelope.payload.is_none());

        // Decrypt with correct password
        let decrypted = decrypt_envelope(&envelope, password).unwrap();
        assert_eq!(decrypted, payload);

        // Decrypt with wrong password
        let wrong_err = decrypt_envelope(&envelope, "wrong_password");
        assert!(wrong_err.is_err());

        // Decrypt with empty password
        let empty_err = decrypt_envelope(&envelope, "");
        assert!(empty_err.is_err());
    }

    #[test]
    fn test_unencrypted_envelope() {
        let payload = BackupPayload {
            version: 1,
            doc_id: "plain123".to_string(),
            source_path: None,
            filename: "plain.md".to_string(),
            content: "Plaintext content".to_string(),
            timestamp: "2025-11-01 12:00:00".to_string(),
            word_count: 17,
        };

        let envelope = BackupEnvelope {
            version: 1,
            doc_id: payload.doc_id.clone(),
            encrypted: false,
            salt: None,
            nonce: None,
            ciphertext: None,
            payload: Some(payload.clone()),
        };

        let result = decrypt_envelope(&envelope, "").unwrap();
        assert_eq!(result, payload);
    }

    #[test]
    fn test_strict_envelope_validation_rejects_html_and_bad_version() {
        let html_page = "<!DOCTYPE html><html><body>404 Not Found</body></html>";
        let config = CloudBackupConfig::default();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let html_owned = html_page.to_string();

        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::{Read, Write};
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html_owned.len(),
                    html_owned
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        let target_url = format!("http://127.0.0.1:{}/backup.json", port);
        let res = download_backup(&config, &target_url);
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("不是有效的 Paper Shell 备份格式")
        );

        // Bad envelope version
        let bad_envelope = BackupEnvelope {
            version: 99,
            doc_id: "bad-version".to_string(),
            encrypted: false,
            salt: None,
            nonce: None,
            ciphertext: None,
            payload: None,
        };
        assert!(decrypt_envelope(&bad_envelope, "").is_err());
    }

    #[test]
    fn test_resolve_target_url_encoding_and_identity() {
        // Normal filename
        let url = resolve_target_url(
            "https://dav.example.com/backup",
            "{stem}_{doc_id}.backup",
            "draft",
            "a1b2c3d4",
            "draft.md",
        )
        .unwrap();
        assert_eq!(url, "https://dav.example.com/backup/draft_a1b2c3d4.backup");

        // Trailing slash handled cleanly
        let url2 = resolve_target_url(
            "https://dav.example.com/backup/",
            "{stem}_{doc_id}.backup",
            "draft",
            "a1b2c3d4",
            "draft.md",
        )
        .unwrap();
        assert_eq!(url2, "https://dav.example.com/backup/draft_a1b2c3d4.backup");

        // Special characters: spaces, hashes, question marks, percent signs, and non-ASCII
        let url_special = resolve_target_url(
            "https://dav.example.com/backup/",
            "{stem}_{doc_id}.backup",
            "我的 报告#1? 100%",
            "ef56",
            "我的 报告#1? 100%.md",
        )
        .unwrap();
        assert!(url_special.contains("%20"));
        assert!(url_special.contains("%23"));
        assert!(url_special.contains("%3F"));
        assert!(url_special.contains("%25"));
        assert!(url_special.contains("%E6%88%91%E7%9A%84"));
        assert!(!url_special.contains('#'));
        assert!(!url_special.contains('?'));
    }

    #[test]
    fn upload_rejects_overwriting_a_different_document() {
        use std::io::{Read, Write};

        let existing = BackupEnvelope {
            version: 1,
            doc_id: "other-document".to_string(),
            encrypted: false,
            salt: None,
            nonce: None,
            ciphertext: None,
            payload: None,
        };
        let body = serde_json::to_string(&existing).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Consume the GET headers before closing to avoid a TCP reset
                // caused by dropping a socket with unread request data.
                let mut headers = Vec::new();
                let mut byte = [0u8; 1];
                while !headers.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                assert!(headers.starts_with(b"GET "));
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let identity = DocumentIdentity::new(Some(Path::new("/tmp/current.md")), "draft");
        let target_url = format!("http://127.0.0.1:{}/current.backup", port);
        let result = upload_backup(
            &CloudBackupConfig::default(),
            &target_url,
            &identity,
            "new content",
        );
        server.join().unwrap();
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("覆盖冲突"),
            "unexpected upload error: {error}"
        );
    }

    #[test]
    fn test_http_upload_and_download_roundtrip_with_chunked_read() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let stored_body: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        let stored_clone = stored_body.clone();
        std::thread::spawn(move || {
            // Read 2 requests: PUT then GET (plus GET conflict check in upload_backup)
            for _ in 0..3 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut header_buf = Vec::new();
                    let mut byte = [0u8; 1];
                    // Read until header boundary \r\n\r\n
                    while let Ok(1) = stream.read(&mut byte) {
                        header_buf.push(byte[0]);
                        if header_buf.ends_with(b"\r\n\r\n") {
                            break;
                        }
                    }
                    let header_str = String::from_utf8_lossy(&header_buf);

                    if header_str.starts_with("GET") {
                        let body = stored_clone.lock().unwrap().clone();
                        if body.is_empty() {
                            // File doesn't exist yet (for conflict check)
                            let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                            let _ = stream.write_all(resp.as_bytes());
                        } else {
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.write_all(&body);
                        }
                    } else if header_str.starts_with("PUT") {
                        // Parse Content-Length
                        let mut content_len = 0;
                        for line in header_str.lines() {
                            if line.to_ascii_lowercase().starts_with("content-length:")
                                && let Some(val) = line.split(':').nth(1)
                            {
                                content_len = val.trim().parse::<usize>().unwrap_or(0);
                            }
                        }

                        let mut body = vec![0u8; content_len];
                        let _ = stream.read_exact(&mut body);
                        *stored_clone.lock().unwrap() = body;

                        let resp =
                            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes());
                    }
                }
            }
        });

        let config = CloudBackupConfig {
            server_url: format!("http://127.0.0.1:{}/", port),
            username: "testuser".to_string(),
            password: Zeroizing::new("testpass".to_string()),
            remote_filename: "backup.json".to_string(),
            enable_encryption: true,
            encryption_password: Zeroizing::new("my_password".to_string()),
            last_backup_time: None,
            last_backup_status: None,
        };

        let target_url = format!("http://127.0.0.1:{}/backup.json", port);
        let identity = DocumentIdentity {
            filename: "doc.md".to_string(),
            stem: "doc".to_string(),
            doc_id: "doc123".to_string(),
            source_path: Some("/tmp/doc.md".to_string()),
        };
        let upload_result = upload_backup(
            &config,
            &target_url,
            &identity,
            "云端备份测试内容\nHello self-hosted cloud!",
        )
        .unwrap();
        assert!(upload_result.contains("云端备份成功"));

        let body = stored_body.lock().unwrap().clone();
        let envelope: BackupEnvelope = serde_json::from_slice(&body).unwrap();
        assert!(envelope.encrypted);
        assert!(envelope.payload.is_none());
        assert!(
            !String::from_utf8(body)
                .unwrap()
                .contains(config.encryption_password.as_str())
        );

        // Session secrets must be re-entered after reloading settings.
        let saved = toml::to_string(&config).unwrap();
        let mut reloaded: CloudBackupConfig = toml::from_str(&saved).unwrap();
        assert!(reloaded.encryption_password.is_empty());
        reloaded.encryption_password = config.encryption_password.clone();
        let downloaded = download_backup(&reloaded, &target_url).unwrap();
        assert_eq!(downloaded.source_path, None);
        assert_eq!(downloaded.filename, "doc.md");
        assert_eq!(downloaded.doc_id, "doc123");
        assert_eq!(
            downloaded.content,
            "云端备份测试内容\nHello self-hosted cloud!"
        );
    }
}
