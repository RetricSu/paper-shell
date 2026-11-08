//! Updates consume the same archives and SHA256SUMS.txt published by release.yml.
//! Nothing is installed until the editor has resolved its unsaved changes.

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

pub const RELEASES_URL: &str = "https://github.com/RetricSu/paper-shell/releases/latest";
const RELEASE_API: &str = "https://api.github.com/repos/RetricSu/paper-shell/releases/latest";
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
const MAC_BINARY: &str = "Paper Shell.app/Contents/MacOS/paper-shell";

static RESTART: Mutex<Option<(PathBuf, Option<PathBuf>)>> = Mutex::new(None);

pub enum UpdateState {
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading { downloaded: u64, total: u64 },
    Ready(PreparedUpdate),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    pub notes_url: String,
    asset: Asset,
    checksums: Asset,
}

pub struct PreparedUpdate {
    pub version: Version,
    // Own the temporary directory for as long as the prepared update is offered.
    directory: tempfile::TempDir,
}

enum UpdateEvent {
    Checked(Result<Option<Release>, String>),
    Progress(u64, u64),
    Prepared(Result<PreparedUpdate, String>),
}

pub struct Updater {
    pub state: UpdateState,
    receiver: Option<Receiver<UpdateEvent>>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            state: UpdateState::Idle,
            receiver: None,
        }
    }
}

impl Updater {
    pub fn has_update(&self) -> bool {
        matches!(
            self.state,
            UpdateState::Available(_) | UpdateState::Downloading { .. } | UpdateState::Ready(_)
        )
    }

    pub fn check(&mut self, ctx: &egui::Context) {
        if self.receiver.is_some() || matches!(self.state, UpdateState::Ready(_)) {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.state = UpdateState::Checking;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(UpdateEvent::Checked(check_latest()));
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        });
    }

    pub fn download(&mut self, ctx: &egui::Context) {
        let UpdateState::Available(release) = &self.state else {
            return;
        };
        let release = release.clone();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.state = UpdateState::Downloading {
            downloaded: 0,
            total: release.asset.size,
        };
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = prepare_update(&release, |downloaded, total| {
                let _ = sender.send(UpdateEvent::Progress(downloaded, total));
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            });
            let _ = sender.send(UpdateEvent::Prepared(result));
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        });
    }

    pub fn poll(&mut self) {
        while let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(UpdateEvent::Checked(result)) => {
                    self.state = match result {
                        Ok(Some(release)) => UpdateState::Available(release),
                        Ok(None) => UpdateState::Current,
                        Err(error) => UpdateState::Failed(error),
                    };
                    self.receiver = None;
                }
                Ok(UpdateEvent::Prepared(result)) => {
                    self.state = match result {
                        Ok(update) => UpdateState::Ready(update),
                        Err(error) => UpdateState::Failed(error),
                    };
                    self.receiver = None;
                }
                Ok(UpdateEvent::Progress(downloaded, total)) => {
                    self.state = UpdateState::Downloading { downloaded, total };
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.state = UpdateState::Failed("更新任务意外结束，请重试。".into());
                    self.receiver = None;
                }
            }
        }
    }

    /// Called only after the existing save/discard/cancel flow has completed.
    pub fn install(&mut self, current_file: Option<PathBuf>) -> bool {
        let UpdateState::Ready(update) = &self.state else {
            return false;
        };
        let result = install_prepared(update).and_then(|executable| {
            *RESTART.lock().map_err(|e| e.to_string())? = Some((executable, current_file));
            Ok(())
        });
        match result {
            Ok(()) => true,
            Err(error) => {
                self.state = UpdateState::Failed(format!("安装失败：{error}"));
                false
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

fn asset_name(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("macos", "aarch64" | "x86_64") => Ok("Paper-Shell-macOS-universal.zip"),
        ("linux", "x86_64") => Ok("Paper-Shell-Linux-x86_64.tar.gz"),
        ("windows", "x86_64") => Ok("Paper-Shell-Windows-x86_64.zip"),
        _ => Err("当前平台没有自动更新包，请前往发布页面下载。".into()),
    }
}

fn select_release(
    release: GithubRelease,
    current: &str,
    os: &str,
    arch: &str,
) -> Result<Option<Release>, String> {
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| "发布版本号无效。".to_string())?;
    let current = Version::parse(current).map_err(|e| e.to_string())?;
    if release.draft
        || release.prerelease
        || !version.pre.is_empty()
        || !version.cmp_precedence(&current).is_gt()
    {
        return Ok(None);
    }
    let find_asset = |name: &str| -> Result<Asset, String> {
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .ok_or_else(|| format!("发布包尚未完整上传（缺少 {name}），请稍后重试。"))?;
        // Never follow a release-supplied URL to a different repository or host.
        let expected = format!(
            "https://github.com/RetricSu/paper-shell/releases/download/{}/{name}",
            release.tag_name
        );
        if asset.browser_download_url != expected || asset.size == 0 || asset.size > MAX_DOWNLOAD {
            return Err(format!("发布附件无效：{name}"));
        }
        Ok(asset.clone())
    };
    Ok(Some(Release {
        version,
        notes_url: format!(
            "https://github.com/RetricSu/paper-shell/releases/tag/{}",
            release.tag_name
        ),
        asset: find_asset(asset_name(os, arch)?)?,
        checksums: find_asset("SHA256SUMS.txt")?,
    }))
}

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(timeout)))
            .https_only(true)
            .user_agent(format!("paper-shell/{}", env!("CARGO_PKG_VERSION")))
            .build(),
    )
}

