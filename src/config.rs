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
        let settings: Settings = confy::load(APP_NAME, None)?;
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
        // Move the path to the front
        self.settings.recent_files.retain(|p| p != &path);
        self.settings.recent_files.insert(0, path);
        self.settings.recent_files.truncate(MAX_RECENT_FILES);

        // Save changes in background since it's synchronous IO
        let settings = self.settings.clone();
        std::thread::spawn(move || {
            if let Err(e) = confy::store(APP_NAME, None, &settings) {
                tracing::error!("Failed to save recent files: {}", e);
            }
        });
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

    /// AI Panel configuration
    #[serde(default)]
    pub ai_panel: AiPanelConfig,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            autosave_interval: 300, // 5 minutes
            font_size: 14.0,
            recent_files: Vec::new(),
            ai_panel: AiPanelConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPanelConfig {
    /// API key for AI service
    #[serde(default)]
    pub api_key: String,

    /// API URL for AI service
    #[serde(default)]
    pub api_url: String,

    /// Model name for AI service
    #[serde(default)]
    pub model_name: String,
}

impl Default for AiPanelConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_url: "https://generativelanguage.googleapis.com/v1beta/models/".to_string(),
            model_name: "gemini-2.5-flash-lite-preview-09-2025".to_string(),
        }
    }
}
