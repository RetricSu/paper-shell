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
        if settings.migrate_skin_market_source()
            && let Err(error) = confy::store(APP_NAME, None, &settings)
        {
            tracing::warn!("Could not persist skin market source migration: {error}");
        }
        info!("Load config from {:?}", Self::config_path()?);
        crate::backup_credentials::restore(&mut settings.cloud_backup);
        Ok(Self { settings })
    }

    pub fn save_cloud_backup(
        &mut self,
        config: crate::plugin::builtin::cloud_backup::CloudBackupConfig,
    ) -> Result<Option<String>, String> {
        let (saved, warning) = crate::backup_credentials::save(config, |new_config| {
            let previous = std::mem::replace(&mut self.settings.cloud_backup, new_config.clone());
            if let Err(error) = self.save() {
                self.settings.cloud_backup = previous;
                return Err(format!("无法保存云端备份设置：{error}"));
            }
            Ok(())
        })?;
        self.settings.cloud_backup = saved;
        Ok(warning)
    }

    /// Save current configuration to disk
    #[allow(dead_code)]
    pub fn save(&self) -> Result<(), ConfigError> {
        #[cfg(test)]
        return Ok(());

        #[cfg(not(test))]
        {
            confy::store(APP_NAME, None, &self.settings)?;
            info!("Save config to {:?}", Self::config_path()?);
            Ok(())
        }
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

#[allow(clippy::derivable_impls)]
impl Default for Config {
    fn default() -> Self {
        #[cfg(test)]
        {
            Self {
                settings: Settings::default(),
            }
        }
        #[cfg(not(test))]
        {
            Self::load().unwrap_or_else(|_| Self {
                settings: Settings::default(),
            })
        }
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

fn default_font_color() -> String {
    "Default".to_string()
}

fn default_current_skin() -> String {
    "classic_paper".to_string()
}

pub fn default_skin_market_repo() -> String {
    "RetricSu/paper-shell".to_string()
}

pub fn default_skin_market_branch() -> String {
    "master".to_string()
}

pub fn default_skin_market_folder() -> String {
    "skins".to_string()
}

pub const FONT_COLOR_PRESETS: &[(&str, &str, egui::Color32)] = &[
    ("墨黑", "#1A1A1A", egui::Color32::from_rgb(26, 26, 26)),
    ("深灰", "#4A4A4A", egui::Color32::from_rgb(74, 74, 74)),
    ("墨蓝", "#1E3A5F", egui::Color32::from_rgb(30, 58, 95)),
    ("苍绿", "#205040", egui::Color32::from_rgb(32, 80, 64)),
    ("赭红", "#7C2D24", egui::Color32::from_rgb(124, 45, 36)),
    ("棕褐", "#523624", egui::Color32::from_rgb(82, 54, 36)),
    ("紫檀", "#4A2E4B", egui::Color32::from_rgb(74, 46, 75)),
];

pub fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let s = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(egui::Color32::from_rgb(r, g, b))
    } else if s.len() == 8 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        let a = u8::from_str_radix(&s[6..8], 16).ok()?;
        Some(egui::Color32::from_rgba_unmultiplied(r, g, b, a))
    } else {
        None
    }
}

