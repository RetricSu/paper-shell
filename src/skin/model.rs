use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_author() -> String {
    "Paper Shell".to_string()
}

fn default_font_family() -> String {
    "Default".to_string()
}

fn default_font_size() -> f32 {
    14.0
}

fn default_line_spacing() -> f32 {
    1.5
}

fn default_font_color() -> String {
    "Default".to_string()
}

fn default_item_spacing_x() -> f32 {
    8.0
}

fn default_item_spacing_y() -> f32 {
    8.0
}

fn default_button_padding_x() -> f32 {
    6.0
}

fn default_button_padding_y() -> f32 {
    3.0
}

fn default_window_margin() -> f32 {
    12.0
}

fn default_editor_padding_left() -> f32 {
    2.0
}

fn default_editor_padding_right() -> f32 {
    6.0
}

fn default_editor_padding_top() -> f32 {
    6.0
}

fn default_editor_padding_bottom() -> f32 {
    6.0
}

fn default_background_image_opacity() -> f32 {
    0.25
}

fn default_background_image_fit() -> String {
    "cover".to_string()
}

fn default_window_opacity() -> f32 {
    1.0
}

fn default_editor_opacity() -> f32 {
    1.0
}

/// Validates that a skin ID contains only safe alphanumeric/underscore/dash characters.
pub fn is_valid_skin_id(id: &str) -> bool {
    let s = id.trim();
    !s.is_empty()
        && s.len() <= 64
        && !s.contains(['/', '\\', ':', '\0', '.'])
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Validates that an asset filename contains only safe characters and has an image extension.
pub fn is_valid_asset_filename(name: &str) -> bool {
    let s = name.trim();
    !s.is_empty()
        && s.len() <= 128
        && !s.contains(['/', '\\', ':', '\0'])
        && !s.starts_with('.')
        && !s.contains("..")
        && (s.ends_with(".png")
            || s.ends_with(".jpg")
            || s.ends_with(".jpeg")
            || s.ends_with(".webp"))
}

/// A complete custom skin / theme definition for Paper Shell.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_author")]
    pub author: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(skip)]
    pub dir: Option<PathBuf>,

    #[serde(default)]
    pub typography: SkinTypography,
    #[serde(default)]
    pub spacing: SkinSpacing,
    #[serde(default)]
    pub colors: SkinColors,
    #[serde(default)]
    pub effects: SkinEffects,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinTypography {
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_line_spacing")]
    pub line_spacing: f32,
    #[serde(default = "default_font_color")]
    pub font_color: String,
    #[serde(default)]
    pub format_use_tabs: bool,
}

