use super::model::{
    Skin, SkinColors, SkinEffects, SkinSpacing, SkinTypography, is_valid_asset_filename,
    is_valid_skin_id,
};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use tracing::{info, warn};

const MAX_IMAGE_DOWNLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const MAX_MANIFEST_SIZE: usize = 1024 * 1024; // 1 MB

fn http_agent() -> ureq::Agent {
    ureq::config::Config::builder()
        .timeout_connect(Some(std::time::Duration::from_secs(3)))
        .timeout_global(Some(std::time::Duration::from_secs(6)))
        .build()
        .new_agent()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketSkin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub manifest_url: String,
    pub image_url: Option<String>,
    pub skin: Skin,
    pub is_installed: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubContentItem {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    path: String,
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    download_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SkinMarketClient {
    pub repo: String,
    pub branch: String,
    pub folder: String,
    pub cached_skins: Vec<MarketSkin>,
    pub last_error: Option<String>,
}

impl Default for SkinMarketClient {
    fn default() -> Self {
        Self::new(
            crate::config::default_skin_market_repo(),
            crate::config::default_skin_market_branch(),
            crate::config::default_skin_market_folder(),
        )
    }
}

impl SkinMarketClient {
    pub fn new(repo: String, branch: String, folder: String) -> Self {
        Self {
            repo,
            branch,
            folder,
            cached_skins: Vec::new(),
            last_error: None,
        }
    }

    /// Community template skins shipped as featured market offerings (guarantees offline usability)
    pub fn community_templates() -> Vec<MarketSkin> {
        vec![
            MarketSkin {
                id: "cyberpunk_neon".to_string(),
                name: "赛博霓虹".to_string(),
                description: "赛博朋克深紫暗调与霓虹荧光青，充满未来科技感".to_string(),
                author: "Community".to_string(),
                version: "1.0.0".to_string(),
                manifest_url: String::new(),
                image_url: None,
                skin: Skin {
                    id: "cyberpunk_neon".to_string(),
                    name: "赛博霓虹".to_string(),
                    description: "赛博朋克深紫暗调与霓虹荧光青，充满未来科技感".to_string(),
                    author: "Community".to_string(),
                    version: "1.0.0".to_string(),
                    builtin: false,
                    dir: None,
                    typography: SkinTypography {
                        font_family: "Default".to_string(),
                        font_size: 14.0,
                        line_spacing: 1.55,
                        font_color: "#00F0FF".to_string(),
                        format_use_tabs: false,
                    },
                    spacing: SkinSpacing::default(),
                    colors: SkinColors {
                        dark_mode: true,
                        background_color: Some("#0A0814".to_string()),
                        editor_background_color: Some("#120E22".to_string()),
                        sidebar_background_color: Some("#080610".to_string()),
                        titlebar_background_color: Some("#0A0814".to_string()),
                        text_color: Some("#00F0FF".to_string()),
                        selection_color: Some("#3D144C".to_string()),
                        accent_color: Some("#FF007F".to_string()),
                    },
                    effects: SkinEffects {
                        background_image: None,
                        background_image_data: None,
                        background_image_opacity: 0.25,
                        background_image_fit: "cover".to_string(),
                        window_opacity: 1.0,
                        editor_opacity: 0.94,
                        corner_radius: 3.0,
                        shadow: true,
                    },
                },
                is_installed: false,
            },
            MarketSkin {
                id: "nordic_frost".to_string(),
                name: "北欧霜雪".to_string(),
                description: "清冷淡雅的冰霜蓝灰，纯粹克制的北欧极简美学".to_string(),
                author: "Community".to_string(),
                version: "1.0.0".to_string(),
                manifest_url: String::new(),
                image_url: None,
                skin: Skin {
                    id: "nordic_frost".to_string(),
                    name: "北欧霜雪".to_string(),
                    description: "清冷淡雅的冰霜蓝灰，纯粹克制的北欧极简美学".to_string(),
                    author: "Community".to_string(),
                    version: "1.0.0".to_string(),
                    builtin: false,
                    dir: None,
                    typography: SkinTypography {
                        font_family: "Default".to_string(),
                        font_size: 14.0,
                        line_spacing: 1.5,
                        font_color: "#2C3E50".to_string(),
                        format_use_tabs: false,
                    },
                    spacing: SkinSpacing::default(),
                    colors: SkinColors {
                        dark_mode: false,
                        background_color: Some("#EDF2F7".to_string()),
                        editor_background_color: Some("#F7FAFC".to_string()),
                        sidebar_background_color: Some("#E2E8F0".to_string()),
                        titlebar_background_color: Some("#EDF2F7".to_string()),
                        text_color: Some("#2C3E50".to_string()),
                        selection_color: Some("#CBD5E0".to_string()),
                        accent_color: Some("#4A90E2".to_string()),
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
                },
                is_installed: false,
            },
            MarketSkin {
                id: "retro_amber".to_string(),
                name: "复古琥珀".to_string(),
                description: "老式 CRT 终端的温润琥珀光泽，极具沉浸复古感".to_string(),
                author: "Community".to_string(),
                version: "1.0.0".to_string(),
                manifest_url: String::new(),
                image_url: None,
                skin: Skin {
                    id: "retro_amber".to_string(),
                    name: "复古琥珀".to_string(),
                    description: "老式 CRT 终端的温润琥珀光泽，极具沉浸复古感".to_string(),
                    author: "Community".to_string(),
                    version: "1.0.0".to_string(),
                    builtin: false,
                    dir: None,
                    typography: SkinTypography {
                        font_family: "Default".to_string(),
                        font_size: 14.0,
                        line_spacing: 1.5,
                        font_color: "#FFB000".to_string(),
                        format_use_tabs: false,
                    },
                    spacing: SkinSpacing::default(),
                    colors: SkinColors {
                        dark_mode: true,
                        background_color: Some("#15110B".to_string()),
                        editor_background_color: Some("#1C170F".to_string()),
                        sidebar_background_color: Some("#100C08".to_string()),
                        titlebar_background_color: Some("#15110B".to_string()),
                        text_color: Some("#FFB000".to_string()),
                        selection_color: Some("#45300B".to_string()),
                        accent_color: Some("#FFA000".to_string()),
                    },
                    effects: SkinEffects {
                        background_image: None,
                        background_image_data: None,
                        background_image_opacity: 0.25,
                        background_image_fit: "cover".to_string(),
                        window_opacity: 1.0,
                        editor_opacity: 0.95,
                        corner_radius: 1.0,
                        shadow: false,
                    },
                },
                is_installed: false,
            },
            MarketSkin {
                id: "bamboo_grove".to_string(),
                name: "竹林青韵".to_string(),
                description: "竹青纸色与墨绿沉香，东方水墨写意风韵".to_string(),
                author: "Community".to_string(),
                version: "1.0.0".to_string(),
                manifest_url: String::new(),
                image_url: None,
                skin: Skin {
                    id: "bamboo_grove".to_string(),
                    name: "竹林青韵".to_string(),
                    description: "竹青纸色与墨绿沉香，东方水墨写意风韵".to_string(),
                    author: "Community".to_string(),
                    version: "1.0.0".to_string(),
                    builtin: false,
                    dir: None,
                    typography: SkinTypography {
                        font_family: "Default".to_string(),
                        font_size: 14.0,
                        line_spacing: 1.6,
                        font_color: "#243D2A".to_string(),
                        format_use_tabs: false,
                    },
                    spacing: SkinSpacing::default(),
                    colors: SkinColors {
                        dark_mode: false,
                        background_color: Some("#F4F6EE".to_string()),
                        editor_background_color: Some("#FAFBF6".to_string()),
                        sidebar_background_color: Some("#EAEEDD".to_string()),
                        titlebar_background_color: Some("#F4F6EE".to_string()),
                        text_color: Some("#243D2A".to_string()),
                        selection_color: Some("#D2E2CB".to_string()),
                        accent_color: Some("#3B7A4C".to_string()),
                    },
                    effects: SkinEffects {
                        background_image: None,
                        background_image_data: None,
                        background_image_opacity: 0.22,
                        background_image_fit: "cover".to_string(),
                        window_opacity: 1.0,
                        editor_opacity: 0.97,
                        corner_radius: 2.0,
                        shadow: false,
                    },
                },
                is_installed: false,
            },
        ]
    }

    /// Fetch list of skins directly from remote GitHub repository folder.
    pub fn fetch_skins(&mut self, installed_ids: &[String]) -> Vec<MarketSkin> {
        let repo = self.repo.trim().trim_matches('/');
        let branch = self.branch.trim();
        let folder = self.folder.trim().trim_matches('/');

        let api_url = format!(
            "https://api.github.com/repos/{}/contents/{}?ref={}",
            repo, folder, branch
        );

        info!("Fetching skin market from GitHub: {}", api_url);

        let agent = http_agent();
        match agent
            .get(&api_url)
            .header("User-Agent", "paper-shell-desktop")
            .header("Accept", "application/vnd.github.v3+json")
            .call()
        {
            Ok(mut response) => {
                match response.body_mut().read_to_string() {
                    Ok(body_str) => match serde_json::from_str::<Vec<GithubContentItem>>(&body_str)
                    {
                        Ok(items) => {
                            let mut results = Vec::new();
                            for item in items {
                                if item.item_type == "dir" {
                                    if !is_valid_skin_id(&item.name) {
                                        continue;
                                    }
                                    // Subfolder: contains skin.toml
                                    let raw_toml_url = format!(
                                        "https://raw.githubusercontent.com/{}/{}/{}/{}/skin.toml",
                                        repo, branch, folder, item.name
                                    );
                                    if let Ok(mut toml_resp) = agent
                                        .get(&raw_toml_url)
                                        .header("User-Agent", "paper-shell-desktop")
                                        .call()
                                        && let Ok(toml_str) = toml_resp.body_mut().read_to_string()
                                        && let Ok(mut skin) = Skin::from_toml_str(&toml_str)
                                    {
                                        skin.builtin = false;
                                        let image_url = skin
                                            .effects
                                            .background_image
                                            .as_ref()
                                            .filter(|img| is_valid_asset_filename(img))
                                            .map(|img_name| {
                                                format!(
                                                    "https://raw.githubusercontent.com/{}/{}/{}/{}/{}",
                                                    repo, branch, folder, item.name, img_name
                                                )
                                            });

                                        let is_installed = installed_ids.contains(&skin.id);
                                        results.push(MarketSkin {
                                            id: skin.id.clone(),
                                            name: skin.name.clone(),
                                            description: skin.description.clone(),
                                            author: skin.author.clone(),
                                            version: skin.version.clone(),
                                            manifest_url: raw_toml_url,
                                            image_url,
                                            skin,
                                            is_installed,
                                        });
                                    }
                                } else if item.item_type == "file" && item.name.ends_with(".toml") {
                                    let download_url = item.download_url.unwrap_or_else(|| {
                                        format!(
                                            "https://raw.githubusercontent.com/{}/{}/{}/{}/{}",
                                            repo, branch, folder, item.name, item.name
                                        )
                                    });
                                    if let Ok(mut toml_resp) = agent
                                        .get(&download_url)
                                        .header("User-Agent", "paper-shell-desktop")
                                        .call()
                                        && let Ok(toml_str) = toml_resp.body_mut().read_to_string()
                                        && let Ok(mut skin) = Skin::from_toml_str(&toml_str)
                                    {
                                        skin.builtin = false;
                                        let is_installed = installed_ids.contains(&skin.id);
                                        results.push(MarketSkin {
                                            id: skin.id.clone(),
                                            name: skin.name.clone(),
                                            description: skin.description.clone(),
                                            author: skin.author.clone(),
                                            version: skin.version.clone(),
                                            manifest_url: download_url,
                                            image_url: None,
                                            skin,
                                            is_installed,
                                        });
                                    }
                                }
                            }

                            if !results.is_empty() {
                                self.last_error = None;
                                self.cached_skins = results.clone();
                                return results;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse GitHub contents JSON: {}", e);
                            self.last_error = Some(format!("解析仓库内容失败: {e}"));
                        }
                    },
                    Err(e) => {
                        warn!("Failed to read GitHub response body: {}", e);
                        self.last_error = Some(format!("读取 GitHub 响应失败: {e}"));
                    }
                }
            }
            Err(e) => {
                warn!("GitHub API request failed: {}", e);
                self.last_error = Some(format!("无法连接 GitHub 仓库 ({repo}): {e}"));
            }
        }

        // Fallback to community starter templates if remote fetch failed or empty
        let mut fallback = Self::community_templates();
        for item in &mut fallback {
            item.is_installed = installed_ids.contains(&item.id);
        }
        self.cached_skins = fallback.clone();
        fallback
    }

    /// Download and install a market skin into the destination skins folder.
    pub fn download_and_install(
        &self,
        market_skin: &MarketSkin,
        skins_dir: &Path,
    ) -> Result<Skin, String> {
        if !is_valid_skin_id(&market_skin.id) {
            return Err("市场皮肤 ID 无效或包含不安全字符".to_string());
        }

        let target_dir = skins_dir.join(&market_skin.id);
        if !target_dir.starts_with(skins_dir) || target_dir == skins_dir {
            return Err("不安全的皮肤安装路径".to_string());
        }
        std::fs::create_dir_all(&target_dir).map_err(|e| format!("创建皮肤目录失败: {e}"))?;

        let agent = http_agent();

        // 1. Fetch or serialize skin.toml
        let toml_str = if !market_skin.manifest_url.is_empty()
            && let Ok(mut resp) = agent
                .get(&market_skin.manifest_url)
                .header("User-Agent", "paper-shell-desktop")
                .call()
            && let Ok(text) = resp.body_mut().read_to_string()
            && text.len() <= MAX_MANIFEST_SIZE
        {
            text
        } else {
            market_skin
                .skin
                .to_toml_string()
                .map_err(|e| format!("序列化皮肤失败: {e}"))?
        };

        std::fs::write(target_dir.join("skin.toml"), toml_str)
            .map_err(|e| format!("写入 skin.toml 失败: {e}"))?;

        // 2. Fetch background image if present and filename is safe
        if let Some(ref img_url) = market_skin.image_url
            && let Some(ref img_name) = market_skin.skin.effects.background_image
            && is_valid_asset_filename(img_name)
            && let Ok(mut resp) = agent
                .get(img_url)
                .header("User-Agent", "paper-shell-desktop")
                .call()
        {
            let mut bytes = Vec::new();
            if resp
                .body_mut()
                .as_reader()
                .take(MAX_IMAGE_DOWNLOAD_SIZE as u64)
                .read_to_end(&mut bytes)
                .is_ok()
                && !bytes.is_empty()
            {
                let _ = std::fs::write(target_dir.join(img_name), bytes);
            }
        }

        let mut installed = market_skin.skin.clone();
        installed.dir = Some(target_dir);
        installed.builtin = false;
        Ok(installed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_templates_are_valid_and_non_empty() {
        let templates = SkinMarketClient::community_templates();
        assert!(templates.len() >= 4);
        for t in templates {
            assert!(t.skin.validate().is_ok());
            assert!(!t.id.is_empty());
            assert!(!t.name.is_empty());
        }
    }

    #[test]
    fn install_template_skin_to_disk() {
        let temp_dir = tempfile::tempdir().unwrap();
        let client = SkinMarketClient::default();
        let templates = SkinMarketClient::community_templates();
        let target = &templates[0];

        let installed = client
            .download_and_install(target, temp_dir.path())
            .unwrap();
        assert_eq!(installed.id, target.id);
        assert!(temp_dir.path().join(&target.id).join("skin.toml").exists());
    }
}
