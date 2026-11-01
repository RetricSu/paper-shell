//! Application configuration module
//!
//! This module centralizes all application configuration settings using `confy`
//! for automatic serialization and OS-specific config directory management.

use crate::constant::{APP_NAME, APP_ORGANIZATION, APP_QUALIFIER, MAX_RECENT_FILES};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error: {0}")]
    Confy(#[from] confy::ConfyError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Config {
    #[allow(dead_code)]
    pub settings: Settings,
}

impl Config {
    /// Load configuration from disk, creating default if it doesn't exist
    pub fn load() -> Result<Self, ConfigError> {
        let mut settings: Settings = confy::load(APP_NAME, None)?;
        settings.migrate_workspace_folders();
        settings.sanitize_typography();
        info!("Load config from {:?}", Self::config_path()?);
        Ok(Self { settings })
    }

    /// Save current configuration to disk
    #[allow(dead_code)]
    pub fn save(&self) -> Result<(), ConfigError> {
        confy::store(APP_NAME, None, &self.settings)?;
        info!("Save config to {:?}", Self::config_path()?);
        Ok(())
    }

    /// Get the application data directory
    /// Falls back to a local "data" directory if platform dirs are unavailable
    pub fn data_dir(&self) -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME) {
            proj_dirs.data_dir().to_path_buf()
        } else {
            PathBuf::from("data")
        }
    }

    /// Get the configuration file path
    #[allow(dead_code)]
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        Ok(confy::get_configuration_file_path(APP_NAME, None)?)
    }

    /// Add a file to the recent files list
    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.settings.add_recent_file(path);

        // Keep settings writes ordered: an older recent-file snapshot must not
        // overwrite a workspace selection saved immediately afterwards.
        if let Err(error) = self.save() {
            tracing::error!("Failed to save recent files: {}", error);
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::load().unwrap_or_else(|_| Self {
            settings: Settings::default(),
        })
    }
}

fn default_font_size() -> f32 {
    14.0
}

fn default_line_spacing() -> f32 {
    1.5
}

