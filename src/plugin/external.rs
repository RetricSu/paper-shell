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
//!
//! [[settings]]
//! key         = "api_key"
//! label       = "API 密钥"
//! type        = "password"
//! required    = true
//!
//! [[settings]]
//! key         = "report_format"
//! label       = "报告格式"
//! type        = "text"
//! default     = "summary"
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
//!   * `PAPER_SHELL_PLUGIN_SETTINGS` — JSON string containing resolved settings,
//! * treats **stdout** as the success message and a non-zero exit code as a
//!   failure (with **stderr** as the error message).

use super::{Plugin, PluginContext, PluginError, PluginLoadError, PluginMetadata, PluginSource};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// File name every external plugin must provide to be discovered.
pub const MANIFEST_FILE: &str = "plugin.toml";

/// Type of declarative configuration setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingType {
    #[default]
    Text,
    Password,
}

/// Declarative configuration field definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDefinition {
    /// Identifier key used in storage and JSON settings payload.
    pub key: String,
    /// Human-readable label displayed in UI.
    #[serde(default)]
    pub label: String,
    /// Optional description / tooltip.
    #[serde(default)]
    pub description: String,
    /// Data input type (text or masked password).
    #[serde(rename = "type", default)]
    pub setting_type: SettingType,
    /// Default value if unset.
    #[serde(default)]
    pub default: String,
    /// Whether this field is mandatory before running.
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawSettings {
    List(Vec<RawSettingItem>),
    Map(BTreeMap<String, RawSettingMapItem>),
}

#[derive(Debug, Clone, Deserialize)]
struct RawSettingItem {
    #[serde(alias = "name")]
    key: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(rename = "type", default)]
    setting_type: Option<SettingType>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    required: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSettingMapItem {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(rename = "type", default)]
    setting_type: Option<SettingType>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    required: Option<bool>,
}

fn deserialize_settings<'de, D>(deserializer: D) -> Result<Vec<SettingDefinition>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<RawSettings>::deserialize(deserializer)?;
    let defs = match raw {
        None => Vec::new(),
        Some(RawSettings::List(list)) => list
            .into_iter()
            .map(|item| {
                let key = item.key;
                let label = item
                    .label
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| key.clone());
                SettingDefinition {
                    key,
                    label,
                    description: item.description.unwrap_or_default(),
                    setting_type: item.setting_type.unwrap_or_default(),
                    default: item.default.unwrap_or_default(),
                    required: item.required.unwrap_or(false),
                }
            })
            .collect(),
        Some(RawSettings::Map(map)) => map
            .into_iter()
            .map(|(key, item)| {
                let label = item
                    .label
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| key.clone());
                SettingDefinition {
                    key,
                    label,
                    description: item.description.unwrap_or_default(),
                    setting_type: item.setting_type.unwrap_or_default(),
                    default: item.default.unwrap_or_default(),
                    required: item.required.unwrap_or(false),
                }
            })
            .collect(),
    };
    Ok(defs)
}

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
    /// Optional declarative settings required or accepted by this plugin.
    #[serde(default, deserialize_with = "deserialize_settings")]
    pub settings: Vec<SettingDefinition>,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("插件 id 不能为空".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("插件名称不能为空".to_string());
        }
        if self.command.trim().is_empty() {
            return Err("插件执行命令不能为空".to_string());
        }
        for (idx, s) in self.settings.iter().enumerate() {
            if s.key.trim().is_empty() {
                return Err(format!("第 {} 个设置项的 key 不能为空", idx + 1));
            }
        }
        let mut keys = std::collections::BTreeSet::new();
        if let Some(duplicate) = self
            .settings
            .iter()
            .map(|setting| &setting.key)
            .find(|key| !keys.insert((*key).clone()))
        {
            return Err(format!("设置项 key '{}' 重复", duplicate));
        }
        Ok(())
    }
}

fn default_version() -> String {
    "0.0.0".to_string()
}

/// A plugin backed by an external process described by a [`PluginManifest`].
pub struct ExternalPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
}

impl ExternalPlugin {
    pub fn new(manifest: PluginManifest, dir: PathBuf) -> Self {
        Self { manifest, dir }
    }

