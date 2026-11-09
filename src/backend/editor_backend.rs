use crate::config::Config;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;
use xxhash_rust::xxh64::xxh64;

const METADATA_KEY: &str = "user.myeditor.id";
const TOTAL_TIME_KEY: &str = "user.myeditor.total_time";
const BLOB_DIR: &str = "blobs";
const HISTORY_DIR: &str = "history";
const MARKS_DIR: &str = "marks";

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_spent: Option<u64>,
    /// Stable version identity. Legacy saves have no identity until normalized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The version this save was based on. None means a root version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Optional user annotation, used as a version or branch label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Main backend interface for content-addressable storage
pub struct EditorBackend {
    data_dir: PathBuf,
    blobs_dir: PathBuf,
    history_dir: PathBuf,
    marks_dir: PathBuf,
}

impl EditorBackend {
    /// Initialize the backend and create necessary directories
    pub fn new() -> Result<Self, BackendError> {
        let config = Config::default();
        let data_dir = config.data_dir();

        let blobs_dir = data_dir.join(BLOB_DIR);
        let history_dir = data_dir.join(HISTORY_DIR);
        let marks_dir = data_dir.join(MARKS_DIR);

        // Create directories if they don't exist
        fs::create_dir_all(&blobs_dir)?;
        fs::create_dir_all(&history_dir)?;
        fs::create_dir_all(&marks_dir)?;

        Ok(Self {
            data_dir,
            blobs_dir,
            history_dir,
            marks_dir,
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

        // The article may have changed outside the editor. Its recorded path is
        // still a better identity match than silently starting a new history.
        if let Ok(uuid) = self.find_uuid_by_file_path(file_path) {
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

    /// Recover identity when both xattr and the current content hash changed.
    fn find_uuid_by_file_path(&self, file_path: &Path) -> Result<String, BackendError> {
        let mut candidates: Vec<(String, DateTime<Utc>)> = Vec::new();
        for entry in fs::read_dir(&self.history_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(entries) = serde_json::from_str::<Vec<HistoryEntry>>(&content)
                && let Some(matching_entry) = entries
                    .iter()
                    .filter(|entry| entry.file_path.as_deref() == Some(file_path))
                    .max_by_key(|entry| entry.timestamp)
                && let Some(uuid) = path.file_stem().and_then(|s| s.to_str())
            {
                candidates.push((uuid.to_string(), matching_entry.timestamp));
            }
        }
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
        Ok(normalize_history_entries(entries))
    }

    /// Atomically save history so a failed write cannot truncate the whole tree.
    fn save_history(&self, uuid: &str, entries: &[HistoryEntry]) -> Result<(), BackendError> {
        let history_path = self.history_dir.join(format!("{}.json", uuid));
        let temp_path = self
            .history_dir
            .join(format!(".{uuid}.{}.tmp", Uuid::new_v4()));
        let result = (|| -> Result<(), BackendError> {
            let mut file = fs::File::create(&temp_path)?;
            serde_json::to_writer_pretty(&mut file, entries)?;
            file.sync_all()?;
            fs::rename(&temp_path, &history_path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }

    /// Main save method: save file with CAS and xattr tracking
    /// Returns (uuid, new_total_time) to avoid redundant xattr reads in UI
    pub fn save(
        &self,
        file_path: &Path,
        content: &str,
        time_spent: u64,
    ) -> Result<(String, u64), BackendError> {
        // 1. Calculate hash
        let hash = Self::calculate_hash(content);

        // 2. Get or create UUID
        let uuid = self.get_or_create_file_id(file_path, &hash)?;

        // 3. Preserve the legacy linear API: its parent is always the latest save.
        let history = self.load_history_by_uuid(&uuid)?;
        let base_id = history.last().and_then(|entry| entry.id.clone());
        self.save_with_parent(file_path, content, time_spent, base_id)
            .map(|(uuid, total_time, _)| (uuid, total_time))
    }

    /// Save a new version based on the version currently shown in the editor.
    /// Saving from an older version creates a child without deleting any siblings.
    pub fn save_with_parent(
        &self,
        file_path: &Path,
        content: &str,
        time_spent: u64,
        parent_id: Option<String>,
    ) -> Result<(String, u64, String), BackendError> {
        self.save_with_identity(file_path, content, time_spent, parent_id, None)
    }

    /// Save using the document identity held by the editor session. Supplying
    /// the UUID prevents a lost extended attribute from starting a new history.
    pub fn save_with_identity(
        &self,
        file_path: &Path,
        content: &str,
        time_spent: u64,
        parent_id: Option<String>,
        document_uuid: Option<&str>,
    ) -> Result<(String, u64, String), BackendError> {
        let hash = Self::calculate_hash(content);
        self.save_blob(&hash, content)?;
        let uuid = if let Some(uuid) = document_uuid {
            let _ = set_file_id_wrapper(file_path, uuid);
            uuid.to_string()
        } else {
            self.get_or_create_file_id(file_path, &hash)?
        };

        let current_total = get_total_time_wrapper(file_path)?.unwrap_or(0);
        let new_total = current_total + time_spent;

        let mut history = self.load_history_by_uuid(&uuid)?;
        let entry_id = format!("v-{}", Uuid::new_v4());
        if let Some(parent) = &parent_id
            && !history
                .iter()
                .any(|entry| entry.id.as_deref() == Some(parent.as_str()))
        {
            return Err(BackendError::InvalidHash(format!(
                "Parent version not found: {parent}"
            )));
        }
        history.push(HistoryEntry {
            hash,
            timestamp: Utc::now(),
            file_path: Some(file_path.to_path_buf()),
            time_spent: Some(time_spent),
            id: Some(entry_id.clone()),
            parent_id,
            label: None,
        });
        self.save_history(&uuid, &history)?;
        // Focus metadata is committed only after the version record is durable.
        let _ = set_total_time_wrapper(file_path, new_total);

        Ok((uuid, new_total, entry_id))
    }

    fn resolve_uuid_for_file(&self, file_path: &Path) -> Result<String, BackendError> {
        if let Ok(Some(uuid)) = get_file_id_wrapper(file_path) {
            return Ok(uuid);
        }
        // A rollback is not a save yet: the file on disk can still have the hash
        // of a descendant, so use that hash to locate the original document.
        let content = fs::read_to_string(file_path)?;
        let hash = Self::calculate_hash(&content);
        let uuid = self
            .find_uuid_by_hash(&hash)
            .or_else(|_| self.find_uuid_by_file_path(file_path))?;
        let _ = set_file_id_wrapper(file_path, &uuid);
        Ok(uuid)
    }

    fn history_for_file(
        &self,
        file_path: &Path,
    ) -> Result<(String, Vec<HistoryEntry>), BackendError> {
        let uuid = self.resolve_uuid_for_file(file_path)?;
        let entries = self.load_history_by_uuid(&uuid)?;
        Ok((uuid, entries))
    }

    /// Find the recorded version the file is currently based on. When the disk
    /// file matches multiple versions, the latest save is the current head.
    pub fn resolve_current_version_id(&self, file_path: &Path, content: &str) -> Option<String> {
        let hash = Self::calculate_hash(content);
        let (_, entries) = self.history_for_file(file_path).ok()?;
        entries
            .iter()
            .rev()
            .find(|entry| entry.hash == hash)
            .or_else(|| entries.last())
            .and_then(|entry| entry.id.clone())
    }

    /// Update only a version's annotation; article content and tree links stay intact.
    pub fn update_version_label(
        &self,
        file_path: &Path,
        version_id: &str,
        label: Option<String>,
    ) -> Result<(), BackendError> {
        let (uuid, mut entries) = self.history_for_file(file_path)?;
        let entry = entries
            .iter_mut()
            .find(|entry| entry.id.as_deref() == Some(version_id))
            .ok_or_else(|| BackendError::InvalidHash(format!("Version not found: {version_id}")))?;
        entry.label = label.filter(|label| !label.trim().is_empty());
        self.save_history(&uuid, &entries)
    }

    /// Delete one visual version (including its folded identical saves), or its
    /// whole branch. Children of a single deleted version move to its parent.
    pub fn delete_version(
        &self,
        file_path: &Path,
        version_id: &str,
        delete_branch: bool,
    ) -> Result<(), BackendError> {
        let (uuid, mut entries) = self.history_for_file(file_path)?;
        let target_index = entries
            .iter()
            .position(|entry| entry.id.as_deref() == Some(version_id))
            .ok_or_else(|| BackendError::InvalidHash(format!("Version not found: {version_id}")))?;
        let target_parent = entries[target_index].parent_id.clone();
        let visual_ids = visual_representative_ids(&entries);
        // Use exactly the same raw-ID folding map as the visual tree. This also
        // handles identical saves interleaved with other branches.
        let mut removed: HashSet<String> = visual_ids
            .iter()
            .filter(|(_, visible_id)| visible_id.as_str() == version_id)
            .map(|(raw_id, _)| raw_id.clone())
            .collect();
        removed.insert(version_id.to_string());
        if delete_branch {
            loop {
                let before = removed.len();
                for entry in &entries {
                    if entry
                        .parent_id
                        .as_ref()
                        .is_some_and(|id| removed.contains(id))
                        && let Some(id) = &entry.id
                    {
                        removed.insert(id.clone());
                    }
                }
                if removed.len() == before {
                    break;
                }
            }
        }
        if removed.len() == entries.len() {
            return Err(BackendError::InvalidHash(
                "Cannot delete every version of a document".into(),
            ));
        }
        entries.retain(|entry| !entry.id.as_ref().is_some_and(|id| removed.contains(id)));
        for entry in &mut entries {
            if entry
                .parent_id
                .as_ref()
                .is_some_and(|id| removed.contains(id))
            {
                entry.parent_id = if delete_branch {
                    None
                } else {
                    target_parent.clone()
                };
            }
        }
        self.save_history(&uuid, &entries)
    }

    /// Load version history for a file, with stable IDs and parent links.
    pub fn load_history(&self, file_path: &Path) -> Result<Vec<HistoryEntry>, BackendError> {
        let (_, entries) = self.history_for_file(file_path)?;
        Ok(entries)
    }

    /// Read all recorded documents once, off the UI thread. No source files or
    /// metadata are changed, and moved/deleted documents retain their history.
    pub fn load_writing_statistics(
        &self,
    ) -> Result<crate::statistics::WritingStatistics, BackendError> {
        let mut statistics = crate::statistics::WritingStatistics::default();
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.history_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry.path().extension().is_some_and(|ext| ext == "json")
            {
                paths.push(entry.path());
            }
        }
        paths.sort();
        for path in paths {
            match self.load_document_statistics(&path, &mut statistics.warnings) {
                Ok(document) if !document.days.is_empty() => statistics.documents.push(document),
                Ok(_) => {}
                Err(error) => statistics
                    .warnings
                    .push(format!("{}: {error}", path.display())),
            }
        }
        statistics
            .documents
            .sort_by(|a, b| b.words.cmp(&a.words).then_with(|| a.id.cmp(&b.id)));
        Ok(statistics)
    }

    fn load_document_statistics(
        &self,
        path: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<crate::statistics::DocumentStatistics, BackendError> {
        use crate::statistics::{DocumentStatistics, count_words, word_changes};

        let mut entries: Vec<HistoryEntry> = serde_json::from_str(&fs::read_to_string(path)?)?;
        entries.sort_by_key(|entry| entry.timestamp);
        let entries = normalize_history_entries(entries);
        let mut document = DocumentStatistics {
            id: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: entries
                .iter()
                .rev()
                .find_map(|entry| entry.file_path.clone()),
            version_count: entries.len(),
            last_saved: entries.last().map(|entry| entry.timestamp),
            ..Default::default()
        };
        // Load every blob first: a child can have an earlier timestamp than its
        // parent after clock changes, and parent content must still be available.
        let mut contents_by_id = HashMap::new();
        let mut hashes_by_id = HashMap::new();
        for entry in &entries {
            let id = entry.id.clone().unwrap();
            hashes_by_id.insert(id.clone(), entry.hash.clone());
            let blob = self.blobs_dir.join(&entry.hash);
            let content = (|| -> io::Result<String> {
                if entry.hash.len() != 16 || !entry.hash.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "无效的版本标识"));
                }
                if !fs::symlink_metadata(&blob)?.file_type().is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "版本内容不是普通文件",
                    ));
                }
                fs::read_to_string(&blob)
            })();
            match content {
                Ok(content) => {
                    contents_by_id.insert(id, content);
                }
                Err(error) => {
                    warnings.push(format!("{} / {}: {error}", document.id, entry.hash));
                }
            }
        }
        document.words = entries
            .last()
            .and_then(|entry| entry.id.as_ref())
            .and_then(|id| contents_by_id.get(id))
            .map(|content| count_words(content));

        let empty = String::new();
        for entry in &entries {
            let date = entry.timestamp.with_timezone(&chrono::Local).date_naive();
            let day = document.days.entry(date).or_default();
            day.seconds += entry.time_spent.unwrap_or(0);
            // Every normalized root starts from an empty document, not from the
            // preceding save of an unrelated root or branch.
            let parent_content = match entry.parent_id.as_deref() {
                Some(parent) => contents_by_id.get(parent),
                None => Some(&empty),
            };
            let parent_hash = entry
                .parent_id
                .as_deref()
                .and_then(|parent| hashes_by_id.get(parent));
            let Some(content) = entry.id.as_deref().and_then(|id| contents_by_id.get(id)) else {
                // A missing version is not an empty document. Do not invent
                // additions across a gap or report an older total as current.
                continue;
            };
            // Repeated saves can add focus time, but never add the same words again.
            if parent_hash.map(String::as_str) == Some(entry.hash.as_str()) {
                continue;
            }
            if let Some(parent) = parent_content {
                let (added, removed) = word_changes(parent, content);
                day.added += added;
                day.removed += removed;
            }
        }
        let mut total_activity = crate::statistics::WritingActivity::default();
        for day in document.days.values() {
            total_activity += *day;
        }
        document.total_activity = total_activity;
        Ok(document)
    }

    /// Remove a document's history and tracking records completely.
    /// Optionally deletes the local file on disk if requested.
    /// Any blobs no longer referenced by any remaining document history are deleted.
    /// If the local file remains, its file_id metadata is cleared so it is no longer tracked.
    pub fn remove_document_record(
        &self,
        doc_id: &str,
        delete_file_on_disk: bool,
    ) -> Result<Option<PathBuf>, BackendError> {
        if doc_id.is_empty()
            || doc_id.contains('/')
            || doc_id.contains('\\')
            || doc_id.contains("..")
        {
            return Err(BackendError::InvalidHash("Invalid document ID".into()));
        }

        let history_path = self.history_dir.join(format!("{}.json", doc_id));
        let mut doc_hashes = Vec::new();
        let mut file_path = None;

        if history_path.exists() {
            if let Ok(content) = fs::read_to_string(&history_path)
                && let Ok(entries) = serde_json::from_str::<Vec<HistoryEntry>>(&content)
            {
                for entry in &entries {
                    doc_hashes.push(entry.hash.clone());
                }
                file_path = entries.iter().rev().find_map(|e| e.file_path.clone());
            }
            fs::remove_file(&history_path)?;
        }

        // Remove comments file if present
        let comments_path = self.marks_dir.join(format!("{}.comments.json", doc_id));
        if comments_path.exists() {
            let _ = fs::remove_file(comments_path);
        }

        // Handle local file
        if let Some(ref path) = file_path
            && path.exists()
        {
            if delete_file_on_disk {
                let _ = fs::remove_file(path);
            } else if let Ok(Some(existing_id)) = get_file_id_wrapper(path)
                && existing_id == doc_id
            {
                let _ = remove_file_id_wrapper(path);
            }
        }

        // Clean up unreferenced blobs
        if !doc_hashes.is_empty() {
            let mut referenced_hashes = HashSet::new();
            if let Ok(entries) = fs::read_dir(&self.history_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().is_some_and(|ext| ext == "json")
                        && let Ok(content) = fs::read_to_string(entry.path())
                        && let Ok(history_entries) =
                            serde_json::from_str::<Vec<HistoryEntry>>(&content)
                    {
                        for e in history_entries {
                            referenced_hashes.insert(e.hash);
                        }
                    }
                }
            }
            for hash in doc_hashes {
                if !referenced_hashes.contains(&hash) {
                    let blob_path = self.blobs_dir.join(&hash);
                    let _ = fs::remove_file(blob_path);
                }
            }
        }

        Ok(file_path)
    }

