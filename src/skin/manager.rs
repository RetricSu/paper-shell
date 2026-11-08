use super::model::Skin;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub struct SkinManager {
    skins_dir: PathBuf,
    skins: Vec<Skin>,
    current_skin_id: String,
    load_errors: Vec<(PathBuf, String)>,
}

impl SkinManager {
    pub fn new(skins_dir: PathBuf, current_skin_id: String) -> Self {
        let mut manager = Self {
            skins_dir,
            skins: Vec::new(),
            current_skin_id,
            load_errors: Vec::new(),
        };
        manager.rescan();
        manager
    }

    pub fn skins_dir(&self) -> &Path {
        &self.skins_dir
    }

    pub fn current_skin_id(&self) -> &str {
        &self.current_skin_id
    }

    pub fn load_errors(&self) -> &[(PathBuf, String)] {
        &self.load_errors
    }

    pub fn all_skins(&self) -> &[Skin] {
        &self.skins
    }

    pub fn get_skin(&self, id: &str) -> Option<&Skin> {
        self.skins.iter().find(|s| s.id == id)
    }

    pub fn current_skin(&self) -> Skin {
        if let Some(skin) = self.get_skin(&self.current_skin_id) {
            skin.clone()
        } else {
            Skin::classic_paper()
        }
    }

    pub fn set_current_skin(&mut self, id: &str) -> bool {
        if self.get_skin(id).is_some() {
            self.current_skin_id = id.to_string();
            true
        } else {
            false
        }
    }

    /// Rescan skins directory and reload all skins (built-in and external).
    pub fn rescan(&mut self) {
        self.skins.clear();
        self.load_errors.clear();

        // 1. Load built-in skins
        self.skins.extend(Skin::builtins());

        // 2. Scan skins directory for user-installed skins
        if self.skins_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&self.skins_dir)
        {
            let mut subdirs = Vec::new();
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type()
                    && file_type.is_dir()
                {
                    subdirs.push(entry.path());
                }
            }
            subdirs.sort();

            for dir in subdirs {
                let manifest_path = if dir.join("skin.toml").exists() {
                    dir.join("skin.toml")
                } else if dir.join("plugin.toml").exists() {
                    dir.join("plugin.toml")
                } else {
                    continue;
                };

                match std::fs::read_to_string(&manifest_path) {
                    Ok(content) => match Skin::from_toml_str(&content) {
                        Ok(mut skin) => {
                            skin.dir = Some(dir);
                            skin.builtin = false;
                            // Replace any duplicate ID or add
                            if let Some(pos) = self.skins.iter().position(|s| s.id == skin.id) {
                                self.skins[pos] = skin;
                            } else {
                                self.skins.push(skin);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse skin at {:?}: {}", manifest_path, e);
                            self.load_errors.push((dir, e));
                        }
                    },
                    Err(e) => {
                        warn!("Failed to read skin manifest {:?}: {}", manifest_path, e);
                        self.load_errors.push((dir, e.to_string()));
                    }
                }
            }
        }

        info!("Loaded {} skin(s) in total", self.skins.len());
    }

    /// Import a skin file (.toml, .json, .zip, .paperskin) or directory into skins directory
    pub fn import_from_file(&mut self, source_path: &Path) -> Result<Skin, String> {
        let skin = Skin::import_from_file(source_path, &self.skins_dir)?;
        self.rescan();
        self.set_current_skin(&skin.id);
        Ok(skin)
    }

    /// Export a skin by ID to a target file path
    pub fn export_skin_to_file(&self, id: &str, target_path: &Path) -> Result<(), String> {
        let skin = self
            .get_skin(id)
            .ok_or_else(|| format!("未找到皮肤“{id}”"))?;
        skin.export_to_file(target_path)
    }

    /// Save or update a custom skin to disk
    pub fn save_custom_skin(&mut self, skin: Skin) -> Result<Skin, String> {
        skin.validate()?;
        let target_dir = self.skins_dir.join(&skin.id);
        if !target_dir.starts_with(&self.skins_dir) || target_dir == self.skins_dir {
            return Err("皮肤 ID 导致路径越界".to_string());
        }
        std::fs::create_dir_all(&target_dir).map_err(|e| format!("创建皮肤目录失败: {e}"))?;

        let toml_str = skin
            .to_toml_string()
            .map_err(|e| format!("序列化皮肤失败: {e}"))?;
        std::fs::write(target_dir.join("skin.toml"), toml_str)
            .map_err(|e| format!("写入 skin.toml 失败: {e}"))?;

        let mut saved = skin;
        saved.dir = Some(target_dir);
        saved.builtin = false;

        self.rescan();
        self.set_current_skin(&saved.id);
        Ok(saved)
    }

    /// Uninstall a custom skin by removing its directory from disk.
    pub fn uninstall_skin(&mut self, id: &str) -> Result<(), String> {
        let skin = self
            .get_skin(id)
            .ok_or_else(|| format!("未找到皮肤“{id}”"))?;

        if skin.builtin {
            return Err("内置皮肤不能被卸载".to_string());
        }

        if let Some(ref dir) = skin.dir {
            // Safety check: ensure directory is directly inside skins_dir and strictly a child
            if !dir.starts_with(&self.skins_dir)
                || dir == &self.skins_dir
                || dir.parent() != Some(&self.skins_dir)
            {
                return Err("不安全的皮肤目录路径，拒绝删除".to_string());
            }
            std::fs::remove_dir_all(dir).map_err(|e| format!("删除皮肤目录失败: {e}"))?;
        }

        if self.current_skin_id == id {
            self.current_skin_id = "classic_paper".to_string();
        }

        self.rescan();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_loads_builtins_by_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SkinManager::new(temp_dir.path().to_path_buf(), "classic_paper".to_string());
        assert!(manager.all_skins().len() >= 6);
        assert_eq!(manager.current_skin().id, "classic_paper");
    }

    #[test]
    fn manager_save_and_uninstall_custom_skin() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut manager =
            SkinManager::new(temp_dir.path().to_path_buf(), "classic_paper".to_string());

        let mut custom = Skin::warm_parchment();
        custom.id = "my_custom_skin".to_string();
        custom.name = "我的自定义皮肤".to_string();
        custom.builtin = false;

        let saved = manager.save_custom_skin(custom).unwrap();
        assert_eq!(saved.id, "my_custom_skin");
        assert_eq!(manager.current_skin_id(), "my_custom_skin");
        assert!(manager.get_skin("my_custom_skin").is_some());

        assert!(manager.uninstall_skin("my_custom_skin").is_ok());
        assert!(manager.get_skin("my_custom_skin").is_none());
        assert_eq!(manager.current_skin_id(), "classic_paper");
    }

    #[test]
    fn manager_rejects_uninstalling_builtin_skin() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut manager =
            SkinManager::new(temp_dir.path().to_path_buf(), "classic_paper".to_string());
        assert!(manager.uninstall_skin("classic_paper").is_err());
    }
}