fn check_latest() -> Result<Option<Release>, String> {
    let mut response = match agent(25)
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(ureq::Error::StatusCode(403 | 429)) => {
            return Err("GitHub 请求受限，请稍后重试。".into());
        }
        Err(error) => return Err(format!("无法检查更新：{error}")),
    };
    let body = response
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("无法读取发布信息：{e}"))?;
    let release = serde_json::from_str(&body).map_err(|e| format!("发布信息无效：{e}"))?;
    select_release(
        release,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

fn expected_checksum(text: &str, filename: &str) -> Result<String, String> {
    let mut found = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(hash), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        if name.trim_start_matches('*') == filename {
            if found.is_some()
                || fields.next().is_some()
                || hash.len() != 64
                || !hash.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("更新包校验记录无效。".into());
            }
            found = Some(hash.to_ascii_lowercase());
        }
    }
    found.ok_or_else(|| "校验文件中缺少当前更新包。".into())
}

fn prepare_update(
    release: &Release,
    mut progress: impl FnMut(u64, u64),
) -> Result<PreparedUpdate, String> {
    // Detect unpackaged macOS builds before spending bandwidth on the archive.
    let _ = installation_target()?;
    let result = (|| -> Result<PreparedUpdate, Box<dyn std::error::Error>> {
        let agent = agent(300);
        let checksums = agent
            .get(&release.checksums.browser_download_url)
            .call()?
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()?;
        let expected = expected_checksum(&checksums, &release.asset.name)?;
        let directory = tempfile::Builder::new()
            .prefix("paper-shell-update-")
            .tempdir()?;
        let archive_path = directory.path().join(&release.asset.name);
        let mut archive = File::create(&archive_path)?;
        let mut response = agent.get(&release.asset.browser_download_url).call()?;
        let mut reader = response
            .body_mut()
            .with_config()
            .limit(MAX_DOWNLOAD + 1)
            .reader();
        download_verified(
            &mut reader,
            &mut archive,
            release.asset.size,
            &expected,
            &mut progress,
        )?;
        archive.sync_all()?;
        drop(archive);
        let payload = directory.path().join("payload");
        fs::create_dir(&payload)?;
        extract_archive(&archive_path, &payload, &release.asset.name)?;
        validate_payload(&payload, &release.version)?;
        Ok(PreparedUpdate {
            version: release.version.clone(),
            directory,
        })
    })();
    result.map_err(|error| format!("下载更新失败：{error}"))
}

fn download_verified(
    reader: impl Read,
    mut output: impl Write,
    size: u64,
    expected: &str,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = reader.take(size + 1);
    let mut hasher = Sha256::new();
    let mut downloaded = 0;
    let mut last_progress = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        downloaded += count as u64;
        if downloaded > size {
            return Err("更新包大小与发布信息不匹配。".into());
        }
        output.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        if downloaded - last_progress >= 1024 * 1024 {
            progress(downloaded, size);
            last_progress = downloaded;
        }
    }
    let actual = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if downloaded != size || actual != expected {
        return Err("更新包校验失败，未修改已安装的应用。".into());
    }
    progress(downloaded, size);
    Ok(())
}

