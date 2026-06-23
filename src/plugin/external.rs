//! External, user-installed plugins.
//!
//! An external plugin is a directory inside the application's `plugins/`
//! folder that contains a `plugin.toml` manifest and an executable or script.
//! This lets users write plugins in any language and install them simply by
//! dropping a folder into place — no recompilation of Paper Shell required.
//!
//! ## Manifest (`plugin.toml`)
//!
//! ```toml
//! id          = "word-count"
//! name        = "字数统计"
//! description = "Count words and print a report"
//! version     = "0.1.0"
//! author      = "you"
//! command     = "python3"
//! args        = ["main.py"]
//! ```
//!
//! ## Runtime protocol
//!
//! When the plugin runs, Paper Shell:
//! * sets the working directory to the plugin folder,
//! * pipes the current editor content to the process's **stdin**,
//! * exposes context through environment variables:
//!   * `PAPER_SHELL_FILE` — absolute path of the open file (empty if none),
//!   * `PAPER_SHELL_DATA_DIR` — application data directory,
//!   * `PAPER_SHELL_PLUGIN_DIR` — the plugin's own directory,
//! * treats **stdout** as the success message and a non-zero exit code as a
//!   failure (with **stderr** as the error message).

use super::{Plugin, PluginContext, PluginError, PluginMetadata};
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// File name every external plugin must provide to be discovered.
const MANIFEST_FILE: &str = "plugin.toml";

/// Deserialized form of a `plugin.toml` manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: String,
    /// The program to execute (e.g. `python3`, `node`, `./run.sh`).
    pub command: String,
    /// Arguments passed to `command`.
    #[serde(default)]
    pub args: Vec<String>,
}

fn default_version() -> String {
    "0.0.0".to_string()
}

/// A plugin backed by an external process described by a [`PluginManifest`].
pub struct ExternalPlugin {
    manifest: PluginManifest,
    dir: PathBuf,
}

/// Scans `plugins_dir` for valid external plugins.
///
/// Each immediate subdirectory containing a `plugin.toml` becomes a plugin.
/// A missing plugins directory is not an error — it simply yields no plugins.
/// Individual malformed manifests are logged and skipped so one bad plugin
/// can't hide the others.
pub fn discover(plugins_dir: &Path) -> std::io::Result<Vec<Arc<dyn Plugin>>> {
    let mut plugins: Vec<Arc<dyn Plugin>> = Vec::new();

    if !plugins_dir.exists() {
        return Ok(plugins);
    }

    for entry in std::fs::read_dir(plugins_dir)? {
        let dir = entry?.path();
        if !dir.is_dir() {
            continue;
        }

        let manifest_path = dir.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            continue;
        }

        match load_manifest(&manifest_path) {
            Ok(manifest) => plugins.push(Arc::new(ExternalPlugin { manifest, dir })),
            Err(e) => tracing::warn!("Invalid plugin manifest {:?}: {}", manifest_path, e),
        }
    }

    Ok(plugins)
}

fn load_manifest(path: &Path) -> Result<PluginManifest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

impl Plugin for ExternalPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            version: self.manifest.version.clone(),
            author: self.manifest.author.clone(),
        }
    }

    fn run(&self, ctx: &PluginContext) -> Result<String, PluginError> {
        let file = ctx
            .file_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut child = Command::new(&self.manifest.command)
            .args(&self.manifest.args)
            .current_dir(&self.dir)
            .env("PAPER_SHELL_FILE", file)
            .env("PAPER_SHELL_DATA_DIR", &ctx.data_dir)
            .env("PAPER_SHELL_PLUGIN_DIR", &self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                PluginError::Execution(format!("无法启动 `{}`：{}", self.manifest.command, e))
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(ctx.content.as_bytes());
        }

        let output = child
            .wait_with_output()
            .map_err(|e| PluginError::Execution(e.to_string()))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(PluginError::Execution(format!(
                "退出码 {:?}\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}
