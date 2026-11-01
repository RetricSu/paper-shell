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

const PBKDF2_ROUNDS: u32 = 100_000;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudBackupConfig {
    /// Server URL (e.g. WebDAV endpoint: https://example.com/dav/papershell/ or http://192.168.1.100:8080/backups/)
    pub server_url: String,
    /// Username for HTTP Basic Auth (optional)
    pub username: String,
    /// Password or Bearer Token (optional)
    pub password: String,
    /// Remote filename template (e.g. "{stem}_{doc_id}.backup" or "{filename}.backup")
    pub remote_filename: String,
    /// Whether end-to-end encryption is enabled
    pub enable_encryption: bool,
    /// Encryption password / passphrase (session-only, not stored in plaintext on disk)
    #[serde(skip)]
    pub encryption_password: String,
    /// Last backup timestamp (for UI display)
    pub last_backup_time: Option<String>,
    /// Last backup status message
    pub last_backup_status: Option<String>,
}

impl Default for CloudBackupConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            username: String::new(),
            password: String::new(),
            remote_filename: "{stem}_{doc_id}.backup".to_string(),
            enable_encryption: false,
            encryption_password: String::new(),
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

    let mut base_url = Url::parse(server_url)
        .map_err(|e| PluginError::Config(format!("无效的服务器 URL: {}", e)))?;

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

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, PBKDF2_ROUNDS, &mut key);

    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| PluginError::Execution(format!("初始化 AES-GCM 密码器失败: {}", e)))?;

    let plaintext = serde_json::to_vec(payload)
        .map_err(|e| PluginError::Execution(format!("序列化备份数据失败: {}", e)))?;

    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|e| PluginError::Execution(format!("构造 Nonce 失败: {}", e)))?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|e| PluginError::Execution(format!("加密数据失败: {}", e)))?;

    Ok(BackupEnvelope {
        version: 1,
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
    if envelope.version != 1 {
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

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, PBKDF2_ROUNDS, &mut key);

    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| PluginError::Execution(format!("初始化 AES-GCM 密码器失败: {}", e)))?;

    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|e| PluginError::Execution(format!("构造 Nonce 失败: {}", e)))?;
    let decrypted = cipher.decrypt(&nonce, ciphertext.as_ref()).map_err(|_| {
        PluginError::Execution("解密失败：密码错误或云端备份数据已损坏".to_string())
    })?;

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
    let password = password.trim();

    if !username.is_empty() {
        let auth_str = format!("{}:{}", username, password);
        let encoded = BASE64.encode(auth_str);
        req = req.header("Authorization", format!("Basic {}", encoded));
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

    // Never silently overwrite a foreign or unrecognized remote object.
    let check_req = agent.get(target_url);
    let check_req = apply_auth(check_req, &config.username, &config.password);
    if let Ok(mut resp) = check_req.call()
        && resp.status().is_success()
    {
        let body_str = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| PluginError::Execution(format!("读取已有云端备份失败: {}", e)))?;
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
    }

    let word_count = content.chars().count();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let payload = BackupPayload {
        version: 1,
        doc_id: identity.doc_id.clone(),
        source_path: identity.source_path.clone(),
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

    let body_bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|e| PluginError::Execution(format!("序列化备份信封失败: {}", e)))?;

    let req = agent.put(target_url);
    let req = apply_auth(req, &config.username, &config.password);
    let req = req.header("Content-Type", "application/json");

    let response = req
        .send(&body_bytes)
        .map_err(|e| PluginError::Execution(format!("上传到云端失败 ({}): {}", target_url, e)))?;

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

    let body_str = response
        .body_mut()
        .read_to_string()
        .map_err(|e| PluginError::Execution(format!("读取云端响应数据失败: {}", e)))?;

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
            password: "testpass".to_string(),
            remote_filename: "backup.json".to_string(),
            enable_encryption: true,
            encryption_password: "my_password".to_string(),
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

        let downloaded = download_backup(&config, &target_url).unwrap();
        assert_eq!(downloaded.filename, "doc.md");
        assert_eq!(downloaded.doc_id, "doc123");
        assert_eq!(
            downloaded.content,
            "云端备份测试内容\nHello self-hosted cloud!"
        );
    }
}