fn safe_archive_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn extract_archive(
    archive: &Path,
    destination: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(File::open(archive)?)?;
        let mut expanded: u64 = 0;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let entry_name = entry.name()?;
            if !safe_archive_path(Path::new(entry_name.as_ref()))
                || entry_name.contains(['\\', ':', '\0'])
            {
                return Err("更新包包含不安全的路径。".into());
            }
            let path = entry
                .enclosed_name()
                .filter(|p| safe_archive_path(p))
                .ok_or("更新包包含不安全的路径。")?;
            if entry.is_symlink() {
                return Err("更新包包含符号链接。".into());
            }
            expanded = expanded
                .checked_add(entry.size())
                .filter(|n| *n <= MAX_EXPANDED)
                .ok_or("更新包解压后过大。")?;
            let output = destination.join(path);
            if entry.is_dir() {
                fs::create_dir_all(&output)?;
            } else {
                fs::create_dir_all(output.parent().ok_or("更新包路径无效。")?)?;
                // create_new also rejects duplicate entries and file/directory collisions.
                let mut file = File::options().write(true).create_new(true).open(&output)?;
                std::io::copy(&mut entry, &mut file)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        &output,
                        fs::Permissions::from_mode(entry.unix_mode().unwrap_or(0o644) & 0o777),
                    )?;
                }
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?));
        let mut found = false;
        for entry in archive.entries()? {
            let mut entry = entry?;
            if entry.path()?.as_ref() != Path::new("paper-shell")
                || !entry.header().entry_type().is_file()
                || found
                || entry.size() > MAX_EXPANDED
            {
                return Err("Linux 更新包内容无效。".into());
            }
            entry.unpack(destination.join("paper-shell"))?;
            found = true;
        }
        if !found {
            return Err("更新包中缺少可执行文件。".into());
        }
    }
    Ok(())
}

fn validate_payload(payload: &Path, _version: &Version) -> Result<(), Box<dyn std::error::Error>> {
    let binary = if cfg!(target_os = "macos") {
        MAC_BINARY
    } else if cfg!(windows) {
        "paper-shell.exe"
    } else {
        "paper-shell"
    };
    if fs::metadata(payload.join(binary))?.len() == 0 {
        return Err("更新包中缺少可执行文件。".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(payload.join(binary), fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(target_os = "macos")]
    {
        let bundle = payload.join("Paper Shell.app");
        let plist = bundle.join("Contents/Info.plist");
        for (key, expected) in [
            ("CFBundleIdentifier", "paper.shell.app".to_string()),
            ("CFBundleShortVersionString", _version.to_string()),
        ] {
            let output = std::process::Command::new("/usr/libexec/PlistBuddy")
                .args(["-c", &format!("Print :{key}")])
                .arg(&plist)
                .output()?;
            if !output.status.success()
                || String::from_utf8_lossy(&output.stdout).trim() != expected
            {
                return Err("更新包的应用标识或版本不匹配。".into());
            }
        }
        let output = std::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&bundle)
            .output()?;
        if !output.status.success() {
            return Err("更新包的应用签名校验失败。".into());
        }
    }
    Ok(())
}

fn installation_target() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let bundle = executable
            .parent()
            .filter(|p| p.ends_with("Contents/MacOS"))
            .and_then(Path::parent)
            .and_then(Path::parent)
            .filter(|p| p.extension().is_some_and(|ext| ext == "app"))
            .ok_or("请将发布版 Paper Shell.app 安装到可写目录后再更新。")?;
        Ok(bundle.to_path_buf())
    }
    #[cfg(not(target_os = "macos"))]
    Ok(executable)
}

