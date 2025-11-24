//! Application configuration module
//!
//! This module centralizes all application configuration settings using `confy`
//! for automatic serialization and OS-specific config directory management.
//!
//! # Configuration Storage
//!
//! Settings are automatically stored in OS-specific locations:
//! - macOS: `~/Library/Application Support/com.RetricSu.Paper-Shell/Paper Shell/config.toml`
//! - Linux: `~/.config/paper-shell/config.toml`
//! - Windows: `%APPDATA%\RetricSu\Paper Shell\config\config.toml`
//!
//! # Usage
//!
//! ```rust
//! // Load settings (creates default if doesn't exist)
//! let settings = Config::load()?;
//!
//! // Modify settings
//! let mut settings = settings;
//! settings.theme = "dark".to_string();
//!
//! // Save settings
//! settings.save()?;
//! ```

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Application name and metadata constants
const APP_QUALIFIER: &str = "com";
const APP_ORGANIZATION: &str = "RetricSu";
const APP_NAME: &str = "Paper Shell";

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error: {0}")]
    Confy(#[from] confy::ConfyError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// User settings that can be customized
///
/// This struct is automatically serialized to/from TOML using `confy`.
/// Add new settings fields here as the application grows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Application theme (for future use)
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Auto-save interval in seconds (0 = disabled)
    #[serde(default = "default_autosave_interval")]
    pub autosave_interval: u64,

    /// Font size (for future use)
    #[serde(default = "default_font_size")]
    pub font_size: f32,
}

// Default value functions for serde
fn default_theme() -> String {
    "light".to_string()
}

fn default_autosave_interval() -> u64 {
    300 // 5 minutes
}

fn default_font_size() -> f32 {
    14.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            autosave_interval: default_autosave_interval(),
            font_size: default_font_size(),
        }
    }
}

/// Main configuration interface
pub struct Config {
    #[allow(dead_code)]
    pub settings: Settings,
}

impl Config {
    /// Load configuration from disk, creating default if it doesn't exist
    pub fn load() -> Result<Self, ConfigError> {
        let settings: Settings = confy::load(APP_NAME, None)?;
        Ok(Self { settings })
    }

    /// Save current configuration to disk
    #[allow(dead_code)]
    pub fn save(&self) -> Result<(), ConfigError> {
        confy::store(APP_NAME, None, &self.settings)?;
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
}

impl Default for Config {
    fn default() -> Self {
        Self::load().unwrap_or_else(|_| Self {
            settings: Settings::default(),
        })
    }
}