fn default_font_family() -> String {
    "Default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Application theme (for future use)
    #[serde(default)]
    pub theme: String,

    /// Auto-save interval in seconds (0 = disabled)
    #[serde(default)]
    pub autosave_interval: u64,

    /// Font size in points (e.g. 14.0, 16.0)
    #[serde(default = "default_font_size")]
    pub font_size: f32,

    /// Line spacing multiplier (e.g. 1.2, 1.5, 1.8, 2.0)
    #[serde(default = "default_line_spacing")]
    pub line_spacing: f32,

    /// Font family name (e.g. "Default", "PingFang SC", "Songti SC")
    #[serde(default = "default_font_family")]
    pub font_family: String,

    /// Recently opened file paths
    /// since the path is a string(heap data),
    /// Using fixed-size array won't make much difference on performance
    #[serde(default)]
    pub recent_files: Vec<PathBuf>,

    /// Folders displayed alongside the default workspace.
    #[serde(default)]
    pub workspace_folders: Vec<PathBuf>,
    /// Read the former single-folder setting without writing it back.
    #[serde(default, skip_serializing)]
    custom_workspace: Option<PathBuf>,

    /// GitHub publish plugin configuration
    #[serde(default)]
    pub github_publish: crate::plugin::builtin::github_publish::GithubPublishConfig,

    /// Cloud backup plugin configuration
    #[serde(default)]
    pub cloud_backup: crate::plugin::builtin::cloud_backup::CloudBackupConfig,

    /// Explicitly disabled plugin IDs
    #[serde(default)]
    pub disabled_plugins: Vec<String>,

    /// External plugin settings keyed by plugin ID and setting key
    #[serde(default)]
    pub external_plugin_settings:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            autosave_interval: 300, // 5 minutes
            font_size: default_font_size(),
            line_spacing: default_line_spacing(),
            font_family: default_font_family(),
            recent_files: Vec::new(),
            custom_workspace: None,
            workspace_folders: Vec::new(),
            github_publish: crate::plugin::builtin::github_publish::GithubPublishConfig::default(),
            cloud_backup: crate::plugin::builtin::cloud_backup::CloudBackupConfig::default(),
            disabled_plugins: Vec::new(),
            external_plugin_settings: std::collections::BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Sanitize typography settings so they remain within sensible ranges.
    pub fn sanitize_typography(&mut self) {
        if self.font_size < 8.0 || self.font_size > 72.0 {
            self.font_size = default_font_size();
        }
        if self.line_spacing < 0.5 || self.line_spacing > 5.0 {
            self.line_spacing = default_line_spacing();
        }
        if self.font_family.trim().is_empty() {
            self.font_family = default_font_family();
        }
    }

    /// Add a path to the front of recent files, deduplicate, and truncate to MAX_RECENT_FILES.
    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    /// Whether the plugin is currently enabled
    pub fn is_plugin_enabled(&self, id: &str) -> bool {
        !self.disabled_plugins.iter().any(|d| d == id)
    }

    /// Set enabled status for a plugin ID
    pub fn set_plugin_enabled(&mut self, id: &str, enabled: bool) {
        if enabled {
            self.disabled_plugins.retain(|d| d != id);
        } else if !self.disabled_plugins.iter().any(|d| d == id) {
            self.disabled_plugins.push(id.to_string());
            self.disabled_plugins.sort();
        }
    }

    /// Get saved external settings for a plugin
    pub fn get_external_settings(
        &self,
        plugin_id: &str,
    ) -> std::collections::BTreeMap<String, String> {
        self.external_plugin_settings
            .get(plugin_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Update saved external settings for a plugin
    pub fn set_external_settings(
        &mut self,
        plugin_id: &str,
        settings: std::collections::BTreeMap<String, String>,
    ) {
        if settings.is_empty() {
            self.external_plugin_settings.remove(plugin_id);
        } else {
            self.external_plugin_settings
                .insert(plugin_id.to_string(), settings);
        }
    }

    /// Clean up state when an external plugin is uninstalled
    pub fn remove_external_plugin_data(&mut self, plugin_id: &str) {
        self.disabled_plugins.retain(|d| d != plugin_id);
        self.external_plugin_settings.remove(plugin_id);
    }

    fn migrate_workspace_folders(&mut self) {
        if let Some(path) = self.custom_workspace.take()
            && !self.workspace_folders.contains(&path)
        {
            self.workspace_folders.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_keep_default_workspace_and_migrate_custom_folder() {
        let mut settings: Settings = toml::from_str("theme = 'light'\nrecent_files = ['draft.txt']\ncustom_workspace = '/tmp/articles'\nuse_custom_workspace = false").unwrap();
        settings.migrate_workspace_folders();
        assert_eq!(
            settings.workspace_folders,
            vec![PathBuf::from("/tmp/articles")]
        );
        assert_eq!(settings.recent_files, vec![PathBuf::from("draft.txt")]);
        settings.workspace_folders.push(PathBuf::from("/tmp/notes"));
        let serialized = toml::to_string(&settings).unwrap();
        assert!(!serialized.contains("custom_workspace"));
        let reloaded: Settings = toml::from_str(&serialized).unwrap();
        assert_eq!(reloaded.workspace_folders, settings.workspace_folders);
        let empty: Settings = toml::from_str("").unwrap();
        assert!(empty.workspace_folders.is_empty());
    }

    #[test]
    fn recent_files_caps_at_25_and_moves_duplicates_to_front() {
        assert_eq!(MAX_RECENT_FILES, 25);
        let mut settings = Settings::default();

        for i in 0..30 {
            settings.add_recent_file(PathBuf::from(format!("/tmp/doc_{}.txt", i)));
        }

        assert_eq!(settings.recent_files.len(), 25);
        // Most recent should be at index 0 (doc_29.txt)
        assert_eq!(settings.recent_files[0], PathBuf::from("/tmp/doc_29.txt"));
        // Oldest kept should be doc_5.txt
        assert_eq!(settings.recent_files[24], PathBuf::from("/tmp/doc_5.txt"));

        // Re-adding doc_10.txt should move it to the front without changing length
        settings.add_recent_file(PathBuf::from("/tmp/doc_10.txt"));
        assert_eq!(settings.recent_files.len(), 25);
        assert_eq!(settings.recent_files[0], PathBuf::from("/tmp/doc_10.txt"));
    }

    #[test]
    fn cloud_backup_settings_roundtrip() {
        let mut settings = Settings::default();
        assert!(!settings.cloud_backup.enable_encryption);
        assert_eq!(settings.cloud_backup.server_url, "");

        settings.cloud_backup.server_url = "https://dav.example.com/papershell/".to_string();
        settings.cloud_backup.enable_encryption = true;
        settings.cloud_backup.encryption_password = "test_pass".to_string();

        let serialized = toml::to_string(&settings).unwrap();
        assert!(!serialized.contains("test_pass"));
        let reloaded: Settings = toml::from_str(&serialized).unwrap();
        assert_eq!(
            reloaded.cloud_backup.server_url,
            "https://dav.example.com/papershell/"
        );
        assert!(reloaded.cloud_backup.enable_encryption);
        assert_eq!(reloaded.cloud_backup.encryption_password, "");
    }

    #[test]
    fn plugin_enable_disable_and_external_settings_roundtrip() {
        let mut settings = Settings::default();
        assert!(settings.is_plugin_enabled("print"));
        assert!(settings.is_plugin_enabled("custom_ext"));

        settings.set_plugin_enabled("print", false);
        assert!(!settings.is_plugin_enabled("print"));
        assert_eq!(settings.disabled_plugins, vec!["print"]);

        let mut ext_map = std::collections::BTreeMap::new();
        ext_map.insert("api_key".to_string(), "secret_123".to_string());
        settings.set_external_settings("custom_ext", ext_map);

        let serialized = toml::to_string(&settings).unwrap();
        let reloaded: Settings = toml::from_str(&serialized).unwrap();

        assert!(!reloaded.is_plugin_enabled("print"));
        assert!(reloaded.is_plugin_enabled("custom_ext"));
        assert_eq!(
            reloaded.get_external_settings("custom_ext").get("api_key"),
            Some(&"secret_123".to_string())
        );

        let mut modified = reloaded;
        modified.remove_external_plugin_data("custom_ext");
        assert!(modified.get_external_settings("custom_ext").is_empty());
    }

    #[test]
    fn typography_settings_roundtrip_and_defaults() {
        let mut settings = Settings::default();
        assert_eq!(settings.font_size, 14.0);
        assert_eq!(settings.line_spacing, 1.5);
        assert_eq!(settings.font_family, "Default");

        settings.font_size = 18.0;
        settings.line_spacing = 1.8;
        settings.font_family = "PingFang SC".to_string();

        let serialized = toml::to_string(&settings).unwrap();
        let reloaded: Settings = toml::from_str(&serialized).unwrap();
        assert_eq!(reloaded.font_size, 18.0);
        assert_eq!(reloaded.line_spacing, 1.8);
        assert_eq!(reloaded.font_family, "PingFang SC");

        // Deserializing empty string should use defaults
        let empty_reloaded: Settings = toml::from_str("").unwrap();
        assert_eq!(empty_reloaded.font_size, 14.0);
        assert_eq!(empty_reloaded.line_spacing, 1.5);
        assert_eq!(empty_reloaded.font_family, "Default");

        // Sanitizing out-of-range values restores defaults
        let mut corrupted = Settings {
            font_size: -5.0,
            line_spacing: 0.0,
            font_family: "   ".to_string(),
            ..Settings::default()
        };
        corrupted.sanitize_typography();
        assert_eq!(corrupted.font_size, 14.0);
        assert_eq!(corrupted.line_spacing, 1.5);
        assert_eq!(corrupted.font_family, "Default");
    }
}