pub fn color_to_hex(color: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

pub fn font_color_display_name(hex: &str) -> &str {
    if hex == "Default" || hex.is_empty() {
        return "系统默认";
    }
    for (name, preset_hex, _) in FONT_COLOR_PRESETS {
        if hex.eq_ignore_ascii_case(preset_hex) {
            return name;
        }
    }
    hex
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

    /// Whether one-click paragraph formatting uses a Tab instead of two spaces.
    #[serde(default)]
    pub format_use_tabs: bool,

    /// Font family name (e.g. "Default", "PingFang SC", "Songti SC")
    #[serde(default = "default_font_family")]
    pub font_family: String,

    /// Font color (e.g. "Default", "#1A1A1A", "#1E3A5F")
    #[serde(default = "default_font_color")]
    pub font_color: String,

    /// Recently opened file paths
    /// since the path is a string(heap data),
    /// Using fixed-size array won't make much difference on performance
    #[serde(default)]
    pub recent_files: Vec<PathBuf>,

    /// Whether Vim mode is enabled
    #[serde(default)]
    pub vim_mode: bool,

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

    /// Currently active skin / theme ID
    #[serde(default = "default_current_skin")]
    pub current_skin: String,

    /// Remote GitHub repo for skin marketplace
    #[serde(default = "default_skin_market_repo")]
    pub skin_market_repo: String,

    /// Remote GitHub branch for skin marketplace
    #[serde(default = "default_skin_market_branch")]
    pub skin_market_branch: String,

    /// Remote GitHub folder path for skin marketplace
    #[serde(default = "default_skin_market_folder")]
    pub skin_market_folder: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            autosave_interval: 300, // 5 minutes
            font_size: default_font_size(),
            line_spacing: default_line_spacing(),
            format_use_tabs: false,
            font_family: default_font_family(),
            font_color: default_font_color(),
            recent_files: Vec::new(),
            vim_mode: false,
            custom_workspace: None,
            workspace_folders: Vec::new(),
            github_publish: crate::plugin::builtin::github_publish::GithubPublishConfig::default(),
            cloud_backup: crate::plugin::builtin::cloud_backup::CloudBackupConfig::default(),
            disabled_plugins: Vec::new(),
            external_plugin_settings: std::collections::BTreeMap::new(),
            current_skin: default_current_skin(),
            skin_market_repo: default_skin_market_repo(),
            skin_market_branch: default_skin_market_branch(),
            skin_market_folder: default_skin_market_folder(),
        }
    }
}

impl Settings {
    /// Replace only the retired factory source, preserving custom repositories/paths.
    pub fn migrate_skin_market_source(&mut self) -> bool {
        let legacy_default = self
            .skin_market_repo
            .trim()
            .eq_ignore_ascii_case("retricsu/paper-shell-themes")
            && self.skin_market_branch.trim() == "main"
            && self.skin_market_folder.trim().trim_matches('/') == "skins";
        if !legacy_default {
            return false;
        }
        self.skin_market_repo = default_skin_market_repo();
        self.skin_market_branch = default_skin_market_branch();
        self.skin_market_folder = default_skin_market_folder();
        true
    }

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
        if self.font_color != "Default" && parse_hex_color(&self.font_color).is_none() {
            self.font_color = default_font_color();
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
        settings.cloud_backup.encryption_password = "test_pass".to_string().into();

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        confy::store_path(&path, &settings).unwrap();
        let reloaded: Settings = confy::load_path(&path).unwrap();
        assert_eq!(
            reloaded.cloud_backup.server_url,
            "https://dav.example.com/papershell/"
        );
        assert!(reloaded.cloud_backup.enable_encryption);
        assert!(reloaded.cloud_backup.encryption_password.is_empty());
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("test_pass")
        );

        // Older versions omitted the password: ask for it once, retaining the
        // existing server and encryption settings.
        let legacy: Settings = toml::from_str(
            "[cloud_backup]\nserver_url = 'https://dav.example.com/papershell/'\nenable_encryption = true\n",
        )
        .unwrap();
        assert_eq!(
            legacy.cloud_backup.server_url,
            settings.cloud_backup.server_url
        );
        assert!(legacy.cloud_backup.enable_encryption);
        assert!(legacy.cloud_backup.encryption_password.is_empty());

        // Clearing the password must also survive restarting the app.
        settings.cloud_backup.encryption_password.clear();
        confy::store_path(&path, &settings).unwrap();
        let cleared: Settings = confy::load_path(&path).unwrap();
        assert!(cleared.cloud_backup.encryption_password.is_empty());
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
        assert!(!settings.format_use_tabs);

