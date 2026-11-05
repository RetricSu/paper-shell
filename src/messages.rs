use crate::backend::editor_backend::HistoryEntry;
use std::path::PathBuf;

/// Response messages from background operations
pub enum ResponseMessage {
    HistoryLoaded {
        path: PathBuf,
        current_version_id: Option<String>,
        result: Result<Vec<HistoryEntry>, String>,
    },
    OpenFile(PathBuf),
    OpenWorkspace(PathBuf),
    /// A plugin finished running: (plugin display name, Ok(message) | Err(error)).
    PluginFinished {
        name: String,
        result: Result<String, String>,
    },
    VimEscape,
}
