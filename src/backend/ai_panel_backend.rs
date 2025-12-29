use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

const NARRATIVE_MAPS_DIR: &str = "narrative_maps";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NarrativeMap {
    pub items: Vec<String>,
}

#[derive(Error, Debug)]
pub enum AiPanelError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct AiPanelBackend {
    narrative_maps_dir: PathBuf,
}

impl AiPanelBackend {
    pub fn new() -> Result<Self, AiPanelError> {
        let config = Config::default();
        let data_dir = config.data_dir();
        let narrative_maps_dir = data_dir.join(NARRATIVE_MAPS_DIR);

        fs::create_dir_all(&narrative_maps_dir)?;

        Ok(Self { narrative_maps_dir })
    }

    pub fn save_narrative_map(&self, uuid: &str, map: &[String]) -> Result<(), AiPanelError> {
        let file_path = self.narrative_maps_dir.join(format!("{}.json", uuid));
        let narrative_map = NarrativeMap { items: map.to_owned() };
        let content = serde_json::to_string_pretty(&narrative_map)?;
        fs::write(file_path, content)?;
        Ok(())
    }

    pub fn load_narrative_map(&self, uuid: &str) -> Result<Option<Vec<String>>, AiPanelError> {
        let file_path = self.narrative_maps_dir.join(format!("{}.json", uuid));

        if !file_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(file_path)?;
        let narrative_map: NarrativeMap = serde_json::from_str(&content)?;
        Ok(Some(narrative_map.items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn setup_test_backend() -> (AiPanelBackend, PathBuf) {
        let test_dir = std::env::temp_dir().join(format!("test_ai_panel_{}", Uuid::new_v4()));
        let narrative_maps_dir = test_dir.join(NARRATIVE_MAPS_DIR);
        fs::create_dir_all(&narrative_maps_dir).unwrap();

        let backend = AiPanelBackend {
            narrative_maps_dir: narrative_maps_dir.clone(),
        };

        (backend, test_dir)
    }

    fn cleanup_test_dir(test_dir: &std::path::Path) {
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_save_and_load_narrative_map() {
        let (backend, test_dir) = setup_test_backend();
        let uuid = Uuid::new_v4().to_string();

        let map = vec![
            "Character is introduced".to_string(),
            "Conflict arises".to_string(),
        ];

        backend.save_narrative_map(&uuid, &map).unwrap();

        let loaded_map = backend.load_narrative_map(&uuid).unwrap();
        assert!(loaded_map.is_some());
        let loaded_map = loaded_map.unwrap();
        assert_eq!(loaded_map.len(), 2);
        assert_eq!(loaded_map[0], "Character is introduced");
        assert_eq!(loaded_map[1], "Conflict arises");

        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_load_nonexistent_narrative_map() {
        let (backend, test_dir) = setup_test_backend();
        let uuid = Uuid::new_v4().to_string();

        let loaded_map = backend.load_narrative_map(&uuid).unwrap();
        assert!(loaded_map.is_none());

        cleanup_test_dir(&test_dir);
    }
}
