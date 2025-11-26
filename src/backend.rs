use crate::config::Config;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;
use xxhash_rust::xxh64::xxh64;

const METADATA_KEY: &str = "user.myeditor.id";
const BLOB_DIR: &str = "blobs";
const HISTORY_DIR: &str = "history";

/// Custom error types for the backend
#[derive(Error, Debug)]
pub enum BackendError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid hash: {0}")]
    InvalidHash(String),

    #[allow(dead_code)]
    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),

    #[allow(dead_code)]
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[allow(dead_code)]
    #[error("Xattr error: {0}")]
    Xattr(String),
}

/// Represents a single version entry in the history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub hash: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<PathBuf>,
}

/// Main backend interface for content-addressable storage
pub struct EditorBackend {
    data_dir: PathBuf,
    blobs_dir: PathBuf,
    history_dir: PathBuf,
}

impl EditorBackend {
    /// Initialize the backend and create necessary directories
    pub fn new() -> Result<Self, BackendError> {
        let config = Config::default();
        let data_dir = config.data_dir();

        let blobs_dir = data_dir.join(BLOB_DIR);
        let history_dir = data_dir.join(HISTORY_DIR);

        // Create directories if they don't exist
        fs::create_dir_all(&blobs_dir)?;
        fs::create_dir_all(&history_dir)?;

        Ok(Self {
            data_dir,
            blobs_dir,
            history_dir,
        })
    }

    /// Calculate XXHash64 of content and return as hex string
    fn calculate_hash(content: &str) -> String {
        let hash = xxh64(content.as_bytes(), 0);
        format!("{:016x}", hash)
    }

    /// Save blob to storage if it doesn't already exist (deduplication)
    fn save_blob(&self, hash: &str, content: &str) -> Result<(), BackendError> {
        let blob_path = self.blobs_dir.join(hash);

        // Only write if blob doesn't exist (deduplication)
        if !blob_path.exists() {
            fs::write(blob_path, content)?;
        }

        Ok(())
    }

    /// Get or set UUID for a file using xattr
    fn get_or_create_file_id(
        &self,
        file_path: &Path,
        content_hash: &str,
    ) -> Result<String, BackendError> {
        // Try to get existing UUID from xattr
        if let Ok(Some(uuid)) = get_file_id_wrapper(file_path) {
            return Ok(uuid);
        }

        // If xattr read failed, try fallback: search history for this hash
        if let Ok(uuid) = self.find_uuid_by_hash(content_hash) {
            // Try to set the UUID back to the file
            let _ = set_file_id_wrapper(file_path, &uuid);
            return Ok(uuid);
        }

        // Generate new UUID
        let new_uuid = Uuid::new_v4().to_string();

        // Try to set xattr (may fail on unsupported filesystems)
        let _ = set_file_id_wrapper(file_path, &new_uuid);

        Ok(new_uuid)
    }

    /// Fallback: search history files for the most recent entry with this hash
    fn find_uuid_by_hash(&self, hash: &str) -> Result<String, BackendError> {
        let mut candidates: Vec<(String, DateTime<Utc>)> = Vec::new();

        // Read all history files
        for entry in fs::read_dir(&self.history_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(entries) = serde_json::from_str::<Vec<HistoryEntry>>(&content)
                && let Some(matching_entry) = entries.iter().find(|e| e.hash == hash)
                && let Some(uuid) = path.file_stem().and_then(|s| s.to_str())
            {
                candidates.push((uuid.to_string(), matching_entry.timestamp));
            }
        }

        // Return the UUID with the most recent timestamp
        candidates.sort_by_key(|(_, timestamp)| *timestamp);
        candidates
            .last()
            .map(|(uuid, _)| uuid.clone())
            .ok_or_else(|| BackendError::InvalidHash("No matching history found".to_string()))
    }

    /// Load history for a UUID
    fn load_history_by_uuid(&self, uuid: &str) -> Result<Vec<HistoryEntry>, BackendError> {
        let history_path = self.history_dir.join(format!("{}.json", uuid));

        if !history_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(history_path)?;
        let entries = serde_json::from_str(&content)?;
        Ok(entries)
    }