impl Default for SkinTypography {
    fn default() -> Self {
        Self {
            font_family: default_font_family(),
            font_size: default_font_size(),
            line_spacing: default_line_spacing(),
            font_color: default_font_color(),
            format_use_tabs: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinSpacing {
    #[serde(default = "default_item_spacing_x")]
    pub item_spacing_x: f32,
    #[serde(default = "default_item_spacing_y")]
    pub item_spacing_y: f32,
    #[serde(default = "default_button_padding_x")]
    pub button_padding_x: f32,
    #[serde(default = "default_button_padding_y")]
    pub button_padding_y: f32,
    #[serde(default = "default_window_margin")]
    pub window_margin: f32,
    #[serde(default = "default_editor_padding_left")]
    pub editor_padding_left: f32,
    #[serde(default = "default_editor_padding_right")]
    pub editor_padding_right: f32,
    #[serde(default = "default_editor_padding_top")]
    pub editor_padding_top: f32,
    #[serde(default = "default_editor_padding_bottom")]
    pub editor_padding_bottom: f32,
}

impl Default for SkinSpacing {
    fn default() -> Self {
        Self {
            item_spacing_x: default_item_spacing_x(),
            item_spacing_y: default_item_spacing_y(),
            button_padding_x: default_button_padding_x(),
            button_padding_y: default_button_padding_y(),
            window_margin: default_window_margin(),
            editor_padding_left: default_editor_padding_left(),
            editor_padding_right: default_editor_padding_right(),
            editor_padding_top: default_editor_padding_top(),
            editor_padding_bottom: default_editor_padding_bottom(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SkinColors {
    #[serde(default)]
    pub dark_mode: bool,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub editor_background_color: Option<String>,
    #[serde(default)]
    pub sidebar_background_color: Option<String>,
    #[serde(default)]
    pub titlebar_background_color: Option<String>,
    #[serde(default)]
    pub text_color: Option<String>,
    #[serde(default)]
    pub selection_color: Option<String>,
    #[serde(default)]
    pub accent_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinEffects {
    /// Relative filename of background image within skin folder (e.g. "background.png")
    #[serde(default)]
    pub background_image: Option<String>,
    /// Base64 encoded image data for portable single-file skins
    #[serde(default)]
    pub background_image_data: Option<String>,
    /// Background image opacity from 0.0 (transparent) to 1.0 (opaque)
    #[serde(default = "default_background_image_opacity")]
    pub background_image_opacity: f32,
    /// Background image scaling / fit: "cover", "contain", "tile", "center"
    #[serde(default = "default_background_image_fit")]
    pub background_image_fit: String,
    /// Window background opacity (0.0 to 1.0)
    #[serde(default = "default_window_opacity")]
    pub window_opacity: f32,
    /// Editor central panel opacity (0.0 to 1.0)
    #[serde(default = "default_editor_opacity")]
    pub editor_opacity: f32,
    /// Corner radius for UI widgets and cards
    #[serde(default)]
    pub corner_radius: f32,
    /// Whether subtle shadows are enabled
    #[serde(default)]
    pub shadow: bool,
}

impl Default for SkinEffects {
    fn default() -> Self {
        Self {
            background_image: None,
            background_image_data: None,
            background_image_opacity: default_background_image_opacity(),
            background_image_fit: default_background_image_fit(),
            window_opacity: default_window_opacity(),
            editor_opacity: default_editor_opacity(),
            corner_radius: 0.0,
            shadow: false,
        }
    }
}

impl Default for Skin {
    fn default() -> Self {
        Self::classic_paper()
    }
}

impl Skin {
    pub fn classic_paper() -> Self {
        Self {
            id: "classic_paper".to_string(),
            name: "经典白纸".to_string(),
            description: "默认简约白纸质感，清晰纯净".to_string(),
            author: "Paper Shell".to_string(),
            version: "1.0.0".to_string(),
            builtin: true,
            dir: None,
            typography: SkinTypography::default(),
            spacing: SkinSpacing::default(),
            colors: SkinColors {
                dark_mode: false,
                background_color: Some("#FAFAFA".to_string()),
                editor_background_color: Some("#FFFFFF".to_string()),
                sidebar_background_color: Some("#F6F6F6".to_string()),
                titlebar_background_color: Some("#FFFFFF".to_string()),
                text_color: Some("#1A1A1A".to_string()),
                selection_color: Some("#C8DCFF".to_string()),
                accent_color: Some("#2E9753".to_string()),
            },
            effects: SkinEffects::default(),
        }
    }

    pub fn warm_parchment() -> Self {
        Self {
            id: "warm_parchment".to_string(),
            name: "古雅羊皮纸".to_string(),
            description: "温润典雅的宣纸与羊皮质感，柔和护眼".to_string(),
            author: "Paper Shell".to_string(),
            version: "1.0.0".to_string(),
            builtin: true,
            dir: None,
            typography: SkinTypography {
                font_family: "Default".to_string(),
                font_size: 14.5,
                line_spacing: 1.6,
                font_color: "#382C1E".to_string(),
                format_use_tabs: false,
            },
            spacing: SkinSpacing {
                item_spacing_x: 9.0,
                item_spacing_y: 9.0,
                button_padding_x: 7.0,
                button_padding_y: 4.0,
                window_margin: 12.0,
                editor_padding_left: 10.0,
                editor_padding_right: 10.0,
                editor_padding_top: 8.0,
                editor_padding_bottom: 8.0,
            },
            colors: SkinColors {
                dark_mode: false,
                background_color: Some("#F3ECE0".to_string()),
                editor_background_color: Some("#FAF5EC".to_string()),
                sidebar_background_color: Some("#ECE3D4".to_string()),
                titlebar_background_color: Some("#F3ECE0".to_string()),
                text_color: Some("#382C1E".to_string()),
                selection_color: Some("#EAD6B8".to_string()),
                accent_color: Some("#A26836".to_string()),
            },
            effects: SkinEffects {
                background_image: None,
                background_image_data: None,
                background_image_opacity: 0.18,
                background_image_fit: "cover".to_string(),
                window_opacity: 1.0,
                editor_opacity: 0.98,
                corner_radius: 1.0,
                shadow: false,
            },
        }
    }

    pub fn obsidian_night() -> Self {
        Self {
            id: "obsidian_night".to_string(),
            name: "黑曜石夜".to_string(),
            description: "沉浸式黑曜石深色系，适合夜间专注写作".to_string(),
            author: "Paper Shell".to_string(),
            version: "1.0.0".to_string(),
            builtin: true,
            dir: None,
            typography: SkinTypography {
                font_family: "Default".to_string(),
                font_size: 14.0,
                line_spacing: 1.5,
                font_color: "#DCDCE0".to_string(),
                format_use_tabs: false,
            },
            spacing: SkinSpacing::default(),
            colors: SkinColors {
                dark_mode: true,
                background_color: Some("#141417".to_string()),
                editor_background_color: Some("#1B1B20".to_string()),
                sidebar_background_color: Some("#111114".to_string()),
                titlebar_background_color: Some("#141417".to_string()),
                text_color: Some("#DCDCE0".to_string()),
                selection_color: Some("#3B3B4F".to_string()),
                accent_color: Some("#7E57C2".to_string()),
            },
            effects: SkinEffects {
                background_image: None,
                background_image_data: None,
                background_image_opacity: 0.20,
                background_image_fit: "cover".to_string(),
                window_opacity: 1.0,
                editor_opacity: 0.95,
                corner_radius: 2.0,
                shadow: false,
            },
        }
    }

    pub fn mint_breeze() -> Self {
        Self {
            id: "mint_breeze".to_string(),
            name: "薄荷清夏".to_string(),
            description: "清爽淡雅的青浅色调，静心凝神".to_string(),
            author: "Paper Shell".to_string(),
            version: "1.0.0".to_string(),
            builtin: true,
            dir: None,
            typography: SkinTypography {
                font_family: "Default".to_string(),
                font_size: 14.0,
                line_spacing: 1.5,
                font_color: "#1D3B2B".to_string(),
                format_use_tabs: false,
            },
            spacing: SkinSpacing::default(),
            colors: SkinColors {
                dark_mode: false,
                background_color: Some("#E8F1EB".to_string()),
                editor_background_color: Some("#F3F8F5".to_string()),
                sidebar_background_color: Some("#DFEBE3".to_string()),
                titlebar_background_color: Some("#E8F1EB".to_string()),
                text_color: Some("#1D3B2B".to_string()),
                selection_color: Some("#C4E3D1".to_string()),
                accent_color: Some("#2E8B57".to_string()),
            },
            effects: SkinEffects {
                background_image: None,
                background_image_data: None,
                background_image_opacity: 0.20,
                background_image_fit: "cover".to_string(),
                window_opacity: 1.0,
                editor_opacity: 0.97,
                corner_radius: 1.0,
                shadow: false,
            },
        }
    }

    pub fn deep_sea() -> Self {
        Self {
            id: "deep_sea".to_string(),
            name: "深海墨蓝".to_string(),
            description: "幽深宁静的海蓝格调，低对比度护眼".to_string(),
            author: "Paper Shell".to_string(),
            version: "1.0.0".to_string(),
            builtin: true,
            dir: None,
            typography: SkinTypography {
                font_family: "Default".to_string(),
                font_size: 14.0,
                line_spacing: 1.55,
                font_color: "#CFDAE8".to_string(),
                format_use_tabs: false,
            },
            spacing: SkinSpacing::default(),
            colors: SkinColors {
                dark_mode: true,
                background_color: Some("#0B1320".to_string()),
                editor_background_color: Some("#111D30".to_string()),
                sidebar_background_color: Some("#080E18".to_string()),
                titlebar_background_color: Some("#0B1320".to_string()),
                text_color: Some("#CFDAE8".to_string()),
                selection_color: Some("#1F385C".to_string()),
                accent_color: Some("#3A86FF".to_string()),
            },
            effects: SkinEffects {
                background_image: None,
                background_image_data: None,
                background_image_opacity: 0.20,
                background_image_fit: "cover".to_string(),
                window_opacity: 1.0,
                editor_opacity: 0.95,
                corner_radius: 2.0,
                shadow: false,
            },
        }
    }

    pub fn cherry_blossom() -> Self {
        Self {
            id: "cherry_blossom".to_string(),
            name: "春日樱粉".to_string(),
            description: "柔和春樱淡粉调，温润惬意".to_string(),
            author: "Paper Shell".to_string(),
            version: "1.0.0".to_string(),
            builtin: true,
            dir: None,
            typography: SkinTypography {
                font_family: "Default".to_string(),
                font_size: 14.0,
                line_spacing: 1.5,
                font_color: "#4A2832".to_string(),
                format_use_tabs: false,
            },
            spacing: SkinSpacing::default(),
            colors: SkinColors {
                dark_mode: false,
                background_color: Some("#FBF0F3".to_string()),
                editor_background_color: Some("#FFF8FA".to_string()),
                sidebar_background_color: Some("#F7E5EA".to_string()),
                titlebar_background_color: Some("#FBF0F3".to_string()),
                text_color: Some("#4A2832".to_string()),
                selection_color: Some("#F5D2DC".to_string()),
                accent_color: Some("#D46A84".to_string()),
            },
            effects: SkinEffects {
                background_image: None,
                background_image_data: None,
                background_image_opacity: 0.20,
                background_image_fit: "cover".to_string(),
                window_opacity: 1.0,
                editor_opacity: 0.98,
                corner_radius: 2.0,
                shadow: false,
            },
        }
    }

    /// List of all built-in skins shipped with Paper Shell
    pub fn builtins() -> Vec<Self> {
        vec![
            Self::classic_paper(),
            Self::warm_parchment(),
            Self::obsidian_night(),
            Self::mint_breeze(),
            Self::deep_sea(),
            Self::cherry_blossom(),
        ]
    }

    pub fn validate(&self) -> Result<(), String> {
        if !is_valid_skin_id(&self.id) {
            return Err(
                "皮肤 ID 无效：必须仅包含英文字母、数字、下划线或短横线，长度 1-64 字符"
                    .to_string(),
            );
        }
        if self.name.trim().is_empty() {
            return Err("皮肤名称不能为空".to_string());
        }
        if let Some(ref bg_img) = self.effects.background_image
            && !is_valid_asset_filename(bg_img)
        {
            return Err(
                "背景图片文件名无效：不能包含路径分隔符，必须以合法图片后缀结尾".to_string(),
            );
        }
        if self.typography.font_size <= 0.0 || self.typography.font_size > 72.0 {
            return Err("字体大小必须在 1 到 72 之间".to_string());
        }
        if self.typography.line_spacing < 0.5 || self.typography.line_spacing > 5.0 {
            return Err("行间距倍率必须在 0.5 到 5.0 之间".to_string());
        }
        Ok(())
    }

    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    pub fn from_toml_str(content: &str) -> Result<Self, String> {
        let skin: Skin = toml::from_str(content).map_err(|e| format!("解析皮肤 TOML 失败: {e}"))?;
        skin.validate()?;
        Ok(skin)
    }

    pub fn from_json_str(content: &str) -> Result<Self, String> {
        let skin: Skin =
            serde_json::from_str(content).map_err(|e| format!("解析皮肤 JSON 失败: {e}"))?;
        skin.validate()?;
        Ok(skin)
    }

    /// Export the skin definition and optional background image to a file.
    /// If target path ends with .zip or .paperskin, writes a zip archive.
    /// If ends with .toml, writes a toml manifest (embedding background image as base64 if needed).
    pub fn export_to_file(&self, target_path: &Path) -> Result<(), String> {
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "zip" || ext == "paperskin" {
            self.export_to_zip(target_path)
        } else {
            // Standalone TOML file export.
            let mut export_skin = self.clone();
            // If skin has a background image file in its dir, embed it as base64 if not already embedded
            if export_skin.effects.background_image_data.is_none()
                && let Some(ref img_name) = export_skin.effects.background_image
                && let Some(ref dir) = export_skin.dir
            {
                let img_path = dir.join(img_name);
                if img_path.exists()
                    && let Ok(bytes) = std::fs::read(&img_path)
                {
                    export_skin.effects.background_image_data =
                        Some(base64::engine::general_purpose::STANDARD.encode(&bytes));
                }
            }

            let toml_str = export_skin
                .to_toml_string()
                .map_err(|e| format!("序列化皮肤失败: {e}"))?;
            std::fs::write(target_path, toml_str).map_err(|e| format!("写入皮肤文件失败: {e}"))?;
            Ok(())
        }
    }

    fn export_to_zip(&self, target_path: &Path) -> Result<(), String> {
        let file = File::create(target_path).map_err(|e| format!("创建压缩包失败: {e}"))?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // 1. Write skin.toml
        zip.start_file("skin.toml", options)
            .map_err(|e| format!("向压缩包写入 skin.toml 失败: {e}"))?;
        let toml_str = self
            .to_toml_string()
            .map_err(|e| format!("序列化皮肤失败: {e}"))?;
        zip.write_all(toml_str.as_bytes())
            .map_err(|e| format!("写入 skin.toml 内容失败: {e}"))?;

        // 2. Write background image if present
        if let Some(ref img_name) = self.effects.background_image
            && let Some(ref dir) = self.dir
        {
            let img_path = dir.join(img_name);
            if img_path.exists()
                && let Ok(bytes) = std::fs::read(&img_path)
            {
                zip.start_file(img_name, options)
                    .map_err(|e| format!("向压缩包写入背景图失败: {e}"))?;
                zip.write_all(&bytes)
                    .map_err(|e| format!("写入背景图数据失败: {e}"))?;
            }
        } else if let Some(ref b64) = self.effects.background_image_data
            && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64)
        {
            let img_name = self
                .effects
                .background_image
                .as_deref()
                .unwrap_or("background.png");
            zip.start_file(img_name, options)
                .map_err(|e| format!("向压缩包写入背景图失败: {e}"))?;
            zip.write_all(&bytes)
                .map_err(|e| format!("写入背景图数据失败: {e}"))?;
        }

        zip.finish()
            .map_err(|e| format!("完成压缩包写入失败: {e}"))?;
        Ok(())
    }

    /// Import a skin from an external file (.toml, .json, .zip, .paperskin) or directory,
    /// installing it into `destination_dir/{skin.id}/`.
    pub fn import_from_file(source_path: &Path, destination_dir: &Path) -> Result<Skin, String> {
        const MAX_ZIP_ENTRIES: usize = 64;
        const MAX_ZIP_EXPANDED_SIZE: u64 = 25 * 1024 * 1024; // 25 MB max total

        if !source_path.exists() {
            return Err("所选导入文件不存在".to_string());
        }

        if source_path.is_dir() {
            // Import directory
            let manifest_path = if source_path.join("skin.toml").exists() {
                source_path.join("skin.toml")
            } else if source_path.join("plugin.toml").exists() {
                source_path.join("plugin.toml")
            } else {
                return Err("目录中未找到 skin.toml 或 plugin.toml".to_string());
            };

            let content = std::fs::read_to_string(&manifest_path)
                .map_err(|e| format!("读取皮肤配置文件失败: {e}"))?;
            let mut skin = Self::from_toml_str(&content)?;

            let target_dir = destination_dir.join(&skin.id);
            if !target_dir.starts_with(destination_dir) || target_dir == destination_dir {
                return Err("皮肤 ID 导致路径越界".to_string());
            }
            std::fs::create_dir_all(&target_dir).map_err(|e| format!("创建皮肤目录失败: {e}"))?;

            // Copy all files from source to destination safely
            if let Ok(entries) = std::fs::read_dir(source_path) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name();
                    let file_name_str = file_name.to_string_lossy();
                    if file_name_str.starts_with('.') || file_name_str.contains(['/', '\\', ':']) {
                        continue;
                    }
                    let target_file = target_dir.join(&file_name);
                    let _ = std::fs::copy(entry.path(), target_file);
                }
            }

            skin.dir = Some(target_dir);
            skin.builtin = false;
            Ok(skin)
        } else {
            let ext = source_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if ext == "zip" || ext == "paperskin" {
                // Extract zip archive
                let file =
                    File::open(source_path).map_err(|e| format!("打开皮肤压缩包失败: {e}"))?;
                let mut archive =
                    zip::ZipArchive::new(file).map_err(|e| format!("读取皮肤压缩包失败: {e}"))?;

                if archive.len() > MAX_ZIP_ENTRIES {
                    return Err(format!("压缩包条目过多（上限 {} 项）", MAX_ZIP_ENTRIES));
                }

                // Find skin.toml inside zip
                let mut toml_content = String::new();
                for i in 0..archive.len() {
                    let mut file = archive
                        .by_index(i)
                        .map_err(|e| format!("读取压缩包项失败: {e}"))?;
                    if file
                        .name()
                        .map(|n| n.ends_with("skin.toml"))
                        .unwrap_or(false)
                    {
                        file.read_to_string(&mut toml_content)
                            .map_err(|e| format!("读取 skin.toml 失败: {e}"))?;
                        break;
                    }
                }

                if toml_content.is_empty() {
                    return Err("压缩包中未找到 skin.toml".to_string());
                }

                let mut skin = Self::from_toml_str(&toml_content)?;
                let target_dir = destination_dir.join(&skin.id);
                if !target_dir.starts_with(destination_dir) || target_dir == destination_dir {
                    return Err("皮肤 ID 导致路径越界".to_string());
                }
                std::fs::create_dir_all(&target_dir)
                    .map_err(|e| format!("创建皮肤目录失败: {e}"))?;

                // Extract files safely into target_dir
                let mut total_expanded: u64 = 0;
                for i in 0..archive.len() {
                    let mut file = archive
                        .by_index(i)
                        .map_err(|e| format!("读取压缩包项失败: {e}"))?;

                    if file.is_symlink() {
                        return Err("压缩包包含不安全的符号链接".to_string());
                    }

                    total_expanded = total_expanded
                        .checked_add(file.size())
                        .filter(|&n| n <= MAX_ZIP_EXPANDED_SIZE)
                        .ok_or_else(|| "压缩包解压后过大".to_string())?;

                    if let Some(enclosed) = file.enclosed_name() {
                        let outpath = target_dir.join(enclosed);
                        if !outpath.starts_with(&target_dir) {
                            return Err("压缩包包含路径穿越条目".to_string());
                        }
                        if file.is_dir() {
                            let _ = std::fs::create_dir_all(&outpath);
                        } else {
                            if let Some(p) = outpath.parent() {
                                let _ = std::fs::create_dir_all(p);
                            }
                            if let Ok(mut outfile) = File::create(&outpath) {
                                let _ = std::io::copy(&mut file, &mut outfile);
                            }
                        }
                    }
                }

                skin.dir = Some(target_dir);
                skin.builtin = false;
                Ok(skin)
            } else if ext == "json" {
                let content = std::fs::read_to_string(source_path)
                    .map_err(|e| format!("读取 JSON 失败: {e}"))?;
                let mut skin = Self::from_json_str(&content)?;
                let target_dir = destination_dir.join(&skin.id);
                if !target_dir.starts_with(destination_dir) || target_dir == destination_dir {
                    return Err("皮肤 ID 导致路径越界".to_string());
                }
                std::fs::create_dir_all(&target_dir)
                    .map_err(|e| format!("创建皮肤目录失败: {e}"))?;

                // Save as skin.toml
                let toml_str = skin
                    .to_toml_string()
                    .map_err(|e| format!("序列化皮肤失败: {e}"))?;
                std::fs::write(target_dir.join("skin.toml"), toml_str)
                    .map_err(|e| format!("写入 skin.toml 失败: {e}"))?;

                // If base64 background image data is embedded, extract it
                if let Some(ref b64) = skin.effects.background_image_data
                    && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64)
                {
                    let img_name = skin
                        .effects
                        .background_image
                        .as_deref()
                        .unwrap_or("background.png");
                    if is_valid_asset_filename(img_name) {
                        let _ = std::fs::write(target_dir.join(img_name), bytes);
                        skin.effects.background_image = Some(img_name.to_string());
                    }
                }

                skin.dir = Some(target_dir);
                skin.builtin = false;
                Ok(skin)
            } else {
                // Assume TOML
                let content = std::fs::read_to_string(source_path)
                    .map_err(|e| format!("读取 TOML 失败: {e}"))?;
                let mut skin = Self::from_toml_str(&content)?;
                let target_dir = destination_dir.join(&skin.id);
                if !target_dir.starts_with(destination_dir) || target_dir == destination_dir {
                    return Err("皮肤 ID 导致路径越界".to_string());
                }
                std::fs::create_dir_all(&target_dir)
                    .map_err(|e| format!("创建皮肤目录失败: {e}"))?;

                // Save manifest
                std::fs::write(target_dir.join("skin.toml"), &content)
                    .map_err(|e| format!("写入 skin.toml 失败: {e}"))?;

                // If base64 background image data is embedded, extract it
                if let Some(ref b64) = skin.effects.background_image_data
                    && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64)
                {
                    let img_name = skin
                        .effects
                        .background_image
                        .as_deref()
                        .unwrap_or("background.png");
                    if is_valid_asset_filename(img_name) {
                        let _ = std::fs::write(target_dir.join(img_name), bytes);
                        skin.effects.background_image = Some(img_name.to_string());
                    }
                }

                skin.dir = Some(target_dir);
                skin.builtin = false;
                Ok(skin)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_skins_are_valid() {
        for skin in Skin::builtins() {
            assert!(skin.validate().is_ok());
            assert!(skin.builtin);
        }
    }

    #[test]
    fn skin_toml_roundtrip() {
        let parchment = Skin::warm_parchment();
        let toml_str = parchment.to_toml_string().unwrap();
        let deserialized = Skin::from_toml_str(&toml_str).unwrap();
        assert_eq!(parchment.id, deserialized.id);
        assert_eq!(parchment.name, deserialized.name);
        assert_eq!(
            parchment.typography.font_size,
            deserialized.typography.font_size
        );
        assert_eq!(
            parchment.spacing.window_margin,
            deserialized.spacing.window_margin
        );
        assert_eq!(parchment.colors.dark_mode, deserialized.colors.dark_mode);
        assert_eq!(
            parchment.effects.background_image_opacity,
            deserialized.effects.background_image_opacity
        );
    }

    #[test]
    fn export_and_import_zip_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut skin = Skin::warm_parchment();
        skin.id = "test_custom_parchment".to_string();
        skin.name = "测试羊皮纸".to_string();
        skin.effects.background_image = Some("bg.png".to_string());
        skin.effects.background_image_data =
            Some(base64::engine::general_purpose::STANDARD.encode(b"fake png data"));

        let zip_path = temp_dir.path().join("test_skin.paperskin");
        assert!(skin.export_to_file(&zip_path).is_ok());
        assert!(zip_path.exists());

        let dest_dir = temp_dir.path().join("installed_skins");
        let imported = Skin::import_from_file(&zip_path, &dest_dir).unwrap();
        assert_eq!(imported.id, "test_custom_parchment");
        assert_eq!(imported.name, "测试羊皮纸");
        assert!(
            dest_dir
                .join("test_custom_parchment")
                .join("skin.toml")
                .exists()
        );
        assert!(
            dest_dir
                .join("test_custom_parchment")
                .join("bg.png")
                .exists()
        );
    }

    #[test]
    fn validate_rejects_path_traversal_ids_and_assets() {
        assert!(is_valid_skin_id("safe_skin-123"));
        assert!(!is_valid_skin_id("../bad"));
        assert!(!is_valid_skin_id("/etc/passwd"));
        assert!(!is_valid_skin_id("skin:bad"));
        assert!(!is_valid_skin_id("skin.name"));
        assert!(!is_valid_skin_id(""));

        assert!(is_valid_asset_filename("background.png"));
        assert!(is_valid_asset_filename("texture-1.jpg"));
        assert!(!is_valid_asset_filename("../secret.png"));
        assert!(!is_valid_asset_filename("/etc/shadow"));
        assert!(!is_valid_asset_filename("script.sh"));
        assert!(!is_valid_asset_filename("binary.exe"));

        let mut skin = Skin::classic_paper();
        skin.id = "../escape".to_string();
        assert!(skin.validate().is_err());

        skin.id = "valid_id".to_string();
        skin.effects.background_image = Some("../escape.png".to_string());
        assert!(skin.validate().is_err());
    }
}
