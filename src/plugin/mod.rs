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

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Origin of a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PluginSource {
    Builtin,
    External,
}

impl std::fmt::Display for PluginSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin => write!(f, "内置"),
            Self::External => write!(f, "外部"),
        }
    }
}

/// Human-readable, display-oriented description of a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Whether this is a built-in or external plugin.
    pub source: PluginSource,
    /// Whether this plugin is currently enabled.
    pub enabled: bool,
    /// Whether this plugin has configurable settings.
    pub has_settings: bool,
    /// Directory of external plugin, if applicable.
    pub dir: Option<PathBuf>,
}

/// Information about a plugin that failed to discover or load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLoadError {
    /// Directory or file path that produced the error.
    pub path: PathBuf,
    /// Plugin ID if one could be extracted.
    pub id: Option<String>,
    /// Display name if one could be extracted.
    pub name: Option<String>,
    /// Human-readable explanation of why discovery or loading failed.
    pub error: String,
}

/// Everything a plugin needs to know about the current editing session.
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
    /// Resolved configuration settings for this plugin.
    pub settings: BTreeMap<String, String>,
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
pub trait Plugin: Send + Sync {
    /// Returns the plugin's display metadata.
    fn metadata(&self) -> PluginMetadata;

    /// The origin of this plugin.
    fn source(&self) -> PluginSource {
        PluginSource::Builtin
    }

    /// Declarative settings schema if this plugin provides one.
    fn settings_definitions(&self) -> Vec<external::SettingDefinition> {
        Vec::new()
    }

    /// Runs the plugin against the given context.
    fn run(&self, ctx: &PluginContext) -> Result<String, PluginError>;
}

/// Owns every plugin known to the application and provides lookup/listing.
pub struct PluginManager {
    plugins_dir: PathBuf,
    github_publish_config: builtin::github_publish::GithubPublishConfig,
    cloud_backup_config: builtin::cloud_backup::CloudBackupConfig,
    disabled_ids: BTreeSet<String>,
    external_settings: BTreeMap<String, BTreeMap<String, String>>,
    builtins: Vec<Arc<dyn Plugin>>,
    externals: Vec<Arc<dyn Plugin>>,
    load_errors: Vec<PluginLoadError>,
}

impl PluginManager {
    /// Builds the manager by registering all built-in plugins, loading external
    /// ones from `plugins_dir`, and filtering based on user enablement state.
    pub fn new(
        plugins_dir: PathBuf,
        github_publish: builtin::github_publish::GithubPublishConfig,
        cloud_backup: builtin::cloud_backup::CloudBackupConfig,
        disabled_plugins: Vec<String>,
        external_settings: BTreeMap<String, BTreeMap<String, String>>,
    ) -> Self {
        let mut manager = Self {
            plugins_dir,
            github_publish_config: github_publish,
            cloud_backup_config: cloud_backup,
            disabled_ids: disabled_plugins.into_iter().collect(),
            external_settings,
            builtins: Vec::new(),
            externals: Vec::new(),
            load_errors: Vec::new(),
        };
        manager.rescan();
        manager
    }

    /// Explicitly rescans the plugins directory and refreshes built-ins.
    pub fn rescan(&mut self) {
        self.builtins = builtin::builtin_plugins(
            self.github_publish_config.clone(),
            self.cloud_backup_config.clone(),
        );

        let discovery = external::discover(&self.plugins_dir, builtin::RESERVED_PLUGIN_IDS);
        self.externals = discovery.plugins;
        self.load_errors = discovery.errors;

        tracing::info!(
            "Loaded {} built-in plugin(s), {} external plugin(s), {} error(s)",
            self.builtins.len(),
            self.externals.len(),
            self.load_errors.len()
        );
    }

    /// Whether a plugin with the given ID is enabled.
    pub fn is_enabled(&self, id: &str) -> bool {
        !self.disabled_ids.contains(id)
    }

