use crate::backend::ai_backend::AiError;
use crate::backend::editor_backend::HistoryEntry;
use crate::backend::sidebar_backend::Mark;
use crate::file::FileData;
use std::collections::HashMap;
use std::path::PathBuf;

/// Response messages from background operations
pub enum ResponseMessage {
    FileSaved(Result<(String, u64), String>), // (uuid, total_time), error
    FileLoaded(Result<FileData, String>),     // FileData, error
    HistoryLoaded(Result<Vec<HistoryEntry>, String>),
    MarksLoaded(Result<HashMap<usize, Mark>, String>),
    NarrativeMapLoaded(Result<Option<Vec<String>>, String>),
    OpenFile(PathBuf),
    AiResponse(Result<Vec<String>, AiError>),
}
