use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

const MARKS_DIR: &str = "marks";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Mark {
    pub note: String,
}

#[derive(Error, Debug)]
pub enum SidebarError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct SidebarBackend {
    marks_dir: PathBuf,
}

impl SidebarBackend {
    pub fn new() -> Result<Self, SidebarError> {
        let config = Config::default();
        let data_dir = config.data_dir();
        let marks_dir = data_dir.join(MARKS_DIR);

        fs::create_dir_all(&marks_dir)?;

        Ok(Self { marks_dir })
    }

    pub fn save_marks(&self, uuid: &str, marks: &HashMap<usize, Mark>) -> Result<(), SidebarError> {
        let file_path = self.marks_dir.join(format!("{}.json", uuid));
        let content = serde_json::to_string_pretty(marks)?;
        fs::write(file_path, content)?;
        Ok(())
    }

    pub fn load_marks(&self, uuid: &str) -> Result<HashMap<usize, Mark>, SidebarError> {
        let file_path = self.marks_dir.join(format!("{}.json", uuid));

        if !file_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(file_path)?;
        let marks = serde_json::from_str(&content)?;
        Ok(marks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use uuid::Uuid;

    fn setup_test_backend() -> (SidebarBackend, PathBuf) {
        let test_dir = std::env::temp_dir().join(format!("test_sidebar_{}", Uuid::new_v4()));
        let marks_dir = test_dir.join(MARKS_DIR);
        fs::create_dir_all(&marks_dir).unwrap();

        let backend = SidebarBackend {
            marks_dir: marks_dir.clone(),
        };

        (backend, test_dir)
    }

    fn cleanup_test_dir(test_dir: &Path) {
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_save_and_load_marks() {
        let (backend, test_dir) = setup_test_backend();
        let uuid = Uuid::new_v4().to_string();

        let mut marks = HashMap::new();
        marks.insert(
            1,
            Mark {
                note: "Test note".to_string(),
            },
        );

        backend.save_marks(&uuid, &marks).unwrap();

        let loaded_marks = backend.load_marks(&uuid).unwrap();
        assert_eq!(loaded_marks.len(), 1);
        assert_eq!(loaded_marks.get(&1).unwrap().note, "Test note");

        cleanup_test_dir(&test_dir);
    }
}