        settings.font_size = 18.0;
        settings.line_spacing = 1.8;
        settings.font_family = "PingFang SC".to_string();
        settings.format_use_tabs = true;

        let serialized = toml::to_string(&settings).unwrap();
        let reloaded: Settings = toml::from_str(&serialized).unwrap();
        assert_eq!(reloaded.font_size, 18.0);
        assert_eq!(reloaded.line_spacing, 1.8);
        assert_eq!(reloaded.font_family, "PingFang SC");
        assert!(reloaded.format_use_tabs);

        // Deserializing empty string should use defaults
        let empty_reloaded: Settings = toml::from_str("").unwrap();
        assert_eq!(empty_reloaded.font_size, 14.0);
        assert_eq!(empty_reloaded.line_spacing, 1.5);
        assert_eq!(empty_reloaded.font_family, "Default");
        assert_eq!(empty_reloaded.font_color, "Default");
        assert!(!empty_reloaded.format_use_tabs);

        // Sanitizing out-of-range values restores defaults
        let mut corrupted = Settings {
            font_size: -5.0,
            line_spacing: 0.0,
            font_family: "   ".to_string(),
            font_color: "not-a-color".to_string(),
            ..Settings::default()
        };
        corrupted.sanitize_typography();
        assert_eq!(corrupted.font_size, 14.0);
        assert_eq!(corrupted.line_spacing, 1.5);
        assert_eq!(corrupted.font_family, "Default");
        assert_eq!(corrupted.font_color, "Default");