fn install_prepared(update: &PreparedUpdate) -> Result<PathBuf, String> {
    let target = installation_target()?;
    let payload = update.directory.path().join("payload");
    #[cfg(target_os = "macos")]
    {
        // Stage beside the installed app, so the final renames stay on one filesystem.
        let stage = tempfile::Builder::new()
            .prefix(".paper-shell-update-")
            .tempdir_in(target.parent().ok_or("应用目录无效。")?)
            .map_err(|e| format!("应用目录不可写，请将应用移出磁盘映像或手动安装：{e}"))?;
        let replacement = stage.path().join("Paper Shell.app");
        copy_bundle(&payload.join("Paper Shell.app"), &replacement).map_err(|e| e.to_string())?;
        // The payload has passed checksum and code-signature validation. Do not make
        // users repeat the browser-download Gatekeeper prompt for every update.
        clear_quarantine(&replacement)?;
        replace_bundle(&replacement, &target, stage)?;
        Ok(target.join("Contents/MacOS/paper-shell"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let binary = payload.join(if cfg!(windows) {
            "paper-shell.exe"
        } else {
            "paper-shell"
        });
        self_replace::self_replace(binary).map_err(|e| e.to_string())?;
        Ok(target)
    }
}

#[cfg(any(target_os = "macos", test))]
fn replace_bundle(
    replacement: &Path,
    target: &Path,
    stage: tempfile::TempDir,
) -> Result<(), String> {
    let backup = stage.path().join("previous.app");
    fs::rename(target, &backup).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(replacement, target) {
        if let Err(rollback) = fs::rename(&backup, target) {
            let kept = stage.keep();
            return Err(format!(
                "{error}；恢复失败：{rollback}。旧应用保留在 {}",
                kept.join("previous.app").display()
            ));
        }
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_bundle(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let output = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_bundle(&entry.path(), &output)?;
        } else {
            fs::copy(entry.path(), output)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn clear_quarantine(bundle: &Path) -> Result<(), String> {
    const QUARANTINE: &str = "com.apple.quarantine";
    let attributes = xattr::list(bundle).map_err(|e| format!("无法读取应用隔离属性：{e}"))?;
    if attributes.into_iter().any(|name| name == QUARANTINE) {
        xattr::remove(bundle, QUARANTINE).map_err(|e| format!("无法清除应用隔离属性：{e}"))?;
    }
    Ok(())
}

/// Run after eframe returns: all editor windows and persistence hooks have closed.
pub fn restart_if_requested() {
    let Some((executable, file)) = RESTART.lock().ok().and_then(|mut value| value.take()) else {
        return;
    };
    let mut command = std::process::Command::new(executable);
    if let Some(file) = file {
        command.arg(file);
    }
    if let Err(error) = command.spawn() {
        rfd::MessageDialog::new()
            .set_title("更新已安装")
            .set_description(format!("无法重新启动纸壳，请手动打开应用。\n{error}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn release(tag: &str) -> GithubRelease {
        let names = [
            "Paper-Shell-macOS-universal.zip",
            "Paper-Shell-Windows-x86_64.zip",
            "Paper-Shell-Linux-x86_64.tar.gz",
            "SHA256SUMS.txt",
        ];
        GithubRelease {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
            assets: names
                .into_iter()
                .map(|name| Asset {
                    name: name.into(),
                    size: 1024,
                    browser_download_url: format!(
                        "https://github.com/RetricSu/paper-shell/releases/download/{tag}/{name}"
                    ),
                })
                .collect(),
        }
    }

    #[test]
    fn versions_compare_numerically_and_never_downgrade_or_install_prereleases() {
        let update = select_release(release("v0.1.10"), "0.1.9", "macos", "aarch64")
            .unwrap()
            .unwrap();
        assert_eq!(update.version, Version::new(0, 1, 10));
        for tag in ["v0.1.9", "v0.1.8", "v0.1.9+rebuild", "v0.2.0-rc.1"] {
            assert!(
                select_release(release(tag), "0.1.9", "linux", "x86_64")
                    .unwrap()
                    .is_none()
            );
        }
        let mut draft = release("v0.2.0");
        draft.draft = true;
        assert!(
            select_release(draft, "0.1.9", "windows", "x86_64")
                .unwrap()
                .is_none()
        );
        let mut prerelease = release("v0.2.0");
        prerelease.prerelease = true;
        assert!(
            select_release(prerelease, "0.1.9", "windows", "x86_64")
                .unwrap()
                .is_none()
        );
        assert!(select_release(release("nightly"), "0.1.9", "macos", "aarch64").is_err());
    }

    #[test]
    fn platform_assets_match_the_release_workflow() {
        let workflow = include_str!("../.github/workflows/release.yml");
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("linux", "x86_64"),
            ("windows", "x86_64"),
        ] {
            let name = asset_name(os, arch).unwrap();
            assert!(workflow.contains(name), "missing {name} in release.yml");
            let selected = select_release(release("v1.0.0"), "0.1.9", os, arch)
                .unwrap()
                .unwrap();
            assert_eq!(selected.asset.name, name);
        }
        assert!(asset_name("linux", "aarch64").is_err());
    }

    #[test]
    fn incomplete_or_foreign_release_assets_are_rejected() {
        let mut missing = release("v1.0.0");
        missing.assets.pop();
        assert!(select_release(missing, "0.1.9", "macos", "aarch64").is_err());
        let mut foreign = release("v1.0.0");
        foreign.assets[0].browser_download_url = "https://example.com/update.zip".into();
        assert!(select_release(foreign, "0.1.9", "macos", "aarch64").is_err());
    }

    #[test]
    fn checksum_record_must_be_unambiguous_and_for_the_exact_asset() {
        let hash = "AB".repeat(32);
        assert_eq!(
            expected_checksum(&format!("{hash}  app.zip\n"), "app.zip").unwrap(),
            hash.to_lowercase()
        );
        assert!(expected_checksum(&format!("{hash}  another-app.zip\n"), "app.zip").is_err());
        assert!(expected_checksum("xyz  app.zip\n", "app.zip").is_err());
        assert!(
            expected_checksum(&format!("{hash}  app.zip\n{hash}  app.zip\n"), "app.zip").is_err()
        );
    }

    #[test]
    fn download_verifies_hash_and_length_before_extraction() {
        let data = b"verified release archive";
        let hash = Sha256::digest(data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut output = Vec::new();
        download_verified(
            Cursor::new(data),
            &mut output,
            data.len() as u64,
            &hash,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(output, data);
        for (size, expected) in [
            (data.len() as u64 + 1, hash.as_str()),
            (data.len() as u64 - 1, hash.as_str()),
            (data.len() as u64, "wrong hash"),
        ] {
            assert!(
                download_verified(Cursor::new(data), Vec::new(), size, expected, |_, _| {})
                    .is_err()
            );
        }
    }

    #[test]
    fn download_via_ureq_stream_succeeds_when_body_equals_asset_size() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let data = b"verified release archive payload";
        let hash = Sha256::digest(data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let size = data.len() as u64;

        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut header_buf = Vec::new();
                let mut byte = [0u8; 1];
                while let Ok(1) = stream.read(&mut byte) {
                    header_buf.push(byte[0]);
                    if header_buf.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(resp.as_bytes()).unwrap();
                stream.write_all(data).unwrap();
            }
        });

        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .build(),
        );

        // 1. Confirm that limiting to exact size reproduces the bug:
        // ureq's LimitReader returns BodyExceedsLimit on the EOF read.
        let mut response_exact = agent
            .get(format!("http://127.0.0.1:{port}/asset.zip"))
            .call()
            .unwrap();
        let mut reader_exact = response_exact.body_mut().with_config().limit(size).reader();
        let err =
            download_verified(&mut reader_exact, Vec::new(), size, &hash, |_, _| {}).unwrap_err();
        assert!(
            err.to_string()
                .contains("the response body is larger than request limit"),
            "expected limit error, got: {err}"
        );

        // 2. Confirm that limit(MAX_DOWNLOAD + 1) fixes the issue:
        let mut response_fixed = agent
            .get(format!("http://127.0.0.1:{port}/asset.zip"))
            .call()
            .unwrap();
        let mut reader_fixed = response_fixed
            .body_mut()
            .with_config()
            .limit(MAX_DOWNLOAD + 1)
            .reader();
        let mut output = Vec::new();
        download_verified(&mut reader_fixed, &mut output, size, &hash, |_, _| {}).unwrap();
        assert_eq!(output, data);
        server.join().unwrap();
    }

    fn make_zip(path: &Path, name: &str, symlink: bool) {
        let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        if symlink {
            archive.add_symlink(name, "/tmp/outside", options).unwrap();
        } else {
            archive.start_file(name, options).unwrap();
            archive.write_all(b"new executable").unwrap();
        }
        archive.finish().unwrap();
    }

    #[test]
    fn zip_extracts_app_paths_but_rejects_traversal_and_symlinks() {
        for (name, symlink, valid) in [
            (MAC_BINARY, false, true),
            ("../escaped", false, false),
            ("/absolute", false, false),
            ("linked", true, false),
        ] {
            let root = tempfile::tempdir().unwrap();
            let archive = root.path().join("app.zip");
            let payload = root.path().join("payload");
            fs::create_dir(&payload).unwrap();
            make_zip(&archive, name, symlink);
            let result = extract_archive(&archive, &payload, "app.zip");
            assert_eq!(result.is_ok(), valid, "{name}: {result:?}");
            if valid {
                assert_eq!(
                    fs::read(payload.join(MAC_BINARY)).unwrap(),
                    b"new executable"
                );
            }
            assert!(!root.path().join("escaped").exists());
        }
    }

    #[test]
    fn linux_tar_extracts_only_the_expected_regular_binary() {
        for name in ["paper-shell", "unexpected"] {
            let root = tempfile::tempdir().unwrap();
            let archive_path = root.path().join("app.tar.gz");
            let encoder = flate2::write::GzEncoder::new(
                File::create(&archive_path).unwrap(),
                flate2::Compression::default(),
            );
            let mut archive = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o755);
            header.set_cksum();
            archive.append_data(&mut header, name, &b"app"[..]).unwrap();
            archive.into_inner().unwrap().finish().unwrap();
            let payload = root.path().join("payload");
            fs::create_dir(&payload).unwrap();
            assert_eq!(
                extract_archive(&archive_path, &payload, "app.tar.gz").is_ok(),
                name == "paper-shell"
            );
        }
    }

    #[test]
    fn bundle_replacement_restores_original_if_final_rename_fails() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Paper Shell.app");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("version"), "old").unwrap();
        let stage = tempfile::tempdir_in(root.path()).unwrap();
        let missing = stage.path().join("missing.app");
        assert!(replace_bundle(&missing, &target, stage).is_err());
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "old");
        let stage = tempfile::tempdir_in(root.path()).unwrap();
        let replacement = stage.path().join("new.app");
        fs::create_dir(&replacement).unwrap();
        fs::write(replacement.join("version"), "new").unwrap();
        replace_bundle(&replacement, &target, stage).unwrap();
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "new");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_release_zip_retains_valid_bundle_signature_after_extraction_and_copy() {
        fn command_ok(command: &mut std::process::Command) {
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("Paper Shell.app");
        fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        fs::copy("/bin/echo", bundle.join("Contents/MacOS/paper-shell")).unwrap();
        fs::write(
            bundle.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>paper.shell.app</string>
<key>CFBundleExecutable</key><string>paper-shell</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.0.0</string>
</dict></plist>"#,
        )
        .unwrap();
        command_ok(
            std::process::Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&bundle),
        );
        let archive = root.path().join("release.zip");
        command_ok(
            std::process::Command::new("/usr/bin/ditto")
                .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
                .arg(&bundle)
                .arg(&archive),
        );
        let payload = root.path().join("payload");
        fs::create_dir(&payload).unwrap();
        extract_archive(&archive, &payload, "release.zip").unwrap();
        validate_payload(&payload, &Version::new(1, 0, 0)).unwrap();
        assert!(validate_payload(&payload, &Version::new(2, 0, 0)).is_err());
        let copied = root.path().join("copied");
        fs::create_dir(&copied).unwrap();
        copy_bundle(
            &payload.join("Paper Shell.app"),
            &copied.join("Paper Shell.app"),
        )
        .unwrap();
        validate_payload(&copied, &Version::new(1, 0, 0)).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_quarantine_removal_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("Paper Shell.app");
        fs::create_dir(&bundle).unwrap();
        xattr::set(&bundle, "com.apple.quarantine", b"test").unwrap();
        clear_quarantine(&bundle).unwrap();
        assert!(
            xattr::get(&bundle, "com.apple.quarantine")
                .unwrap()
                .is_none()
        );
        clear_quarantine(&bundle).unwrap();
    }

    #[test]
    fn controller_surfaces_background_failure_and_can_retry() {
        let (sender, receiver) = mpsc::channel();
        let mut updater = Updater {
            state: UpdateState::Checking,
            receiver: Some(receiver),
        };
        sender
            .send(UpdateEvent::Checked(Err("offline".into())))
            .unwrap();
        updater.poll();
        assert!(matches!(updater.state, UpdateState::Failed(ref message) if message == "offline"));
        assert!(updater.receiver.is_none());
    }
}