    /// Resolves the plugin's settings using provided inputs, falling back to defaults,
    /// and verifying that all required settings are filled.
    pub fn resolve_settings(
        &self,
        provided: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, PluginError> {
        let mut resolved = BTreeMap::new();
        for def in &self.manifest.settings {
            let val = provided
                .get(&def.key)
                .cloned()
                .unwrap_or_else(|| def.default.clone());
            if def.required && val.trim().is_empty() {
                let name = if def.label.is_empty() {
                    &def.key
                } else {
                    &def.label
                };
                return Err(PluginError::Config(format!(
                    "插件“{}”设置不完整：必填项“{}”未填写",
                    self.manifest.name, name
                )));
            }
            resolved.insert(def.key.clone(), val);
        }
        Ok(resolved)
    }
}

/// Results of scanning the plugins directory.
pub struct DiscoveryResult {
    pub plugins: Vec<Arc<dyn Plugin>>,
    pub errors: Vec<PluginLoadError>,
}

/// Scans `plugins_dir` for external plugins.
///
/// Ensures deterministic ordering, checks for invalid manifests, reserved IDs,
/// duplicate IDs, and collects all failures into `errors`.
pub fn discover(plugins_dir: &Path, reserved_ids: &[&str]) -> DiscoveryResult {
    let mut plugins: Vec<Arc<dyn Plugin>> = Vec::new();
    let mut errors: Vec<PluginLoadError> = Vec::new();

    if !plugins_dir.exists() {
        return DiscoveryResult { plugins, errors };
    }

    let entries = match std::fs::read_dir(plugins_dir) {
        Ok(e) => e,
        Err(err) => {
            errors.push(PluginLoadError {
                path: plugins_dir.to_path_buf(),
                id: None,
                name: None,
                error: format!("无法读取插件目录：{}", err),
            });
            return DiscoveryResult { plugins, errors };
        }
    };

    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in entries {
        match entry {
            Ok(ent) => {
                let path = ent.path();
                match ent.file_type() {
                    Ok(file_type) if file_type.is_dir() => dirs.push(path),
                    Ok(_) => {}
                    Err(err) => errors.push(PluginLoadError {
                        path,
                        id: None,
                        name: None,
                        error: format!("读取插件目录项类型失败：{}", err),
                    }),
                }
            }
            Err(err) => {
                errors.push(PluginLoadError {
                    path: plugins_dir.to_path_buf(),
                    id: None,
                    name: None,
                    error: format!("读取插件目录项失败：{}", err),
                });
            }
        }
    }

    // Stable sort by path
    dirs.sort();

    let mut candidates: Vec<(PathBuf, PluginManifest)> = Vec::new();

    for dir in dirs {
        let manifest_path = dir.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            continue;
        }

        let manifest = match load_manifest(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                errors.push(PluginLoadError {
                    path: dir,
                    id: None,
                    name: None,
                    error: format!("配置文件错误: {}", e),
                });
                continue;
            }
        };

        if let Err(e) = manifest.validate() {
            errors.push(PluginLoadError {
                path: dir,
                id: Some(manifest.id),
                name: Some(manifest.name),
                error: e,
            });
            continue;
        }

        if reserved_ids.contains(&manifest.id.as_str()) {
            errors.push(PluginLoadError {
                path: dir,
                id: Some(manifest.id.clone()),
                name: Some(manifest.name),
                error: format!("插件 ID '{}' 与内置插件保留 ID 冲突", manifest.id),
            });
            continue;
        }

        candidates.push((dir, manifest));
    }

    let mut id_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, manifest) in &candidates {
        *id_counts.entry(manifest.id.clone()).or_default() += 1;
    }

    for (dir, manifest) in candidates {
        if id_counts.get(&manifest.id).copied().unwrap_or_default() > 1 {
            errors.push(PluginLoadError {
                path: dir,
                id: Some(manifest.id.clone()),
                name: Some(manifest.name),
                error: format!(
                    "插件 ID '{}' 与其他插件重复；所有同 ID 插件均未加载",
                    manifest.id
                ),
            });
            continue;
        }

        plugins.push(Arc::new(ExternalPlugin { manifest, dir }));
    }

    // Stable sort plugins by id and errors by path
    plugins.sort_by(|a, b| a.metadata().id.cmp(&b.metadata().id));
    errors.sort_by(|a, b| a.path.cmp(&b.path));

    DiscoveryResult { plugins, errors }
}