    /// Update the enabled state for a plugin ID.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if enabled {
            self.disabled_ids.remove(id);
        } else {
            self.disabled_ids.insert(id.to_string());
        }
    }

    /// Returns metadata for only enabled plugins (in stable order).
    pub fn enabled_metadata(&self) -> Vec<PluginMetadata> {
        self.all_metadata()
            .into_iter()
            .filter(|p| p.enabled)
            .collect()
    }

    /// Alias for `enabled_metadata()` to preserve existing callers.
    pub fn metadata(&self) -> Vec<PluginMetadata> {
        self.enabled_metadata()
    }

    /// Returns metadata for all known plugins (both enabled and disabled) in stable order.
    pub fn all_metadata(&self) -> Vec<PluginMetadata> {
        let mut list = Vec::with_capacity(self.builtins.len() + self.externals.len());

        for b in &self.builtins {
            let mut meta = b.metadata();
            meta.enabled = self.is_enabled(&meta.id);
            list.push(meta);
        }

        for e in &self.externals {
            let mut meta = e.metadata();
            meta.enabled = self.is_enabled(&meta.id);
            list.push(meta);
        }

        list
    }

    /// Returns any discovery or manifest load errors encountered.
    pub fn load_errors(&self) -> &[PluginLoadError] {
        &self.load_errors
    }

    /// Looks up an **enabled** plugin by its stable id.
    ///
    /// Disabled plugins return `None` to prevent execution.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        if !self.is_enabled(id) {
            return None;
        }
        self.find_any(id)
    }

    /// Looks up a plugin by its ID regardless of enabled status (useful for configuration UI).
    pub fn find_any(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        self.builtins
            .iter()
            .chain(self.externals.iter())
            .find(|p| p.metadata().id == id)
            .cloned()
    }

    /// Retrieves saved settings for an external plugin.
    pub fn external_settings_for(&self, id: &str) -> BTreeMap<String, String> {
        self.external_settings.get(id).cloned().unwrap_or_default()
    }

    /// Updates saved settings in memory for an external plugin.
    pub fn set_external_settings(&mut self, id: &str, settings: BTreeMap<String, String>) {
        if settings.is_empty() {
            self.external_settings.remove(id);
        } else {
            self.external_settings.insert(id.to_string(), settings);
        }
    }

    /// Update built-in plugin configuration and rebuild builtins.
    pub fn update_builtin_configs(
        &mut self,
        github: Option<builtin::github_publish::GithubPublishConfig>,
        cloud: Option<builtin::cloud_backup::CloudBackupConfig>,
    ) {
        if let Some(cfg) = github {
            self.github_publish_config = cfg;
        }
        if let Some(cfg) = cloud {
            self.cloud_backup_config = cfg;
        }
        self.builtins = builtin::builtin_plugins(
            self.github_publish_config.clone(),
            self.cloud_backup_config.clone(),
        );
    }

    /// Safely uninstalls an external plugin and rescans.
    ///
    /// Validates that the target directory is an immediate child of the plugins directory.
    pub fn safe_uninstall_external_plugin(&mut self, id: &str) -> Result<PathBuf, String> {
        let plugin = self
            .externals
            .iter()
            .find(|p| p.metadata().id == id)
            .ok_or_else(|| format!("未找到外部插件“{}”", id))?;

        let meta = plugin.metadata();
        let target_dir = meta
            .dir
            .as_ref()
            .ok_or_else(|| format!("插件“{}”无有效目录", id))?;

        let removed = external::uninstall_plugin_directory(&self.plugins_dir, target_dir)
            .map_err(|e| format!("卸载插件失败: {}", e))?;

        self.disabled_ids.remove(id);
        self.external_settings.remove(id);
        self.rescan();

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_plugin_cannot_be_retrieved_via_get() {
        let dir = std::env::temp_dir().join(format!("test_plugins_{}", uuid::Uuid::new_v4()));
        let mut manager = PluginManager::new(
            dir,
            builtin::github_publish::GithubPublishConfig::default(),
            builtin::cloud_backup::CloudBackupConfig::default(),
            vec!["print".to_string()],
            BTreeMap::new(),
        );

        assert!(!manager.is_enabled("print"));
        assert!(manager.is_enabled("cloud_backup"));
        assert!(manager.get("print").is_none());
        assert!(manager.find_any("print").is_some());

        // Re-enable
        manager.set_enabled("print", true);
        assert!(manager.is_enabled("print"));
        assert!(manager.get("print").is_some());
    }

    #[test]
    fn metadata_filtering_matches_enabled_state() {
        let dir = std::env::temp_dir().join(format!("test_plugins_{}", uuid::Uuid::new_v4()));
        let manager = PluginManager::new(
            dir,
            builtin::github_publish::GithubPublishConfig::default(),
            builtin::cloud_backup::CloudBackupConfig::default(),
            vec!["cloud_backup".to_string()],
            BTreeMap::new(),
        );

        let enabled = manager.enabled_metadata();
        assert!(!enabled.iter().any(|p| p.id == "cloud_backup"));
        assert!(enabled.iter().any(|p| p.id == "print"));

        let all = manager.all_metadata();
        let cloud = all.iter().find(|p| p.id == "cloud_backup").unwrap();
        assert!(!cloud.enabled);
        let print = all.iter().find(|p| p.id == "print").unwrap();
        assert!(print.enabled);
    }

    #[test]
    fn rescan_loads_new_external_plugins_and_uninstall_cleans_state() {
        let dir = std::env::temp_dir().join(format!("test_plugins_{}", uuid::Uuid::new_v4()));
        let mut manager = PluginManager::new(
            dir.clone(),
            builtin::github_publish::GithubPublishConfig::default(),
            builtin::cloud_backup::CloudBackupConfig::default(),
            Vec::new(),
            BTreeMap::new(),
        );
        assert_eq!(manager.all_metadata().len(), 3);

        let plugin_dir = dir.join("word-count");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            "id = 'word-count'\nname = 'Word count'\ncommand = 'wc'\n",
        )
        .unwrap();
        assert!(manager.find_any("word-count").is_none());

        manager.rescan();
        assert!(manager.get("word-count").is_some());
        manager.set_enabled("word-count", false);
        manager.set_external_settings(
            "word-count",
            BTreeMap::from([("mode".to_string(), "words".to_string())]),
        );
        assert!(manager.get("word-count").is_none());

        manager
            .safe_uninstall_external_plugin("word-count")
            .unwrap();
        assert!(!plugin_dir.exists());
        assert!(manager.find_any("word-count").is_none());
        assert!(manager.external_settings_for("word-count").is_empty());
        assert!(manager.is_enabled("word-count"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