    /// Save history for a UUID
    fn save_history(&self, uuid: &str, entries: &[HistoryEntry]) -> Result<(), BackendError> {
        let history_path = self.history_dir.join(format!("{}.json", uuid));
        let content = serde_json::to_string_pretty(entries)?;
        fs::write(history_path, content)?;
        Ok(())
    }

    /// Main save method: save file with CAS and xattr tracking
    pub fn save(&self, file_path: &Path, content: &str) -> Result<(), BackendError> {
        // 1. Calculate hash
        let hash = Self::calculate_hash(content);

        // 2. Save blob (with deduplication)
        self.save_blob(&hash, content)?;

        // 3. Get or create UUID
        let uuid = self.get_or_create_file_id(file_path, &hash)?;

        // 4. Update history
        let mut history = self.load_history_by_uuid(&uuid)?;
        history.push(HistoryEntry {
            hash,
            timestamp: Utc::now(),
            file_path: Some(file_path.to_path_buf()),
        });
        self.save_history(&uuid, &history)?;

        Ok(())
    }

    /// Load version history for a file
    pub fn load_history(&self, file_path: &Path) -> Result<Vec<HistoryEntry>, BackendError> {
        // Get UUID from xattr
        let uuid = get_file_id_wrapper(file_path)?
            .ok_or_else(|| BackendError::FileNotFound(file_path.to_path_buf()))?;

        self.load_history_by_uuid(&uuid)
    }

    /// Restore content from a specific hash
    pub fn restore_version(&self, hash: &str) -> Result<String, BackendError> {
        let blob_path = self.blobs_dir.join(hash);

        if !blob_path.exists() {
            return Err(BackendError::InvalidHash(format!(
                "Blob not found for hash: {}",
                hash
            )));
        }

        let content = fs::read_to_string(blob_path)?;
        Ok(content)
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Get UUID for a file, creating one if it doesn't exist
    pub fn get_uuid(&self, file_path: &Path, content: &str) -> Result<String, BackendError> {
        let hash = Self::calculate_hash(content);
        self.get_or_create_file_id(file_path, &hash)
    }
}

impl Default for EditorBackend {
    fn default() -> Self {
        Self::new().expect("Failed to initialize EditorBackend")
    }
}

// ============================================================================
// Cross-Platform Xattr Wrapper
// ============================================================================

/// Write UUID to file metadata (cross-platform)
fn set_file_id_wrapper(path: &Path, id: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        xattr::set(path, METADATA_KEY, id.as_bytes())
    }
    #[cfg(windows)]
    {
        // Windows ADS: Write to "filename:streamname"
        let ads_path = format!("{}:{}", path.to_string_lossy(), METADATA_KEY);
        fs::write(ads_path, id.as_bytes())
    }
    #[cfg(not(any(unix, windows)))]
    {
        // Unsupported platform
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Extended attributes not supported on this platform",
        ))
    }
}