fn load_manifest(path: &Path) -> Result<PluginManifest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

/// Checks whether `target_dir` is safely an immediate child directory of `plugins_dir`.
pub fn is_safe_plugin_subdir(
    plugins_dir: &Path,
    target_dir: &Path,
) -> Result<bool, std::io::Error> {
    if !plugins_dir.exists() || !target_dir.exists() {
        return Ok(false);
    }
    let plugins_canonical = plugins_dir.canonicalize()?;
    if std::fs::symlink_metadata(target_dir)?
        .file_type()
        .is_symlink()
    {
        return Ok(false);
    }
    let Some(target_parent) = target_dir.parent() else {
        return Ok(false);
    };
    if target_parent.canonicalize()? != plugins_canonical {
        return Ok(false);
    }
    let target_canonical = target_dir.canonicalize()?;
    if !target_canonical.is_dir() {
        return Ok(false);
    }
    match target_canonical.parent() {
        Some(parent) => Ok(parent == plugins_canonical),
        None => Ok(false),
    }
}

/// Uninstalls an external plugin by deleting its directory, strictly verifying
/// that it is an immediate child of `plugins_dir`.
pub fn uninstall_plugin_directory(
    plugins_dir: &Path,
    target_dir: &Path,
) -> Result<PathBuf, std::io::Error> {
    if !is_safe_plugin_subdir(plugins_dir, target_dir)? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "安全检查未通过：目标必须是 plugins 目录的直接子文件夹",
        ));
    }
    let canonical = target_dir.canonicalize()?;
    std::fs::remove_dir_all(&canonical)?;
    Ok(canonical)
}

