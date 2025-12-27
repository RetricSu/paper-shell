use std::path::PathBuf;

// the FileData is self-contained in the disk file
// we use the trick called extended attributes to write metadata to a disk file.
pub struct FileData {
    pub uuid: String,
    pub path: PathBuf,
    pub total_time: u64,
    pub content: String,
}
