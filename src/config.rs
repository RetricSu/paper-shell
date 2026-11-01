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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Application theme (for future use)
    #[serde(default)]
    pub theme: String,

    /// Auto-save interval in seconds (0 = disabled)
    #[serde(default)]
    pub autosave_interval: u64,

    /// Font size (for future use)
    #[serde(default)]
    pub font_size: f32,

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            autosave_interval: 300, // 5 minutes
            font_size: 14.0,
            recent_files: Vec::new(),
            custom_workspace: None,
            workspace_folders: Vec::new(),
            github_publish: crate::plugin::builtin::github_publish::GithubPublishConfig::default(),
            cloud_backup: crate::plugin::builtin::cloud_backup::CloudBackupConfig::default(),
        }
    }
}

impl Settings {
    /// Add a path to the front of recent files, deduplicate, and truncate to MAX_RECENT_FILES.
    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(MAX_RECENT_FILES);
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
}