/// Read UUID from file metadata (cross-platform)
fn get_file_id_wrapper(path: &Path) -> io::Result<Option<String>> {
    #[cfg(unix)]
    {
        match xattr::get(path, METADATA_KEY)? {
            Some(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).to_string())),
            None => Ok(None),
        }
    }
    #[cfg(windows)]
    {
        use std::io::ErrorKind;
        let ads_path = format!("{}:{}", path.to_string_lossy(), METADATA_KEY);
        match fs::read_to_string(ads_path) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        // Unsupported platform
        Ok(None)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_backend() -> (EditorBackend, PathBuf) {
        let test_dir = std::env::temp_dir().join(format!("test_backend_{}", Uuid::new_v4()));
        let backend = EditorBackend {
            data_dir: test_dir.clone(),
            blobs_dir: test_dir.join(BLOB_DIR),
            history_dir: test_dir.join(HISTORY_DIR),
        };

        fs::create_dir_all(&backend.blobs_dir).unwrap();
        fs::create_dir_all(&backend.history_dir).unwrap();

        (backend, test_dir)
    }

    fn cleanup_test_dir(test_dir: &Path) {
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_hash_calculation() {
        let content1 = "Hello, World!";
        let content2 = "Hello, World!";
        let content3 = "Different content";

        let hash1 = EditorBackend::calculate_hash(content1);
        let hash2 = EditorBackend::calculate_hash(content2);
        let hash3 = EditorBackend::calculate_hash(content3);

        assert_eq!(hash1, hash2, "Same content should produce same hash");
        assert_ne!(
            hash1, hash3,
            "Different content should produce different hash"
        );
        assert_eq!(hash1.len(), 16, "Hash should be 16 hex characters");
    }

    #[test]
    fn test_blob_storage() {
        let (backend, test_dir) = setup_test_backend();

        let content = "Test content for blob storage";
        let hash = EditorBackend::calculate_hash(content);

        // Save blob
        backend.save_blob(&hash, content).unwrap();

        // Verify blob exists
        let blob_path = backend.blobs_dir.join(&hash);
        assert!(blob_path.exists(), "Blob file should exist");

        // Verify content
        let saved_content = fs::read_to_string(blob_path).unwrap();
        assert_eq!(saved_content, content, "Blob content should match");

        // Test deduplication (save again)
        let mtime_before = fs::metadata(backend.blobs_dir.join(&hash))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        backend.save_blob(&hash, content).unwrap();
        let mtime_after = fs::metadata(backend.blobs_dir.join(&hash))
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(
            mtime_before, mtime_after,
            "Blob should not be overwritten (deduplication)"
        );

        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_history_tracking() {
        let (backend, test_dir) = setup_test_backend();

        let uuid = Uuid::new_v4().to_string();
        let entries = vec![
            HistoryEntry {
                hash: "abc123".to_string(),
                timestamp: Utc::now(),
                file_path: Some(PathBuf::from("/test/file.txt")),
            },
            HistoryEntry {
                hash: "def456".to_string(),
                timestamp: Utc::now(),
                file_path: Some(PathBuf::from("/test/file.txt")),
            },
        ];

        // Save history
        backend.save_history(&uuid, &entries).unwrap();

        // Load history
        let loaded_entries = backend.load_history_by_uuid(&uuid).unwrap();

        assert_eq!(loaded_entries.len(), 2, "Should load 2 history entries");
        assert_eq!(loaded_entries[0].hash, "abc123");
        assert_eq!(loaded_entries[1].hash, "def456");

        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_full_save_workflow() {
        let (backend, test_dir) = setup_test_backend();

        // Create a test file
        let test_file = test_dir.join("test_file.txt");
        fs::write(&test_file, "initial content").unwrap();

        // Save version 1
        let content1 = "Version 1 content";
        backend.save(&test_file, content1).unwrap();

        // Save version 2
        let content2 = "Version 2 content - updated";
        backend.save(&test_file, content2).unwrap();

        // Save version 3 (same as version 1 - test deduplication)
        backend.save(&test_file, content1).unwrap();

        // Verify blobs exist
        let hash1 = EditorBackend::calculate_hash(content1);
        let hash2 = EditorBackend::calculate_hash(content2);

        assert!(
            backend.blobs_dir.join(&hash1).exists(),
            "Blob for version 1 should exist"
        );
        assert!(
            backend.blobs_dir.join(&hash2).exists(),
            "Blob for version 2 should exist"
        );

        // Verify history (try to get UUID from xattr, fallback to finding by hash
        let history = match backend.load_history(&test_file) {
            Ok(h) => h,
            Err(_) => {
                // Fallback: find UUID by hash
                let uuid = backend.find_uuid_by_hash(&hash1).unwrap();
                backend.load_history_by_uuid(&uuid).unwrap()
            }
        };

        assert_eq!(history.len(), 3, "Should have 3 history entries");

        // Restore version 2
        let restored = backend.restore_version(&hash2).unwrap();
        assert_eq!(restored, content2, "Restored content should match");

        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_restore_version() {
        let (backend, test_dir) = setup_test_backend();

        let content = "Content to restore";
        let hash = EditorBackend::calculate_hash(content);

        // Save blob
        backend.save_blob(&hash, content).unwrap();

        // Restore
        let restored = backend.restore_version(&hash).unwrap();
        assert_eq!(restored, content, "Restored content should match original");

        // Test invalid hash
        let result = backend.restore_version("invalid_hash_123");
        assert!(result.is_err(), "Should error on invalid hash");

        cleanup_test_dir(&test_dir);
    }
}