impl Plugin for ExternalPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            version: self.manifest.version.clone(),
            author: self.manifest.author.clone(),
            source: PluginSource::External,
            enabled: true,
            has_settings: !self.manifest.settings.is_empty(),
            dir: Some(self.dir.clone()),
        }
    }

    fn source(&self) -> PluginSource {
        PluginSource::External
    }

    fn settings_definitions(&self) -> Vec<SettingDefinition> {
        self.manifest.settings.clone()
    }

    fn run(&self, ctx: &PluginContext) -> Result<String, PluginError> {
        let resolved = self.resolve_settings(&ctx.settings)?;
        let settings_json = serde_json::to_string(&resolved)
            .map_err(|e| PluginError::Config(format!("序列化设置失败: {}", e)))?;

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
            .env("PAPER_SHELL_PLUGIN_SETTINGS", &settings_json)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_with_list_and_map_settings() {
        let list_toml = r#"
id = "ext_list"
name = "List Ext"
command = "echo"
args = ["hello"]

[[settings]]
key = "token"
label = "Secret Token"
type = "password"
required = true

[[settings]]
key = "endpoint"
label = "API Endpoint"
type = "text"
default = "https://example.com"
"#;
        let manifest: PluginManifest = toml::from_str(list_toml).unwrap();
        assert_eq!(manifest.id, "ext_list");
        assert_eq!(manifest.settings.len(), 2);
        assert_eq!(manifest.settings[0].setting_type, SettingType::Password);
        assert!(manifest.settings[0].required);
        assert_eq!(manifest.settings[1].default, "https://example.com");
        assert_eq!(manifest.settings[1].setting_type, SettingType::Text);

        let map_toml = r#"
id = "ext_map"
name = "Map Ext"
command = "echo"

[settings.auth]
type = "password"
required = true

[settings.lang]
default = "zh"
"#;
        let manifest_map: PluginManifest = toml::from_str(map_toml).unwrap();
        assert_eq!(manifest_map.id, "ext_map");
        assert_eq!(manifest_map.settings.len(), 2);
        let auth = manifest_map
            .settings
            .iter()
            .find(|s| s.key == "auth")
            .unwrap();
        assert_eq!(auth.setting_type, SettingType::Password);
        assert!(auth.required);
        let lang = manifest_map
            .settings
            .iter()
            .find(|s| s.key == "lang")
            .unwrap();
        assert_eq!(lang.default, "zh");
    }

    #[test]
    fn manifest_validation_rejects_empty_fields() {
        let empty_id = PluginManifest {
            id: "   ".to_string(),
            name: "Test".to_string(),
            description: "".to_string(),
            version: "0.1.0".to_string(),
            author: "".to_string(),
            command: "echo".to_string(),
            args: vec![],
            settings: vec![],
        };
        assert!(empty_id.validate().is_err());

        let empty_cmd = PluginManifest {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "".to_string(),
            version: "0.1.0".to_string(),
            author: "".to_string(),
            command: "".to_string(),
            args: vec![],
            settings: vec![],
        };
        assert!(empty_cmd.validate().is_err());

        let duplicate_setting_keys = PluginManifest {
            command: "echo".to_string(),
            settings: vec![
                SettingDefinition {
                    key: "same".to_string(),
                    label: String::new(),
                    description: String::new(),
                    setting_type: SettingType::Text,
                    default: String::new(),
                    required: false,
                },
                SettingDefinition {
                    key: "same".to_string(),
                    label: String::new(),
                    description: String::new(),
                    setting_type: SettingType::Password,
                    default: String::new(),
                    required: false,
                },
            ],
            ..empty_cmd
        };
        assert!(
            duplicate_setting_keys
                .validate()
                .unwrap_err()
                .contains("重复")
        );
    }

    #[test]
    fn settings_resolution_enforces_required_fields_and_defaults() {
        let manifest = PluginManifest {
            id: "test".to_string(),
            name: "测试插件".to_string(),
            description: "".to_string(),
            version: "0.1.0".to_string(),
            author: "".to_string(),
            command: "echo".to_string(),
            args: vec![],
            settings: vec![
                SettingDefinition {
                    key: "req_key".to_string(),
                    label: "必填键".to_string(),
                    description: "".to_string(),
                    setting_type: SettingType::Password,
                    default: "".to_string(),
                    required: true,
                },
                SettingDefinition {
                    key: "opt_key".to_string(),
                    label: "可选键".to_string(),
                    description: "".to_string(),
                    setting_type: SettingType::Text,
                    default: "default_val".to_string(),
                    required: false,
                },
            ],
        };
        let plugin = ExternalPlugin::new(manifest, PathBuf::from("/tmp"));

        let mut provided = BTreeMap::new();
        // Missing req_key
        let err = plugin.resolve_settings(&provided).unwrap_err();
        assert!(err.to_string().contains("必填项“必填键”未填写"));

        provided.insert("req_key".to_string(), "my_secret".to_string());
        let resolved = plugin.resolve_settings(&provided).unwrap();
        assert_eq!(resolved.get("req_key").unwrap(), "my_secret");
        assert_eq!(resolved.get("opt_key").unwrap(), "default_val");
    }

    #[cfg(unix)]
    #[test]
    fn run_passes_resolved_settings_as_json() {
        let temp = std::env::temp_dir().join(format!("test_ps_settings_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp).unwrap();
        let plugin = ExternalPlugin::new(
            PluginManifest {
                id: "settings-test".to_string(),
                name: "Settings test".to_string(),
                description: String::new(),
                version: "1.0.0".to_string(),
                author: String::new(),
                command: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "printf '%s' \"$PAPER_SHELL_PLUGIN_SETTINGS\"".to_string(),
                ],
                settings: vec![
                    SettingDefinition {
                        key: "token".to_string(),
                        label: "Token".to_string(),
                        description: String::new(),
                        setting_type: SettingType::Password,
                        default: String::new(),
                        required: true,
                    },
                    SettingDefinition {
                        key: "mode".to_string(),
                        label: "Mode".to_string(),
                        description: String::new(),
                        setting_type: SettingType::Text,
                        default: "summary".to_string(),
                        required: false,
                    },
                ],
            },
            temp.clone(),
        );
        let mut values = BTreeMap::new();
        values.insert("token".to_string(), "secret-value".to_string());
        let output = plugin
            .run(&PluginContext {
                file_path: None,
                content: "document".to_string(),
                data_dir: temp.clone(),
                title: None,
                description: None,
                collection: None,
                printer: None,
                print_margin_points: None,
                settings: values,
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(payload["token"], "secret-value");
        assert_eq!(payload["mode"], "summary");
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn discovery_handles_sorting_duplicates_and_reserved_ids() {
        let temp = std::env::temp_dir().join(format!("test_ps_disc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp).unwrap();

        // 1. Valid plugin A
        let p_a = temp.join("dir_b_plugin");
        std::fs::create_dir_all(&p_a).unwrap();
        std::fs::write(
            p_a.join(MANIFEST_FILE),
            r#"
id = "b_plugin"
name = "B Plugin"
command = "echo"
"#,
        )
        .unwrap();

        // 2. Valid plugin B (id "a_plugin")
        let p_b = temp.join("dir_a_plugin");
        std::fs::create_dir_all(&p_b).unwrap();
        std::fs::write(
            p_b.join(MANIFEST_FILE),
            r#"
id = "a_plugin"
name = "A Plugin"
command = "echo"
"#,
        )
        .unwrap();

        // 3. Reserved ID plugin (claims "print")
        let p_res = temp.join("dir_c_reserved");
        std::fs::create_dir_all(&p_res).unwrap();
        std::fs::write(
            p_res.join(MANIFEST_FILE),
            r#"
id = "print"
name = "Reserved Print"
command = "echo"
"#,
        )
        .unwrap();

        // 4. Duplicate ID plugin (also claims "a_plugin")
        let p_dup = temp.join("dir_d_duplicate");
        std::fs::create_dir_all(&p_dup).unwrap();
        std::fs::write(
            p_dup.join(MANIFEST_FILE),
            r#"
id = "a_plugin"
name = "Duplicate A"
command = "echo"
"#,
        )
        .unwrap();

        // 5. Malformed manifest remains visible as a load error.
        let p_invalid = temp.join("dir_e_invalid");
        std::fs::create_dir_all(&p_invalid).unwrap();
        std::fs::write(p_invalid.join(MANIFEST_FILE), "not valid TOML = [").unwrap();

        let reserved = &["print", "cloud_backup", "github_publish"];
        let result = discover(&temp, reserved);

        assert_eq!(result.plugins.len(), 1);
        assert_eq!(result.plugins[0].metadata().id, "b_plugin");

        assert_eq!(result.errors.len(), 4);
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.error.contains("保留 ID 冲突"))
        );
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.error.contains("配置文件错误"))
        );
        assert_eq!(
            result
                .errors
                .iter()
                .filter(|error| error.error.contains("所有同 ID 插件均未加载"))
                .count(),
            2
        );

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn safe_uninstall_path_checks() {
        let temp = std::env::temp_dir().join(format!("test_ps_uninst_{}", uuid::Uuid::new_v4()));
        let plugins_dir = temp.join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();

        let valid_plugin_dir = plugins_dir.join("my_plugin");
        std::fs::create_dir_all(&valid_plugin_dir).unwrap();

        let nested_dir = valid_plugin_dir.join("subfolder");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let outside_dir = temp.join("outside_folder");
        std::fs::create_dir_all(&outside_dir).unwrap();

        // Direct child is safe
        assert!(is_safe_plugin_subdir(&plugins_dir, &valid_plugin_dir).unwrap());

        // Plugins dir itself is NOT safe to delete as a plugin
        assert!(!is_safe_plugin_subdir(&plugins_dir, &plugins_dir).unwrap());

        // Nested subfolder is NOT a direct child
        assert!(!is_safe_plugin_subdir(&plugins_dir, &nested_dir).unwrap());

        // Outside folder is NOT safe
        assert!(!is_safe_plugin_subdir(&plugins_dir, &outside_dir).unwrap());

        #[cfg(unix)]
        {
            let linked = plugins_dir.join("linked_plugin");
            std::os::unix::fs::symlink(&valid_plugin_dir, &linked).unwrap();
            assert!(!is_safe_plugin_subdir(&plugins_dir, &linked).unwrap());
        }

        // Actual uninstall succeeds for direct child
        assert!(uninstall_plugin_directory(&plugins_dir, &valid_plugin_dir).is_ok());
        assert!(!valid_plugin_dir.exists());

        // Uninstall fails for outside dir
        assert!(uninstall_plugin_directory(&plugins_dir, &outside_dir).is_err());
        assert!(outside_dir.exists());

        std::fs::remove_dir_all(&temp).unwrap();
    }
}
