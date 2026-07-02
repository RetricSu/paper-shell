//! Plugin system for Paper Shell.
//!
//! The plugin system is designed around a single small contract — the [`Plugin`]
//! trait — so that the rest of the application never needs to know whether a
//! plugin is built into the binary or installed by the user from disk.
//!
//! There are two kinds of plugins:
//!
//! * **Built-in plugins** (see [`builtin`]) are written in Rust and compiled
//!   into the application. They get full, type-safe access to the host.
//! * **External plugins** (see [`external`]) are installed by the user as a
//!   directory containing a `plugin.toml` manifest plus an executable/script.
//!   They communicate with the host through a simple, language-agnostic
//!   protocol (environment variables + stdin/stdout), so users can write them
//!   in any language they like and drop them into the plugins directory.
//!
//! Both kinds are exposed uniformly through [`PluginManager`], which the UI
//! layer queries for the list of installed plugins and uses to run them.

pub mod builtin;
pub mod external;

use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Human-readable, display-oriented description of a plugin.
///
/// This is intentionally cheap to clone so the UI can hold a snapshot of every
/// installed plugin without borrowing the manager.
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Stable, unique identifier used to look the plugin up (e.g. `"github_publish"`).
    pub id: String,
    /// Display name shown in the menu.
    pub name: String,
    /// One-line description shown as a tooltip.
    pub description: String,
    /// Semantic version string.
    pub version: String,
    /// Plugin author.
    pub author: String,
}

/// Everything a plugin needs to know about the current editing session.
///
/// The context is a plain data snapshot taken on the UI thread and handed to
/// the plugin, which always runs on a background thread. This keeps plugins
/// decoupled from the live application state and safe to run concurrently.
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// Path of the file currently open in the editor, if any.
    pub file_path: Option<PathBuf>,
    /// Current editor content.
    pub content: String,
    /// Application data directory (useful for plugins that cache state).
    pub data_dir: PathBuf,
    pub title: Option<String>,
    pub description: Option<String>,
    pub collection: Option<String>,
    pub printer: Option<String>,
    pub print_margin_points: Option<u16>,
}

/// Errors a plugin can report while running.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("请先打开并保存一个文件，再运行该插件")]
    NoActiveFile,

    #[error("插件未正确配置：{0}")]
    Config(String),

    #[error("执行失败：{0}")]
    Execution(String),

    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
}

/// The contract every plugin implements.
///
/// Implementations must be `Send + Sync` because plugins are stored behind an
/// [`Arc`] and executed on background threads.
pub trait Plugin: Send + Sync {
    /// Returns the plugin's display metadata.
    fn metadata(&self) -> PluginMetadata;

    /// Runs the plugin against the given context.
    ///
    /// On success returns a human-readable message to show the user (e.g. a
    /// summary of what happened). Errors are surfaced to the user as-is.
    fn run(&self, ctx: &PluginContext) -> Result<String, PluginError>;
}

/// Owns every plugin known to the application and provides lookup/listing.
///
/// The manager is the single integration point for the rest of the app: the
/// title bar asks it for [`PluginManager::metadata`] to build the menu, and the
/// app asks it for [`PluginManager::get`] to execute a plugin by id.
pub struct PluginManager {
    plugins: Vec<Arc<dyn Plugin>>,
}

impl PluginManager {
    /// Builds the manager by registering all built-in plugins and discovering
    /// external ones from `plugins_dir`.
    ///
    /// Discovery failures are logged but never fatal — a broken plugin must not
    /// prevent the editor from starting.
    pub fn new(
        plugins_dir: PathBuf,
        github_publish: builtin::github_publish::GithubPublishConfig,
    ) -> Self {
        let mut plugins: Vec<Arc<dyn Plugin>> = builtin::builtin_plugins(github_publish);

        match external::discover(&plugins_dir) {
            Ok(mut external) => plugins.append(&mut external),
            Err(e) => tracing::warn!("Failed to discover external plugins: {}", e),
        }

        tracing::info!("Loaded {} plugin(s)", plugins.len());
        Self { plugins }
    }

    /// Returns a snapshot of every installed plugin's metadata, in menu order.
    pub fn metadata(&self) -> Vec<PluginMetadata> {
        self.plugins.iter().map(|p| p.metadata()).collect()
    }

    /// Looks up a plugin by its stable id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins
            .iter()
            .find(|p| p.metadata().id == id)
            .cloned()
    }
}