    /// Get total writing time for a file
    #[allow(dead_code)]
    pub fn get_total_time(&self, file_path: &Path) -> Result<u64, BackendError> {
        get_total_time_wrapper(file_path)?
            .ok_or_else(|| BackendError::FileNotFound(file_path.to_path_buf()))
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

    /// Load selection comments stored separately from the article's plain text.
    pub fn load_comments(
        &self,
        uuid: &str,
    ) -> Result<Vec<crate::comments::TextComment>, BackendError> {
        let path = self.marks_dir.join(format!("{}.comments.json", uuid));
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    /// Save selection comments without modifying the article or its version history.
    pub fn save_comments(
        &self,
        uuid: &str,
        comments: &[crate::comments::TextComment],
    ) -> Result<(), BackendError> {
        let path = self.marks_dir.join(format!("{}.comments.json", uuid));
        let temp_path = self
            .marks_dir
            .join(format!(".{}.comments.{}.tmp", uuid, Uuid::new_v4()));
        let result = (|| -> Result<(), BackendError> {
            let mut file = fs::File::create(&temp_path)?;
            serde_json::to_writer_pretty(&mut file, comments)?;
            file.sync_all()?;
            fs::rename(&temp_path, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Get UUID for a file, creating one if it doesn't exist
    #[allow(dead_code)]
    pub fn get_uuid(&self, file_path: &Path, content: &str) -> Result<String, BackendError> {
        let hash = Self::calculate_hash(content);
        self.get_or_create_file_id(file_path, &hash)
    }

    /// Get UUID and total time together (reduces xattr reads for UI initialization)
    pub fn get_file_metadata(
        &self,
        file_path: &Path,
        content: &str,
    ) -> Result<(String, u64), BackendError> {
        let hash = Self::calculate_hash(content);
        let uuid = self.get_or_create_file_id(file_path, &hash)?;
        let total_time = get_total_time_wrapper(file_path)?.unwrap_or(0);
        Ok((uuid, total_time))
    }
}

impl Default for EditorBackend {
    fn default() -> Self {
        Self::new().expect("Failed to initialize EditorBackend")
    }
}

/// Upgrade the original linear JSON history in memory. Only legacy entries
/// without an ID inherit the preceding save as their parent; explicit tree
/// links are never replaced, even when they point to a deleted version.
pub(crate) fn normalize_history_entries(entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
    let mut entries = entries;
    let mut used_ids = HashSet::new();
    let mut legacy = Vec::new();
    for (index, entry) in entries.iter_mut().enumerate() {
        if entry.id.is_none() {
            let hash_prefix: String = entry.hash.chars().take(8).collect();
            let mut id = format!(
                "legacy-{index}-{}-{hash_prefix}",
                entry.timestamp.timestamp()
            );
            let mut suffix = 1;
            while used_ids.contains(&id) {
                id = format!(
                    "legacy-{index}-{}-{hash_prefix}-{suffix}",
                    entry.timestamp.timestamp()
                );
                suffix += 1;
            }
            entry.id = Some(id.clone());
            used_ids.insert(id);
            legacy.push(index);
        } else {
            used_ids.insert(entry.id.clone().unwrap());
        }
    }
    for index in legacy {
        if index > 0 && entries[index].parent_id.is_none() {
            entries[index].parent_id = entries[index - 1].id.clone();
        }
    }
    entries
}

/// Map every raw save ID to its visible version ID. Exact duplicate children
/// fold into their visible parent; labels always keep a save visible. Both the
/// tree renderer and deletion logic use this function so they cannot diverge.
pub(crate) fn visual_representative_ids(entries: &[HistoryEntry]) -> HashMap<String, String> {
    let mut representatives = HashMap::new();
    let mut visible_hashes = HashMap::new();
    let mut labeled_visible = HashSet::new();
    for entry in entries {
        let raw_id = entry.id.clone().unwrap();
        let parent_visible = entry.parent_id.as_ref().map(|parent| {
            representatives
                .get(parent)
                .cloned()
                .unwrap_or_else(|| parent.clone())
        });
        let fold = entry.label.is_none()
            && parent_visible.as_ref().is_some_and(|parent| {
                visible_hashes.get(parent) == Some(&entry.hash) && !labeled_visible.contains(parent)
            });
        let visible_id = if fold {
            parent_visible.unwrap()
        } else {
            raw_id.clone()
        };
        representatives.insert(raw_id.clone(), visible_id.clone());
        if visible_id == raw_id {
            visible_hashes.insert(raw_id.clone(), entry.hash.clone());
            if entry.label.is_some() {
                labeled_visible.insert(raw_id);
            }
        }
    }
    representatives
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

/// Remove UUID and time from file metadata (cross-platform)
fn remove_file_id_wrapper(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = xattr::remove(path, METADATA_KEY);
        let _ = xattr::remove(path, TOTAL_TIME_KEY);
        Ok(())
    }
    #[cfg(windows)]
    {
        let ads_path = format!("{}:{}", path.to_string_lossy(), METADATA_KEY);
        let _ = fs::remove_file(ads_path);
        let ads_time = format!("{}:{}", path.to_string_lossy(), TOTAL_TIME_KEY);
        let _ = fs::remove_file(ads_time);
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(())
    }
}

/// Write total time to file metadata (cross-platform)
fn set_total_time_wrapper(path: &Path, time: u64) -> io::Result<()> {
    let time_str = time.to_string();
    #[cfg(unix)]
    {
        xattr::set(path, TOTAL_TIME_KEY, time_str.as_bytes())?;
        Ok(())
    }
    #[cfg(windows)]
    {
        let ads_path = format!("{}:{}", path.to_string_lossy(), TOTAL_TIME_KEY);
        fs::write(ads_path, time_str.as_bytes())
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

/// Read total time from file metadata (cross-platform)
fn get_total_time_wrapper(path: &Path) -> io::Result<Option<u64>> {
    #[cfg(unix)]
    {
        match xattr::get(path, TOTAL_TIME_KEY)? {
            Some(bytes) => {
                let time_str = String::from_utf8_lossy(&bytes);
                match time_str.parse::<u64>() {
                    Ok(time) => Ok(Some(time)),
                    Err(_) => Ok(None),
                }
            }
            None => Ok(None),
        }
    }
    #[cfg(windows)]
    {
        use std::io::ErrorKind;
        let ads_path = format!("{}:{}", path.to_string_lossy(), TOTAL_TIME_KEY);
        match fs::read_to_string(ads_path) {
            Ok(content) => match content.parse::<u64>() {
                Ok(time) => Ok(Some(time)),
                Err(_) => Ok(None),
            },
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
            marks_dir: test_dir.join(MARKS_DIR),
        };

        fs::create_dir_all(&backend.blobs_dir).unwrap();
        fs::create_dir_all(&backend.history_dir).unwrap();
        fs::create_dir_all(&backend.marks_dir).unwrap();

        (backend, test_dir)
    }

    fn cleanup_test_dir(test_dir: &Path) {
        let _ = fs::remove_dir_all(test_dir);
    }

    fn statistics_entry(
        backend: &EditorBackend,
        text: &str,
        timestamp: &str,
        seconds: Option<u64>,
        path: &str,
    ) -> HistoryEntry {
        let hash = EditorBackend::calculate_hash(text);
        backend.save_blob(&hash, text).unwrap();
        HistoryEntry {
            hash,
            timestamp: timestamp.parse().unwrap(),
            file_path: Some(PathBuf::from(path)),
            time_spent: seconds,
            id: None,
            parent_id: None,
            label: None,
        }
    }

    #[test]
    fn writing_statistics_aggregate_versions_and_keep_document_identity() {
        use crate::statistics::{StatisticsGrouping, StatisticsPeriod};
        let (backend, root) = setup_test_backend();
        let first = statistics_entry(
            &backend,
            "甲乙 hello",
            "2026-11-01T12:00:00Z",
            Some(30),
            "old.md",
        );
        let second = statistics_entry(
            &backend,
            "甲丙 hello",
            "2026-11-02T12:00:00Z",
            Some(60),
            "renamed.md",
        );
        let repeated = statistics_entry(
            &backend,
            "甲丙 hello",
            "2026-11-02T12:01:00Z",
            Some(10),
            "renamed.md",
        );
        let last = statistics_entry(
            &backend,
            "甲",
            "2026-11-03T12:00:00Z",
            Some(40),
            "renamed.md",
        );
        // Deliberately unordered: aggregation follows timestamps, not JSON order.
        backend
            .save_history("doc-a", &[last, first, repeated, second])
            .unwrap();
        let other = statistics_entry(
            &backend,
            "one two",
            "2026-11-02T12:00:00Z",
            Some(20),
            "other.md",
        );
        backend.save_history("doc-b", &[other]).unwrap();
        let stats = backend.load_writing_statistics().unwrap();
        assert!(stats.warnings.is_empty());
        assert_eq!(stats.documents.len(), 2);
        assert_eq!(stats.total_words(), 3);
        assert_eq!(
            stats
                .documents
                .iter()
                .find(|d| d.id == "doc-a")
                .unwrap()
                .path,
            Some(PathBuf::from("renamed.md"))
        );
        let report = stats.report(
            StatisticsPeriod::AllTime,
            StatisticsGrouping::Day,
            "2026-11-05".parse().unwrap(),
        );
        assert_eq!(report.activity.added, 6);
        assert_eq!(report.activity.removed, 3);
        assert_eq!(report.activity.seconds, 160);
        assert_eq!(report.active_days, 3);
        cleanup_test_dir(&root);
    }

    #[test]
    fn writing_statistics_skip_corruption_without_inventing_words_across_gaps() {
        use crate::statistics::{StatisticsGrouping, StatisticsPeriod};
        let (backend, root) = setup_test_backend();
        let first = statistics_entry(
            &backend,
            "甲乙丙",
            "2026-11-01T12:00:00Z",
            None,
            "missing.md",
        );
        let mut missing = first.clone();
        missing.hash = "0000000000000000".into();
        missing.timestamp = "2026-11-02T12:00:00Z".parse().unwrap();
        let after_gap = statistics_entry(
            &backend,
            "甲乙丙丁",
            "2026-11-03T12:00:00Z",
            Some(30),
            "missing.md",
        );
        let mut missing_latest = missing.clone();
        missing_latest.timestamp = "2026-11-04T12:00:00Z".parse().unwrap();
        backend
            .save_history("partial", &[first, missing, after_gap, missing_latest])
            .unwrap();
        let good = statistics_entry(
            &backend,
            "hello world",
            "2026-11-01T12:00:00Z",
            Some(10),
            "good.md",
        );
        backend.save_history("good", &[good]).unwrap();
        fs::write(backend.history_dir.join("broken.json"), "not JSON").unwrap();
        fs::create_dir(backend.history_dir.join("directory.json")).unwrap();
        let stats = backend.load_writing_statistics().unwrap();
        assert_eq!(stats.warnings.len(), 3);
        assert_eq!(stats.documents.len(), 2);
        assert_eq!(stats.total_words(), 2);
        assert_eq!(
            stats
                .documents
                .iter()
                .find(|d| d.id == "partial")
                .unwrap()
                .words,
            None
        );
        let report = stats.report(
            StatisticsPeriod::AllTime,
            StatisticsGrouping::Day,
            "2026-11-05".parse().unwrap(),
        );
        assert_eq!(report.activity.added, 5);
        assert_eq!(report.activity.removed, 0);
        assert_eq!(report.activity.seconds, 40);
        cleanup_test_dir(&root);
    }

    #[cfg(unix)]
    #[test]
    fn writing_statistics_do_not_follow_symlinked_records_or_blobs() {
        let (backend, root) = setup_test_backend();
        let entry = statistics_entry(&backend, "甲乙", "2026-11-01T12:00:00Z", None, "file.md");
        backend
            .save_history("real", std::slice::from_ref(&entry))
            .unwrap();
        std::os::unix::fs::symlink(
            backend.history_dir.join("real.json"),
            backend.history_dir.join("link.json"),
        )
        .unwrap();
        fs::remove_file(backend.blobs_dir.join(&entry.hash)).unwrap();
        let target = root.join("elsewhere.txt");
        fs::write(&target, "甲乙").unwrap();
        std::os::unix::fs::symlink(&target, backend.blobs_dir.join(&entry.hash)).unwrap();
        let stats = backend.load_writing_statistics().unwrap();
        assert_eq!(stats.documents.len(), 1);
        assert_eq!(stats.warnings.len(), 1);
        assert_eq!(stats.total_words(), 0);
        cleanup_test_dir(&root);
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
                time_spent: None,
                id: None,
                parent_id: None,
                label: None,
            },
            HistoryEntry {
                hash: "def456".to_string(),
                timestamp: Utc::now(),
                file_path: Some(PathBuf::from("/test/file.txt")),
                time_spent: None,
                id: None,
                parent_id: None,
                label: None,
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
        let (_, total_time) = backend.save(&test_file, content1, 7).unwrap();
        assert_eq!(
            total_time, 7,
            "legacy save must not double-count focus time"
        );

        // Save version 2
        let content2 = "Version 2 content - updated";
        backend.save(&test_file, content2, 0).unwrap();

        // Save version 3 (same as version 1 - test deduplication)
        backend.save(&test_file, content1, 0).unwrap();

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
        assert_eq!(
            history[2].parent_id, history[1].id,
            "legacy save stays linear even when content matches an older version"
        );
        assert!(fs::read_dir(&backend.history_dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));

        // Restore version 2
        let restored = backend.restore_version(&hash2).unwrap();
        assert_eq!(restored, content2, "Restored content should match");

        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn legacy_history_is_upgraded_without_losing_its_linear_lineage() {
        let (backend, root) = setup_test_backend();
        let file = root.join("legacy.md");
        fs::write(&file, "second").unwrap();
        set_file_id_wrapper(&file, "legacy").unwrap();
        let first = statistics_entry(
            &backend,
            "first",
            "2026-01-01T00:00:00Z",
            Some(2),
            "legacy.md",
        );
        let second = statistics_entry(
            &backend,
            "second",
            "2026-01-02T00:00:00Z",
            Some(3),
            "legacy.md",
        );
        backend.save_history("legacy", &[first, second]).unwrap();

        let loaded = backend.load_history(&file).unwrap();
        let first_id = loaded[0].id.clone().unwrap();
        assert_eq!(loaded[0].parent_id, None);
        assert_eq!(loaded[1].parent_id.as_deref(), Some(first_id.as_str()));
        let raw_before_save: Vec<HistoryEntry> = serde_json::from_str(
            &fs::read_to_string(backend.history_dir.join("legacy.json")).unwrap(),
        )
        .unwrap();
        assert!(raw_before_save.iter().all(|entry| entry.id.is_none()));

        backend
            .save_with_parent(&file, "alternate", 4, Some(first_id.clone()))
            .unwrap();
        let upgraded = backend.load_history(&file).unwrap();
        assert_eq!(upgraded.len(), 3);
        assert_eq!(upgraded[0].id.as_deref(), Some(first_id.as_str()));
        assert_eq!(upgraded[1].parent_id.as_deref(), Some(first_id.as_str()));
        assert_eq!(upgraded[2].parent_id.as_deref(), Some(first_id.as_str()));
        let raw_after_save: Vec<HistoryEntry> = serde_json::from_str(
            &fs::read_to_string(backend.history_dir.join("legacy.json")).unwrap(),
        )
        .unwrap();
        assert!(raw_after_save.iter().all(|entry| entry.id.is_some()));
        cleanup_test_dir(&root);
    }

    #[test]
    fn saving_from_an_older_version_creates_a_sibling_branch() {
        let (backend, root) = setup_test_backend();
        let file = root.join("branched.md");
        fs::write(&file, "root").unwrap();
        let (_, _, first) = backend.save_with_parent(&file, "root", 0, None).unwrap();
        fs::write(&file, "main").unwrap();
        let (_, _, main) = backend
            .save_with_parent(&file, "main", 0, Some(first.clone()))
            .unwrap();
        fs::write(&file, "side").unwrap();
        let (_, _, side) = backend
            .save_with_parent(&file, "side", 0, Some(first.clone()))
            .unwrap();

        let history = backend.load_history(&file).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[1].id.as_deref(), Some(main.as_str()));
        assert_eq!(history[1].parent_id.as_deref(), Some(first.as_str()));
        assert_eq!(history[2].id.as_deref(), Some(side.as_str()));
        assert_eq!(history[2].parent_id.as_deref(), Some(first.as_str()));
        assert_eq!(backend.restore_version(&history[1].hash).unwrap(), "main");

        let tree =
            crate::ui::history::tree::build_version_tree(history, &backend, Some(&side)).unwrap();
        assert_eq!(tree.nodes.len(), 3);
        assert_eq!(tree.branches.len(), 2);
        let root_node = tree.nodes.iter().find(|node| node.id == first).unwrap();
        assert_eq!(root_node.children_ids.len(), 2);
        assert!(root_node.is_fork_point);
        let side_node = tree.nodes.iter().find(|node| node.id == side).unwrap();
        assert!(side_node.is_current);
        assert_eq!(side_node.branch_id, 0);
        let main_node = tree.nodes.iter().find(|node| node.id == main).unwrap();
        assert_ne!(main_node.branch_id, 0);
        assert!(side_node.diff_lines.iter().any(|line| {
            line.line_type == crate::ui::history::DiffLineType::Removed && line.content == "root"
        }));
        assert!(
            !side_node
                .diff_lines
                .iter()
                .any(|line| line.content == "main")
        );
        cleanup_test_dir(&root);
    }

    #[test]
    fn version_labels_and_deletion_preserve_the_remaining_tree() {
        let (backend, root) = setup_test_backend();
        let file = root.join("managed.md");
        fs::write(&file, "root").unwrap();
        let first = backend.save_with_parent(&file, "root", 0, None).unwrap().2;
        let second = backend
            .save_with_parent(&file, "second", 0, Some(first.clone()))
            .unwrap()
            .2;
        let third = backend
            .save_with_parent(&file, "third", 0, Some(second.clone()))
            .unwrap()
            .2;
        let side = backend
            .save_with_parent(&file, "side", 0, Some(first.clone()))
            .unwrap()
            .2;
        backend
            .update_version_label(&file, &side, Some("人物线".into()))
            .unwrap();
        assert_eq!(
            backend
                .load_history(&file)
                .unwrap()
                .iter()
                .find(|entry| entry.id.as_deref() == Some(side.as_str()))
                .unwrap()
                .label
                .as_deref(),
            Some("人物线")
        );

        backend.delete_version(&file, &second, false).unwrap();
        let after_single = backend.load_history(&file).unwrap();
        assert_eq!(after_single.len(), 3);
        assert!(
            !after_single
                .iter()
                .any(|entry| entry.id.as_deref() == Some(second.as_str()))
        );
        assert_eq!(
            after_single
                .iter()
                .find(|entry| entry.id.as_deref() == Some(third.as_str()))
                .unwrap()
                .parent_id
                .as_deref(),
            Some(first.as_str())
        );
        assert!(
            after_single
                .iter()
                .any(|entry| entry.id.as_deref() == Some(side.as_str()))
        );

        backend.delete_version(&file, &side, true).unwrap();
        let after_branch = backend.load_history(&file).unwrap();
        assert_eq!(after_branch.len(), 2);
        assert!(
            after_branch
                .iter()
                .all(|entry| entry.id.as_deref() != Some(side.as_str()))
        );
        assert!(backend.delete_version(&file, &first, true).is_err());
        assert_eq!(backend.load_history(&file).unwrap().len(), 2);
        cleanup_test_dir(&root);
    }

    #[test]
    fn branched_statistics_compare_each_save_with_its_real_parent() {
        use crate::statistics::{StatisticsGrouping, StatisticsPeriod};
        let (backend, root) = setup_test_backend();
        let mut first = statistics_entry(&backend, "甲", "2026-02-01T00:00:00Z", None, "branch.md");
        first.id = Some("first".into());
        let mut second = statistics_entry(
            &backend,
            "甲乙丙",
            "2026-02-02T00:00:00Z",
            None,
            "branch.md",
        );
        second.id = Some("second".into());
        second.parent_id = Some("first".into());
        let mut side =
            statistics_entry(&backend, "甲丁", "2026-02-03T00:00:00Z", None, "branch.md");
        side.id = Some("side".into());
        side.parent_id = Some("first".into());
        backend
            .save_history("branched", &[first, second, side])
            .unwrap();
        let stats = backend.load_writing_statistics().unwrap();
        assert!(stats.warnings.is_empty());
        assert_eq!(stats.total_words(), 2);
        let report = stats.report(
            StatisticsPeriod::AllTime,
            StatisticsGrouping::Day,
            "2026-02-04".parse().unwrap(),
        );
        assert_eq!(report.activity.added, 4);
        assert_eq!(report.activity.removed, 0);
        cleanup_test_dir(&root);
    }

    #[test]
    fn identical_saves_fold_visually_but_keep_raw_links_and_focus_time() {
        let (backend, root) = setup_test_backend();
        let file = root.join("duplicates.md");
        fs::write(&file, "same").unwrap();
        let first = backend.save_with_parent(&file, "same", 5, None).unwrap().2;
        let duplicate = backend
            .save_with_parent(&file, "same", 7, Some(first.clone()))
            .unwrap()
            .2;
        let child = backend
            .save_with_parent(&file, "changed", 3, Some(duplicate.clone()))
            .unwrap()
            .2;
        let history = backend.load_history(&file).unwrap();
        let tree =
            crate::ui::history::tree::build_version_tree(history, &backend, Some(&child)).unwrap();
        assert_eq!(tree.nodes.len(), 2);
        let first_node = tree.nodes.iter().find(|node| node.id == first).unwrap();
        assert_eq!(first_node.merged_ids, vec![duplicate.clone()]);
        assert_eq!(first_node.entry.time_spent, Some(12));
        let child_node = tree.nodes.iter().find(|node| node.id == child).unwrap();
        assert_eq!(child_node.parent_id.as_deref(), Some(first.as_str()));

        backend.delete_version(&file, &first, false).unwrap();
        let remaining = backend.load_history(&file).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id.as_deref(), Some(child.as_str()));
        assert_eq!(remaining[0].parent_id, None);
        cleanup_test_dir(&root);
    }

    #[cfg(unix)]
    #[test]
    fn session_identity_survives_lost_xattr_and_invalid_parent_is_rejected() {
        let (backend, root) = setup_test_backend();
        let file = root.join("identity.md");
        fs::write(&file, "first").unwrap();
        let (uuid, total, first) = backend.save_with_parent(&file, "first", 5, None).unwrap();
        assert_eq!(total, 5);
        xattr::remove(&file, METADATA_KEY).unwrap();
        fs::write(&file, "second").unwrap();
        let (saved_uuid, _, second) = backend
            .save_with_identity(&file, "second", 7, Some(first.clone()), Some(&uuid))
            .unwrap();
        assert_eq!(saved_uuid, uuid);
        assert_eq!(backend.load_history(&file).unwrap().len(), 2);
        let history = backend.load_history(&file).unwrap();
        assert_eq!(history[1].id.as_deref(), Some(second.as_str()));
        assert_eq!(history[1].parent_id.as_deref(), Some(first.as_str()));
        assert_eq!(fs::read_dir(&backend.history_dir).unwrap().count(), 1);

        assert!(
            backend
                .save_with_identity(
                    &file,
                    "orphan",
                    100,
                    Some("missing-parent".into()),
                    Some(&uuid),
                )
                .is_err()
        );
        assert_eq!(backend.load_history(&file).unwrap().len(), 2);
        assert_eq!(get_total_time_wrapper(&file).unwrap(), Some(12));

        xattr::remove(&file, METADATA_KEY).unwrap();
        fs::write(&file, "externally changed").unwrap();
        let (recovered_uuid, _) = backend
            .get_file_metadata(&file, "externally changed")
            .unwrap();
        assert_eq!(recovered_uuid, uuid);
        assert_eq!(backend.load_history(&file).unwrap().len(), 2);
        cleanup_test_dir(&root);
    }

    #[test]
    fn interleaved_duplicate_saves_are_deleted_with_their_visual_node() {
        let (backend, root) = setup_test_backend();
        let file = root.join("interleaved.md");
        fs::write(&file, "root").unwrap();
        let first = backend.save_with_parent(&file, "root", 1, None).unwrap().2;
        let duplicate = backend
            .save_with_parent(&file, "root", 1, Some(first.clone()))
            .unwrap()
            .2;
        let other = backend
            .save_with_parent(&file, "other", 1, Some(first.clone()))
            .unwrap()
            .2;
        let late_duplicate = backend
            .save_with_parent(&file, "root", 1, Some(duplicate.clone()))
            .unwrap()
            .2;
        let tree = crate::ui::history::tree::build_version_tree(
            backend.load_history(&file).unwrap(),
            &backend,
            Some(&other),
        )
        .unwrap();
        let first_node = tree.nodes.iter().find(|node| node.id == first).unwrap();
        assert_eq!(
            first_node.merged_ids,
            vec![duplicate.clone(), late_duplicate.clone()]
        );
        assert_eq!(tree.nodes.len(), 2);

        backend.delete_version(&file, &first, false).unwrap();
        let remaining = backend.load_history(&file).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id.as_deref(), Some(other.as_str()));
        assert_eq!(remaining[0].parent_id, None);
        cleanup_test_dir(&root);
    }

    #[test]
    fn statistics_support_multiple_roots_and_reverse_timestamp_parent_links() {
        use crate::statistics::{StatisticsGrouping, StatisticsPeriod};
        let (backend, root) = setup_test_backend();
        let mut first = statistics_entry(&backend, "甲", "2026-03-03T00:00:00Z", None, "roots.md");
        first.id = Some("first-root".into());
        let mut second_root =
            statistics_entry(&backend, "乙", "2026-03-02T00:00:00Z", None, "roots.md");
        second_root.id = Some("second-root".into());
        let mut early_child =
            statistics_entry(&backend, "甲丙", "2026-03-01T00:00:00Z", None, "roots.md");
        early_child.id = Some("early-child".into());
        early_child.parent_id = Some("first-root".into());
        backend
            .save_history("roots", &[first, second_root, early_child])
            .unwrap();
        let stats = backend.load_writing_statistics().unwrap();
        assert!(stats.warnings.is_empty());
        assert_eq!(stats.total_words(), 1);
        let report = stats.report(
            StatisticsPeriod::AllTime,
            StatisticsGrouping::Day,
            "2026-03-04".parse().unwrap(),
        );
        assert_eq!(report.activity.added, 3);
        assert_eq!(report.activity.removed, 0);
        cleanup_test_dir(&root);
    }

    #[test]
    fn test_comments_round_trip() {
        let (backend, test_dir) = setup_test_backend();
        let uuid = Uuid::new_v4().to_string();
        assert!(backend.load_comments(&uuid).unwrap().is_empty());
        let comments = vec![crate::comments::TextComment {
            id: "comment-1".into(),
            text: "记住这里".into(),
            start: 1,
            end: 4,
            quote: " selected ".into(),
            updated_at: Some(chrono::Utc::now()),
        }];
        backend.save_comments(&uuid, &comments).unwrap();
        assert_eq!(backend.load_comments(&uuid).unwrap(), comments);
        backend.save_comments(&uuid, &[]).unwrap();
        assert!(backend.load_comments(&uuid).unwrap().is_empty());
        assert!(fs::read_dir(&backend.marks_dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn corrupt_comments_are_not_overwritten_by_loading() {
        let (backend, test_dir) = setup_test_backend();
        let uuid = Uuid::new_v4().to_string();
        let path = backend.marks_dir.join(format!("{}.comments.json", uuid));
        fs::write(&path, "not json").unwrap();
        assert!(backend.load_comments(&uuid).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "not json");
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

    #[test]
    fn test_remove_document_record_preserves_local_file_cleans_xattr_and_blobs() {
        let (backend, test_dir) = setup_test_backend();
        let file_a = test_dir.join("article_a.md");
        let file_b = test_dir.join("article_b.md");
        fs::write(&file_a, "独创文章内容 A").unwrap();
        fs::write(&file_b, "独创文章内容 B").unwrap();

        let (uuid_a, _) = backend.save(&file_a, "独创文章内容 A", 60).unwrap();
        let (uuid_b, _) = backend.save(&file_b, "独创文章内容 B", 120).unwrap();

        let hash_a = EditorBackend::calculate_hash("独创文章内容 A");
        let hash_b = EditorBackend::calculate_hash("独创文章内容 B");

        assert!(backend.history_dir.join(format!("{uuid_a}.json")).exists());
        assert!(backend.blobs_dir.join(&hash_a).exists());
        assert!(backend.blobs_dir.join(&hash_b).exists());

        let stats = backend.load_writing_statistics().unwrap();
        assert_eq!(stats.documents.len(), 2);

        // Remove document A record (keeping local file)
        let affected = backend.remove_document_record(&uuid_a, false).unwrap();
        assert_eq!(affected, Some(file_a.clone()));

        // History file should be gone
        assert!(!backend.history_dir.join(format!("{uuid_a}.json")).exists());
        // Local file A still exists
        assert!(file_a.exists());
        // xattr on file A should be cleared
        assert_eq!(get_file_id_wrapper(&file_a).unwrap(), None);

        // Blob A should be deleted since no other doc references it
        assert!(!backend.blobs_dir.join(&hash_a).exists());
        // Blob B and document B should remain intact
        assert!(backend.blobs_dir.join(&hash_b).exists());
        assert!(backend.history_dir.join(format!("{uuid_b}.json")).exists());

        let stats_after = backend.load_writing_statistics().unwrap();
        assert_eq!(stats_after.documents.len(), 1);
        assert_eq!(stats_after.documents[0].id, uuid_b);

        // Path traversal rejected
        assert!(backend.remove_document_record("../secret", false).is_err());
        assert!(backend.remove_document_record("", false).is_err());

        cleanup_test_dir(&test_dir);
    }

    #[test]
    fn test_remove_document_record_deletes_disk_file_when_requested() {
        let (backend, test_dir) = setup_test_backend();
        let file = test_dir.join("temp_note.md");
        fs::write(&file, "临时笔记待删").unwrap();

        let (uuid, _) = backend.save(&file, "临时笔记待删", 30).unwrap();
        assert!(file.exists());

        let affected = backend.remove_document_record(&uuid, true).unwrap();
        assert_eq!(affected, Some(file.clone()));
        assert!(!file.exists());
        assert!(!backend.history_dir.join(format!("{uuid}.json")).exists());

        cleanup_test_dir(&test_dir);
    }
}