        // Custom hex colors are preserved by sanitize_typography
        let mut custom_color = Settings {
            font_color: "#1E3A5F".to_string(),
            ..Settings::default()
        };
        custom_color.sanitize_typography();
        assert_eq!(custom_color.font_color, "#1E3A5F");
    }

    #[test]
    fn skin_market_default_repo_points_to_paper_shell_skins_folder() {
        let settings = Settings::default();
        assert_eq!(settings.skin_market_repo, "RetricSu/paper-shell");
        assert_eq!(settings.skin_market_branch, "master");
        assert_eq!(settings.skin_market_folder, "skins");

        assert_eq!(default_skin_market_repo(), "RetricSu/paper-shell");
        assert_eq!(default_skin_market_branch(), "master");
        assert_eq!(default_skin_market_folder(), "skins");
    }

    #[test]
    fn legacy_market_source_migrates_once_and_preserves_custom_sources() {
        let mut settings = Settings {
            skin_market_repo: "retricsu/paper-shell-themes".into(),
            skin_market_branch: "main".into(),
            ..Settings::default()
        };
        assert!(settings.migrate_skin_market_source());
        assert_eq!(settings.skin_market_repo, default_skin_market_repo());
        assert_eq!(settings.skin_market_branch, "master");
        assert!(!settings.migrate_skin_market_source());
        for (repo, branch, folder) in [
            ("someone/themes", "main", "skins"),
            ("retricsu/paper-shell-themes", "custom", "skins"),
            ("retricsu/paper-shell-themes", "main", "my-skins"),
            ("RetricSu/paper-shell", "main", "skins"),
        ] {
            settings.skin_market_repo = repo.into();
            settings.skin_market_branch = branch.into();
            settings.skin_market_folder = folder.into();
            assert!(!settings.migrate_skin_market_source());
            assert_eq!(
                (
                    &*settings.skin_market_repo,
                    &*settings.skin_market_branch,
                    &*settings.skin_market_folder
                ),
                (repo, branch, folder)
            );
        }
    }

    #[test]
    fn anime_skin_import_keeps_the_jpeg_background_and_loads_a_texture() {
        let source =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skins/summer_platform/skin.toml");
        let destination = tempfile::tempdir().unwrap();
        let skin = crate::skin::Skin::import_from_file(&source, destination.path()).unwrap();
        let image_path = skin.dir.as_ref().unwrap().join("background.jpg");
        let bytes = std::fs::read(image_path).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (1920, 1038));
        let mut renderer = crate::skin::SkinRenderer::new();
        let ctx = egui::Context::default();
        assert!(renderer.get_or_load_texture(&ctx, &skin).is_some());
        let package = destination.path().join("summer_platform.paperskin");
        skin.export_to_file(&package).unwrap();
        let reimported = tempfile::tempdir().unwrap();
        let restored = crate::skin::Skin::import_from_file(&package, reimported.path()).unwrap();
        assert!(renderer.get_or_load_texture(&ctx, &restored).is_some());
        assert_eq!(
            std::fs::read(restored.dir.unwrap().join("background.jpg")).unwrap(),
            bytes
        );
    }

    #[test]
    fn whisper_dawn_skin_preserves_classic_paper_spacing_and_loads_background() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skins/whisper_dawn/skin.toml");
        let destination = tempfile::tempdir().unwrap();
        let skin = crate::skin::Skin::import_from_file(&source, destination.path()).unwrap();
        assert_eq!(skin.id, "whisper_dawn");

        // Verify padding matches classic paper defaults
        let classic = crate::skin::Skin::classic_paper();
        assert_eq!(
            skin.spacing.editor_padding_left,
            classic.spacing.editor_padding_left
        );
        assert_eq!(
            skin.spacing.editor_padding_right,
            classic.spacing.editor_padding_right
        );
        assert_eq!(
            skin.spacing.editor_padding_top,
            classic.spacing.editor_padding_top
        );
        assert_eq!(
            skin.spacing.editor_padding_bottom,
            classic.spacing.editor_padding_bottom
        );

        // Verify typography matches classic paper defaults
        assert_eq!(skin.typography.font_size, classic.typography.font_size);
        assert_eq!(
            skin.typography.line_spacing,
            classic.typography.line_spacing
        );

        // Verify subtle background opacity
        assert_eq!(skin.effects.background_image_opacity, 0.18);

        // Verify background texture loads cleanly
        let image_path = skin.dir.as_ref().unwrap().join("background.jpg");
        let bytes = std::fs::read(image_path).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (1920, 1038));

        let mut renderer = crate::skin::SkinRenderer::new();
        let ctx = egui::Context::default();
        assert!(renderer.get_or_load_texture(&ctx, &skin).is_some());

        // Verify package export and re-import roundtrip
        let package = destination.path().join("whisper_dawn.paperskin");
        skin.export_to_file(&package).unwrap();
        let reimported = tempfile::tempdir().unwrap();
        let restored = crate::skin::Skin::import_from_file(&package, reimported.path()).unwrap();
        assert!(renderer.get_or_load_texture(&ctx, &restored).is_some());
        assert_eq!(
            std::fs::read(restored.dir.unwrap().join("background.jpg")).unwrap(),
            bytes
        );
    }

    #[test]
    fn repository_skins_folder_contains_valid_skins() {
        let repo_skins = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skins");
        assert!(repo_skins.is_dir(), "skins folder must exist in repo");

        for skin_id in [
            "cyberpunk_neon",
            "nordic_frost",
            "retro_amber",
            "bamboo_grove",
            "warm_parchment",
            "obsidian_night",
            "mint_breeze",
            "deep_sea",
            "cherry_blossom",
            "summer_platform",
            "megazone_ride",
            "whisper_dawn",
        ] {
            let skin_toml = repo_skins.join(skin_id).join("skin.toml");
            assert!(skin_toml.is_file(), "skin file {:?} must exist", skin_toml);
            let content = std::fs::read_to_string(&skin_toml).unwrap();
            let parsed = crate::skin::Skin::from_toml_str(&content).expect("valid skin toml");
            assert_eq!(parsed.id, skin_id);
            assert!(parsed.validate().is_ok());
        }
    }
}
