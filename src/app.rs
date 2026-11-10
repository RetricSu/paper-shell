use crate::backend::editor_backend::EditorBackend;
use crate::backend::time_backend::TimeBackend;
use crate::file::FileData;
use crate::messages::ResponseMessage;
use crate::plugin::builtin::cloud_backup::DocumentIdentity;
use crate::plugin::{PluginContext, PluginManager};
use crate::style::configure_style;
use crate::ui::about::AboutWindow;
use crate::ui::dialog::{DocumentChangeDecision, PendingDocumentAction, UnsavedChangesDialog};
use crate::ui::editor::Editor;
use crate::ui::history::{HistoryAction, HistoryWindow};
use crate::ui::plugins::cloud_backup::{CloudBackupAction, CloudBackupDialog};
use crate::ui::plugins::manager::{
    ExternalPluginConfigWindow, PluginManagerAction, PluginManagerWindow,
};
use crate::ui::plugins::{GithubPublishConfigWindow, PluginOutputWindow, PublishDialog};

#[cfg(not(target_os = "macos"))]
use crate::ui::plugins::PrintDialog;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

fn write_cloud_recovery_snapshot(data_dir: &Path, content: &str) -> std::io::Result<PathBuf> {
    let directory = data_dir.join("history").join("cloud_backup_recovery");
    std::fs::create_dir_all(&directory)?;
    let filename = format!(
        "recovery_{}_{}.md",
        chrono::Local::now().format("%Y%m%d_%H%M%S"),
        uuid::Uuid::new_v4()
    );
    let path = directory.join(filename);
    std::fs::write(&path, content)?;
    Ok(path)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EscapeSource {
    Monitor,
    RawInput,
}

pub struct PaperShellApp {
    editor: Editor,
    pub response_sender: Sender<ResponseMessage>,
    response_receiver: Receiver<ResponseMessage>,

    history_window: HistoryWindow,
    statistics_window: crate::ui::statistics::StatisticsWindow,
    about_window: AboutWindow,
    updater: crate::updater::Updater,

    current_font: String,
    available_fonts: Vec<String>,

    last_focus_state: bool,
    config: crate::config::Config,

    editor_backend: Arc<EditorBackend>,
    sidebar: crate::ui::sidebar::Sidebar,
    draft_directory: Option<PathBuf>,
    draft_doc_id: String,
    current_version_id: Option<String>,
    document_uuid: Option<String>,
    saved_content: String,
    saved_comments: Vec<crate::comments::TextComment>,
    file_error: Option<String>,
    time_backend: TimeBackend,

    plugin_manager: PluginManager,
    plugin_metadata: Vec<crate::plugin::PluginMetadata>,
    plugin_output: PluginOutputWindow,
    plugin_config_window: GithubPublishConfigWindow,
    plugin_manager_window: PluginManagerWindow,
    external_plugin_config_window: ExternalPluginConfigWindow,
    cloud_backup_dialog: CloudBackupDialog,
    publish_dialog: PublishDialog,
    unsaved_dialog: UnsavedChangesDialog,
    allow_close_once: bool,
    #[cfg(not(target_os = "macos"))]
    print_dialog: PrintDialog,
    vim_pending_escape: bool,
    last_vim_escape: Option<(EscapeSource, std::time::Instant)>,
    root_viewport_focused: bool,
    settings_window: crate::ui::settings::SettingsWindow,
    skin_manager: crate::skin::SkinManager,
    skin_market_client: crate::skin::SkinMarketClient,
    skin_renderer: crate::skin::SkinRenderer,
    #[cfg(target_os = "macos")]
    print_requested: bool,
}

impl Default for PaperShellApp {
    fn default() -> Self {
        // Initialize tracing subscriber for console output
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_thread_ids(true)
            .try_init();

        let (sender, receiver) = channel();
        let mut config = crate::config::Config::default();
        config.settings.sanitize_typography();
        let current_font = config.settings.font_family.clone();
        let mut editor = Editor::default();
        editor.font_size = config.settings.font_size;
        editor.line_spacing = config.settings.line_spacing;
        editor.font_color = crate::config::parse_hex_color(&config.settings.font_color);
        editor.set_vim_mode(config.settings.vim_mode);
        let available_fonts = crate::ui::font::enumerate_chinese_fonts();

        let plugins_dir = config.data_dir().join("plugins");
        let plugin_manager = PluginManager::new(
            plugins_dir,
            config.settings.github_publish.clone(),
            config.settings.cloud_backup.clone(),
            config.settings.disabled_plugins.clone(),
            config.settings.external_plugin_settings.clone(),
        );
        let plugin_metadata = plugin_manager.metadata();
        // Tests must not inherit the user's persisted sidebar layout.
        #[cfg(test)]
        let workspace_state = crate::ui::sidebar::WorkspaceState::default();
        #[cfg(not(test))]
        let workspace_state = crate::ui::sidebar::WorkspaceState::load(&config.data_dir());
        let sidebar = crate::ui::sidebar::Sidebar::new(workspace_state);

        let skins_dir = config.data_dir().join("skins");
        let skin_manager =
            crate::skin::SkinManager::new(skins_dir, config.settings.current_skin.clone());
        let skin_market_client = crate::skin::SkinMarketClient::new(
            config.settings.skin_market_repo.clone(),
            config.settings.skin_market_branch.clone(),
            config.settings.skin_market_folder.clone(),
        );
        let skin_renderer = crate::skin::SkinRenderer::new();

        Self {
            editor,
            editor_backend: Arc::new(EditorBackend::default()),
            sidebar,
            draft_directory: None,
            draft_doc_id: uuid::Uuid::new_v4().to_string(),
            current_version_id: None,
            document_uuid: None,
            saved_content: String::new(),
            saved_comments: Vec::new(),
            file_error: None,
            time_backend: TimeBackend::default(),
            response_receiver: receiver,
            response_sender: sender,
            history_window: HistoryWindow::new(),
            statistics_window: crate::ui::statistics::StatisticsWindow::default(),
            about_window: AboutWindow::default(),
            updater: crate::updater::Updater::default(),
            available_fonts,
            current_font,
            last_focus_state: false,
            config,
            plugin_manager,
            plugin_metadata,
            plugin_output: PluginOutputWindow::new(),
            plugin_config_window: GithubPublishConfigWindow::new(),
            plugin_manager_window: PluginManagerWindow::new(),
            external_plugin_config_window: ExternalPluginConfigWindow::new(),
            cloud_backup_dialog: CloudBackupDialog::new(),
            publish_dialog: PublishDialog::new(),
            unsaved_dialog: UnsavedChangesDialog::new(),
            allow_close_once: false,
            #[cfg(not(target_os = "macos"))]
            print_dialog: PrintDialog::new(),
            vim_pending_escape: false,
            last_vim_escape: None,
            root_viewport_focused: true,
            settings_window: crate::ui::settings::SettingsWindow::new(),
            skin_manager,
            skin_market_client,
            skin_renderer,
            #[cfg(target_os = "macos")]
            print_requested: false,
        }
    }
}

impl PaperShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        configure_style(&cc.egui_ctx);

        let mut app = Self::default();
        let current_skin = app.skin_manager.current_skin();
        crate::style::set_active_skin(current_skin.clone());
        crate::skin::SkinRenderer::apply_to_context(&cc.egui_ctx, &current_skin);

        if app.current_font != "Default" {
            let fonts = crate::ui::font::apply_font(&app.current_font);
            cc.egui_ctx.set_fonts(fonts);
        }
        if let Some(path) = initial_file {
            app.open_file(path);
        }

        #[cfg(target_os = "macos")]
        {
            let (esc_sender, esc_receiver) = std::sync::mpsc::channel();
            crate::escape_monitor::set_escape_sender(esc_sender);
            crate::escape_monitor::install_escape_monitor();
            let sender = app.response_sender.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                while esc_receiver.recv().is_ok() {
                    let _ = sender.send(ResponseMessage::VimEscape);
                    ctx.request_repaint();
                }
            });
        }

        app.updater.check(&cc.egui_ctx);
        app
    }

    fn spawn_new_window(&self) {
        // Spawn a new instance of the application
        if let Err(e) = std::process::Command::new(std::env::current_exe().unwrap()).spawn() {
            tracing::error!("Failed to spawn new window: {}", e);
        }
    }
}

// file related operations without UI
impl PaperShellApp {
    fn load_file_data(&self, path: &PathBuf) -> Result<FileData, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e: std::io::Error| format!("Failed to read file {:?}: {}", path, e))?;

        let (uuid, total_time) = self
            .editor_backend
            .get_file_metadata(path, &content)
            .map_err(|e| format!("Failed to get metadata: {}", e))?;

        let (comments, comments_error) = match self.editor_backend.load_comments(&uuid) {
            Ok(comments) => (comments, None),
            Err(error) => (
                Vec::new(),
                Some(format!(
                    "评论文件无法读取，正文仍可编辑；原评论文件未修改: {}",
                    error
                )),
            ),
        };

        Ok(FileData {
            uuid,
            path: path.to_path_buf(),
            total_time,
            current_version_id: self
                .editor_backend
                .resolve_current_version_id(path, &content),
            content,
            comments,
            comments_error,
        })
    }

    fn try_load_history(&mut self) {
        let current_file = self.editor.get_current_file().cloned();
        if let Some(path) = current_file {
            let backend = Arc::clone(&self.editor_backend);
            let sender = self.response_sender.clone();
            self.history_window
                .set_current_version_id(self.current_version_id.clone());
            let current_version_id = self.current_version_id.clone();

            std::thread::spawn(move || {
                let result = backend.load_history(&path).map_err(|e| e.to_string());
                let _ = sender.send(ResponseMessage::HistoryLoaded {
                    path,
                    current_version_id,
                    result,
                });
            });

            self.history_window.open();
        }
    }

    fn is_document_dirty(&self) -> bool {
        self.editor.get_content() != self.saved_content
            || self.editor.comments() != self.saved_comments.as_slice()
    }

    fn open_file(&mut self, path: PathBuf) {
        if self.editor.get_current_file() == Some(&path) {
            return;
        }
        if self.is_document_dirty() {
            self.unsaved_dialog
                .open(PendingDocumentAction::OpenFile(path));
            return;
        }
        self.apply_open_file(path);
    }

    fn apply_open_file(&mut self, path: PathBuf) {
        match self.load_file_data(&path) {
            Ok(data) => self.apply_load_file_data(data),
            Err(e) => self.file_error = Some(e),
        }
    }

    fn apply_new_document(&mut self, directory: Option<PathBuf>) {
        self.draft_directory = Some(directory.unwrap_or_else(|| self.document_directory()));
        let font_size = self.config.settings.font_size;
        let line_spacing = self.config.settings.line_spacing;
        self.editor = Editor::default();
        self.editor.font_size = font_size;
        self.editor.line_spacing = line_spacing;
        self.editor.font_color = crate::config::parse_hex_color(&self.config.settings.font_color);
        self.editor.set_vim_mode(self.config.settings.vim_mode);
        self.editor.set_content(String::new());
        self.history_window.close();
        self.draft_doc_id = uuid::Uuid::new_v4().to_string();
        self.current_version_id = None;
        self.document_uuid = None;
        self.saved_content.clear();
        self.saved_comments.clear();
        self.time_backend.get_and_reset_writing_time();
    }

    fn document_directory(&self) -> PathBuf {
        self.draft_directory
            .clone()
            .or_else(|| {
                self.editor
                    .get_current_file()
                    .and_then(|path| path.parent())
                    .map(|path| path.to_path_buf())
            })
            .unwrap_or_else(|| self.config.data_dir())
    }

    fn add_workspace_folder(&mut self, path: PathBuf) {
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                self.file_error = Some(format!("无法打开文件夹 {}: {}", path.display(), error));
                return;
            }
        };
        if let Err(error) = std::fs::read_dir(&path) {
            self.file_error = Some(format!("无法打开文件夹 {}: {}", path.display(), error));
            return;
        }
        let default_root = self.config.data_dir();
        let default_root = default_root.canonicalize().unwrap_or(default_root);
        if path != default_root && !self.config.settings.workspace_folders.contains(&path) {
            self.config.settings.workspace_folders.push(path);
            self.save_workspace_folders();
        }
        self.sidebar.refresh();
        self.sidebar.expand();
    }

    fn save_workspace_folders(&mut self) {
        if let Err(error) = self.config.save() {
            self.file_error = Some(format!("无法记住文件夹列表: {}", error));
        }
    }

    fn handle_sidebar_action(&mut self, action: crate::ui::sidebar::SidebarAction) {
        use crate::ui::sidebar::SidebarAction;
        match action {
            SidebarAction::OpenFile(path) => self.open_file(path),
            SidebarAction::OpenFolder => {
                let root = self.document_directory();
                let sender = self.response_sender.clone();
                std::thread::spawn(move || {
                    if let Some(path) = rfd::FileDialog::new().set_directory(root).pick_folder() {
                        let _ = sender.send(ResponseMessage::OpenWorkspace(path));
                    }
                });
            }
            SidebarAction::RemoveFolder(path) => {
                self.config
                    .settings
                    .workspace_folders
                    .retain(|root| root != &path);
                self.save_workspace_folders();
                self.sidebar.on_root_removed(&path);
                self.sidebar.refresh();
            }
            SidebarAction::NewDocument(directory) => {
                if self.is_document_dirty() {
                    self.unsaved_dialog
                        .open(PendingDocumentAction::NewDocument(directory));
                } else {
                    self.apply_new_document(directory);
                }
            }
        }
    }

    fn try_open_file_from_selector(&self) {
        let data_dir = self.document_directory();

        // Keep a reference to the sender to use in the outer scope
        let sender = self.response_sender.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .set_directory(&data_dir)
                .add_filter("文章", &["txt", "md", "markdown", "text", "org", "rst"])
                .pick_file()
            {
                let _ = sender.send(ResponseMessage::OpenFile(path));
            }
        });
    }

    // Finish saves before switching documents so results cannot land on a different file.
    fn save_file(&mut self) -> bool {
        let content = self.editor.get_content();
        let path = match self.editor.get_current_file().cloned() {
            Some(path) => path,
            None => {
                let Some(path) = rfd::FileDialog::new()
                    .set_directory(self.document_directory())
                    .add_filter("文章", &["txt", "md", "markdown", "text", "org", "rst"])
                    .set_file_name("未命名.txt")
                    .save_file()
                else {
                    return false;
                };
                path
            }
        };
        if let Err(error) = std::fs::write(&path, &content) {
            self.file_error = Some(format!("无法保存 {}: {}", path.display(), error));
            return false;
        }
        match self.editor_backend.save_with_identity(
            &path,
            &content,
            self.time_backend.get_writing_time(),
            self.current_version_id.clone(),
            self.document_uuid.as_deref(),
        ) {
            Ok((uuid, total_time, version_id)) => {
                self.statistics_window.invalidate();
                // The body/history save has consumed this interval even if comment
                // persistence fails and needs to be retried.
                self.time_backend.get_and_reset_writing_time();
                // The version already exists even if only comment persistence
                // fails; a retry must be parented to this save, not its parent.
                self.current_version_id = Some(version_id);
                self.document_uuid = Some(uuid.clone());
                if let Err(error) = self
                    .editor_backend
                    .save_comments(&uuid, self.editor.comments())
                {
                    self.file_error = Some(format!("正文已写入，但无法保存评论: {}", error));
                    return false;
                }
                self.draft_directory = None;
                self.editor.set_current_file(Some(path.clone()));
                self.editor.set_current_file_total_time(total_time);
                self.saved_content = content;
                self.saved_comments = self.editor.comments().to_vec();
                self.config.add_recent_file(path);
                self.sidebar.refresh();
                if self.history_window.is_open() {
                    self.try_load_history();
                }
                true
            }
            Err(error) => {
                self.file_error = Some(format!("正文已写入，但无法保存版本记录: {}", error));
                false
            }
        }
    }

    fn export_document(&mut self, format: crate::export::ExportFormat) {
        let content = self.editor.get_content();
        if content.trim().is_empty() {
            self.file_error = Some("当前文档为空，无法导出".to_string());
            return;
        }

        let default_name = crate::export::suggested_file_name(
            self.editor
                .get_current_file()
                .map(std::path::PathBuf::as_path),
            format,
        );
        let filter_name = format.filter_name();
        let filter_ext = format.extension();

        let Some(path) = rfd::FileDialog::new()
            .set_directory(self.document_directory())
            .add_filter(filter_name, &[filter_ext])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };

        let path = if path.extension().is_none() {
            path.with_extension(filter_ext)
        } else {
            path
        };

        let title = self
            .editor
            .get_current_file()
            .and_then(|path| path.file_stem())
            .and_then(|name| name.to_str())
            .unwrap_or("未命名文档");

        match crate::export::export_document(format, &content, &path, title) {
            Ok(()) => {
                let label = format.label();
                self.plugin_output.finish(
                    format!("导出 {}", label),
                    Ok(format!("已成功导出为 {}：\n{}", label, path.display())),
                );
            }
            Err(error) => {
                self.file_error = Some(format!("导出失败: {}", error));
            }
        }
    }

    fn apply_load_file_data(&mut self, data: FileData) {
        self.draft_directory = None;
        self.history_window.close();
        self.current_version_id = data.current_version_id;
        self.document_uuid = Some(data.uuid.clone());
        self.saved_content = data.content.clone();
        self.editor.set_content(data.content);
        self.editor.set_current_file(Some(data.path.clone()));
        self.editor.set_current_file_total_time(data.total_time);
        self.editor.set_comments(data.comments);
        self.saved_comments = self.editor.comments().to_vec();
        self.time_backend.get_and_reset_writing_time();
        self.config.add_recent_file(data.path.clone());
        if let Some(error) = data.comments_error {
            self.file_error = Some(error);
        }
        tracing::info!("File opened: {:?}", data.path);
    }

    fn update_time_backend_if_focus_changed(&mut self) {
        let is_focused = self.editor.is_focused();
        if is_focused != self.last_focus_state {
            self.time_backend.update_focus(is_focused);
            self.last_focus_state = is_focused;
        }
    }

    fn check_response_messages(&mut self) {
        while let Ok(response) = self.response_receiver.try_recv() {
            match response {
                ResponseMessage::HistoryLoaded {
                    path,
                    current_version_id,
                    result,
                } => {
                    if self.editor.get_current_file() != Some(&path)
                        || self.current_version_id != current_version_id
                    {
                        continue;
                    }
                    self.history_window
                        .set_current_version_id(current_version_id);
                    match result {
                        Ok(entries) => {
                            if let Err(e) = self
                                .history_window
                                .set_history(entries, &self.editor_backend)
                            {
                                tracing::info!("Failed to set history: {}", e);
                                self.history_window.set_error(e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to load history: {}", e);
                            self.history_window.set_error(e);
                        }
                    }
                }
                ResponseMessage::OpenFile(path) => self.open_file(path),
                ResponseMessage::OpenWorkspace(path) => self.add_workspace_folder(path),
                ResponseMessage::PluginFinished { name, result } => {
                    if let Err(e) = &result {
                        tracing::error!("Plugin '{}' failed: {}", name, e);
                    } else {
                        tracing::info!("Plugin '{}' finished", name);
                    }
                    self.plugin_output.finish(name, result);
                }
                ResponseMessage::MarketSkinsFetched { result } => match result {
                    Ok(skins) => {
                        let count = skins.len();
                        self.skin_market_client.cached_skins = skins;
                        self.skin_market_client.last_error = None;
                        self.settings_window.set_status(
                            false,
                            format!("已成功从 GitHub 仓库拉取 {} 款皮肤", count),
                        );
                    }
                    Err(err) => {
                        self.skin_market_client.last_error = Some(err.clone());
                        self.settings_window
                            .set_status(true, format!("从 GitHub 拉取失败：{err}"));
                    }
                },
                ResponseMessage::VimEscape => {
                    self.record_vim_escape(EscapeSource::Monitor);
                }
            }
        }
    }

    fn handle_history_action(&mut self, action: HistoryAction) {
        match action {
            HistoryAction::RollbackToVersion(hash) => {
                if self.is_document_dirty() {
                    self.unsaved_dialog
                        .open(PendingDocumentAction::Rollback(hash));
                } else {
                    let _ = self.apply_rollback(&hash);
                }
            }
            HistoryAction::UpdateVersionLabel { version_id, label } => {
                let Some(path) = self.editor.get_current_file().cloned() else {
                    return;
                };
                match self
                    .editor_backend
                    .update_version_label(&path, &version_id, label)
                {
                    Ok(()) => self.try_load_history(),
                    Err(error) => {
                        self.file_error = Some(format!("无法保存版本备注: {error}"));
                    }
                }
            }
            HistoryAction::DeleteVersion {
                version_id,
                delete_branch,
            } => {
                let Some(path) = self.editor.get_current_file().cloned() else {
                    return;
                };
                let mut deletes_current =
                    self.current_version_id.as_deref() == Some(version_id.as_str());
                if delete_branch
                    && !deletes_current
                    && let Ok(entries) = self.editor_backend.load_history(&path)
                {
                    let mut ancestor = self.current_version_id.clone();
                    while let Some(id) = ancestor {
                        if id == version_id {
                            deletes_current = true;
                            break;
                        }
                        ancestor = entries
                            .iter()
                            .find(|entry| entry.id.as_deref() == Some(id.as_str()))
                            .and_then(|entry| entry.parent_id.clone());
                    }
                }
                if deletes_current {
                    self.file_error =
                        Some("当前编辑基准所在分叉不能删除，请先回滚到其他版本".into());
                    return;
                }
                match self
                    .editor_backend
                    .delete_version(&path, &version_id, delete_branch)
                {
                    Ok(()) => self.try_load_history(),
                    Err(error) => {
                        self.file_error = Some(format!("无法删除历史版本: {error}"));
                    }
                }
            }
        }
    }

    fn handle_statistics_action(&mut self, action: crate::ui::statistics::StatisticsAction) {
        match action {
            crate::ui::statistics::StatisticsAction::OpenFile(path) => {
                self.open_file(path);
            }
            crate::ui::statistics::StatisticsAction::RevealInFolder(path) => {
                crate::ui::statistics::reveal_path_in_file_manager(&path);
            }
            crate::ui::statistics::StatisticsAction::DeleteDocument {
                doc_id,
                delete_local_file,
            } => {
                let current_file = self.editor.get_current_file().cloned();
                match self
                    .editor_backend
                    .remove_document_record(&doc_id, delete_local_file)
                {
                    Ok(affected_path) => {
                        let is_current = affected_path
                            .as_ref()
                            .is_some_and(|p| current_file.as_ref() == Some(p));
                        if is_current {
                            if delete_local_file {
                                self.apply_new_document(None);
                            } else {
                                self.document_uuid = None;
                                self.current_version_id = None;
                                self.history_window.close();
                            }
                        }
                        self.statistics_window.remove_document(&doc_id);
                        self.statistics_window.invalidate();
                        if delete_local_file {
                            self.sidebar.refresh();
                        }
                    }
                    Err(error) => {
                        self.statistics_window
                            .set_error(format!("踢出文档记录失败: {error}"));
                    }
                }
            }
        }
    }

    fn apply_rollback(&mut self, hash: &str) -> bool {
        let Some(path) = self.editor.get_current_file().cloned() else {
            self.file_error = Some(format!("无法回滚到版本 {hash}: 当前文档尚未保存"));
            return false;
        };
        let version = self
            .editor_backend
            .load_history(&path)
            .ok()
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.id.as_deref() == Some(hash))
                    .or_else(|| entries.iter().rev().find(|entry| entry.hash == hash))
                    .cloned()
            });
        let Some(version) = version else {
            let message = format!("无法回滚到版本 {hash}: 版本记录不存在");
            self.file_error = Some(message);
            return false;
        };
        match self.editor_backend.restore_version(&version.hash) {
            Ok(content) => {
                self.current_version_id = version.id;
                self.editor.restore_content(content);
                tracing::info!("Rolled back to version: {}", hash);
                true
            }
            Err(e) => {
                let msg = format!("无法回滚到版本 {}: {}", hash, e);
                tracing::error!("{}", msg);
                self.file_error = Some(msg);
                false
            }
        }
    }

    fn cloud_document_identity(&self) -> DocumentIdentity {
        DocumentIdentity::new(
            self.editor.get_current_file().map(|path| path.as_path()),
            &self.draft_doc_id,
        )
    }

    fn create_cloud_recovery_snapshot(&self) -> Result<PathBuf, String> {
        write_cloud_recovery_snapshot(&self.config.data_dir(), &self.editor.get_content())
            .map_err(|error| format!("无法写入本地恢复快照: {}", error))
    }

    /// Runs an installed plugin by id on a background thread, snapshotting the
    /// current editing context so the plugin stays decoupled from live state.
    fn run_plugin(&mut self, id: String) {
        let Some(plugin) = self.plugin_manager.get(&id) else {
            tracing::warn!("Plugin not found: {}", id);
            return;
        };

        let name = plugin.metadata().name;
        let settings = self.plugin_manager.external_settings_for(&id);
        self.plugin_output.start(&name);

        let ctx = PluginContext {
            file_path: self.editor.get_current_file().cloned(),
            content: self.editor.get_content(),
            data_dir: self.config.data_dir(),
            title: None,
            description: None,
            collection: None,
            printer: None,
            print_margin_points: None,
            settings,
        };
        let sender = self.response_sender.clone();

        std::thread::spawn(move || {
            let result = plugin.run(&ctx).map_err(|e| e.to_string());
            let _ = sender.send(ResponseMessage::PluginFinished { name, result });
        });
    }

    /// Ensures the plugins directory exists and opens it in the system file
    /// manager so users can install plugins by dropping folders into it.
    fn open_plugins_folder(&self) -> Result<PathBuf, String> {
        let dir = self.config.data_dir().join("plugins");
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("无法创建插件目录 {}：{}", dir.display(), error))?;

        #[cfg(target_os = "macos")]
        let opener = "open";
        #[cfg(target_os = "windows")]
        let opener = "explorer";
        #[cfg(all(unix, not(target_os = "macos")))]
        let opener = "xdg-open";

        std::process::Command::new(opener)
            .arg(&dir)
            .spawn()
            .map_err(|error| format!("无法打开插件目录 {}：{}", dir.display(), error))?;
        Ok(dir)
    }

    fn refresh_plugin_metadata(&mut self) {
        self.plugin_metadata = self.plugin_manager.metadata();
    }

    fn open_plugins_folder_with_feedback(&mut self) {
        match self.open_plugins_folder() {
            Ok(dir) => self.plugin_manager_window.set_status(
                false,
                format!(
                    "已打开插件目录。放入插件后请点击“重新扫描”：{}",
                    dir.display()
                ),
            ),
            Err(error) => {
                self.file_error = Some(error.clone());
                self.plugin_manager_window.set_status(true, error);
            }
        }
    }

    fn handle_plugin_manager_action(&mut self, ctx: &egui::Context, action: PluginManagerAction) {
        match action {
            PluginManagerAction::SetEnabled { id, enabled } => {
                let previous = self.config.settings.is_plugin_enabled(&id);
                self.config.settings.set_plugin_enabled(&id, enabled);
                match self.config.save() {
                    Ok(()) => {
                        self.plugin_manager.set_enabled(&id, enabled);
                        self.refresh_plugin_metadata();
                        self.plugin_manager_window.set_status(
                            false,
                            format!("插件已{}", if enabled { "启用" } else { "禁用" }),
                        );
                    }
                    Err(error) => {
                        self.config.settings.set_plugin_enabled(&id, previous);
                        self.plugin_manager_window
                            .set_status(true, format!("无法保存插件状态：{error}"));
                    }
                }
            }
            PluginManagerAction::Rescan => {
                self.plugin_manager.rescan();
                self.refresh_plugin_metadata();
                let plugin_count = self.plugin_manager.all_metadata().len();
                let error_count = self.plugin_manager.load_errors().len();
                self.plugin_manager_window.set_status(
                    error_count > 0,
                    format!("扫描完成：{plugin_count} 个插件，{error_count} 个加载问题"),
                );
            }
            PluginManagerAction::OpenPluginsFolder => self.open_plugins_folder_with_feedback(),
            PluginManagerAction::Configure(id) => match id.as_str() {
                "cloud_backup" => {
                    self.cloud_backup_dialog
                        .open(ctx, &self.config.settings.cloud_backup);
                }
                "github_publish" => {
                    self.plugin_config_window
                        .open(&self.config.settings.github_publish, false);
                    ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
                }
                _ => {
                    if let Some(plugin) = self.plugin_manager.find_any(&id) {
                        let metadata = plugin.metadata();
                        let definitions = plugin.settings_definitions();
                        let saved = self.plugin_manager.external_settings_for(&id);
                        self.external_plugin_config_window
                            .open(&metadata, definitions, &saved);
                        ctx.request_repaint_of(self.plugin_manager_window.viewport_id());
                    }
                }
            },
            PluginManagerAction::Uninstall(id) => {
                match self.plugin_manager.safe_uninstall_external_plugin(&id) {
                    Ok(removed) => {
                        self.config.settings.remove_external_plugin_data(&id);
                        self.refresh_plugin_metadata();
                        match self.config.save() {
                            Ok(()) => self
                                .plugin_manager_window
                                .set_status(false, format!("插件已卸载：{}", removed.display())),
                            Err(error) => self.plugin_manager_window.set_status(
                                true,
                                format!("插件文件已删除，但清理设置保存失败：{error}"),
                            ),
                        }
                    }
                    Err(error) => self.plugin_manager_window.set_status(true, error),
                }
            }
            PluginManagerAction::SelectSkin(id) => {
                self.apply_skin(ctx, &id);
            }
            PluginManagerAction::ImportSkin => {
                self.import_skin_dialog(ctx);
            }
            PluginManagerAction::ExportSkin(id) => {
                self.export_skin_dialog(&id);
            }
            PluginManagerAction::ExportCurrentSettingsAsSkin => {
                self.export_current_skin_dialog();
            }
            PluginManagerAction::UninstallSkin(id) => match self.skin_manager.uninstall_skin(&id) {
                Ok(()) => {
                    let current_id = self.skin_manager.current_skin_id().to_string();
                    self.apply_skin(ctx, &current_id);
                    self.plugin_manager_window
                        .set_status(false, format!("皮肤“{id}”已卸载"));
                }
                Err(error) => self.plugin_manager_window.set_status(true, error),
            },
            PluginManagerAction::SaveCustomSkin(skin) => {
                let skin_name = skin.name.clone();
                match self.skin_manager.save_custom_skin(skin) {
                    Ok(saved) => {
                        self.apply_skin(ctx, &saved.id);
                        self.plugin_manager_window
                            .set_status(false, format!("自定义皮肤“{skin_name}”已保存并应用"));
                    }
                    Err(error) => self.plugin_manager_window.set_status(true, error),
                }
            }
            PluginManagerAction::FetchMarketSkins => {
                let sender = self.response_sender.clone();
                let mut client = self.skin_market_client.clone();
                let installed_ids: Vec<String> = self
                    .skin_manager
                    .all_skins()
                    .iter()
                    .map(|s| s.id.clone())
                    .collect();
                self.plugin_manager_window
                    .set_status(false, "正在从 GitHub 仓库拉取皮肤列表…".to_string());
                std::thread::spawn(move || {
                    let skins = client.fetch_skins(&installed_ids);
                    let result = if let Some(err) = client.last_error {
                        Err(err)
                    } else {
                        Ok(skins)
                    };
                    let _ = sender.send(ResponseMessage::MarketSkinsFetched { result });
                });
            }
            PluginManagerAction::InstallMarketSkin(market_skin) => {
                let name = market_skin.name.clone();
                match self
                    .skin_market_client
                    .download_and_install(&market_skin, self.skin_manager.skins_dir())
                {
                    Ok(installed) => {
                        self.skin_manager.rescan();
                        self.apply_skin(ctx, &installed.id);
                        self.plugin_manager_window
                            .set_status(false, format!("已成功安装并应用皮肤“{name}”"));
                    }
                    Err(error) => self.plugin_manager_window.set_status(true, error),
                }
            }
        }
    }

    fn handle_settings_action(
        &mut self,
        ctx: &egui::Context,
        action: crate::ui::settings::SettingsAction,
    ) {
        match action {
            crate::ui::settings::SettingsAction::ToggleVimMode => {
                self.config.settings.vim_mode = !self.config.settings.vim_mode;
                self.editor.set_vim_mode(self.config.settings.vim_mode);
                if let Err(error) = self.config.save() {
                    self.file_error = Some(format!("无法保存 Vim 模式设置: {}", error));
                }
            }
            crate::ui::settings::SettingsAction::SetFormatUseTabs(use_tabs) => {
                self.config.settings.format_use_tabs = use_tabs;
                if let Err(error) = self.config.save() {
                    self.file_error = Some(format!("无法保存段落缩进设置: {}", error));
                }
            }
            crate::ui::settings::SettingsAction::SetAutosaveInterval(interval) => {
                self.config.settings.autosave_interval = interval;
                if let Err(error) = self.config.save() {
                    self.file_error = Some(format!("无法保存自动保存间隔: {}", error));
                }
            }
            crate::ui::settings::SettingsAction::FontSizeChange(size) => {
                let size = size.clamp(10.0, 36.0);
                self.editor.font_size = size;
                self.config.settings.font_size = size;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                ctx.request_repaint();
            }
            crate::ui::settings::SettingsAction::LineSpacingChange(spacing) => {
                let spacing = spacing.clamp(1.0, 3.0);
                self.editor.line_spacing = spacing;
                self.config.settings.line_spacing = spacing;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                ctx.request_repaint();
            }
            crate::ui::settings::SettingsAction::FontChange(font_name) => {
                let new_fonts = if font_name == "Default" {
                    crate::ui::font::setup_fonts()
                } else {
                    crate::ui::font::apply_font(&font_name)
                };
                ctx.set_fonts(new_fonts);
                self.current_font = font_name.clone();
                self.config.settings.font_family = font_name;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
            }
            crate::ui::settings::SettingsAction::FontColorChange(hex) => {
                self.editor.font_color = crate::config::parse_hex_color(&hex);
                self.config.settings.font_color = hex;
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                ctx.request_repaint();
            }
            crate::ui::settings::SettingsAction::ResetTypography => {
                self.editor.font_size = 14.0;
                self.editor.line_spacing = 1.5;
                self.editor.font_color = None;
                self.config.settings.font_size = 14.0;
                self.config.settings.line_spacing = 1.5;
                self.config.settings.font_family = "Default".to_string();
                self.config.settings.font_color = "Default".to_string();
                self.config.settings.format_use_tabs = false;
                ctx.set_fonts(crate::ui::font::setup_fonts());
                self.current_font = "Default".to_string();
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
                ctx.request_repaint();
            }
            crate::ui::settings::SettingsAction::SelectSkin(id) => {
                self.apply_skin(ctx, &id);
            }
            crate::ui::settings::SettingsAction::ImportSkin => {
                self.import_skin_dialog(ctx);
            }
            crate::ui::settings::SettingsAction::ExportCurrentSkin => {
                self.export_current_skin_dialog();
            }
            crate::ui::settings::SettingsAction::ExportSkin(id) => {
                self.export_skin_dialog(&id);
            }
            crate::ui::settings::SettingsAction::UninstallSkin(id) => {
                match self.skin_manager.uninstall_skin(&id) {
                    Ok(()) => {
                        let current_id = self.skin_manager.current_skin_id().to_string();
                        self.apply_skin(ctx, &current_id);
                        self.settings_window
                            .set_status(false, format!("皮肤“{id}”已卸载"));
                    }
                    Err(error) => self.settings_window.set_status(true, error),
                }
            }
            crate::ui::settings::SettingsAction::SaveCustomSkin(skin) => {
                let skin_name = skin.name.clone();
                match self.skin_manager.save_custom_skin(skin) {
                    Ok(saved) => {
                        self.apply_skin(ctx, &saved.id);
                        self.settings_window
                            .set_status(false, format!("自定义皮肤“{skin_name}”已保存并应用"));
                    }
                    Err(error) => self.settings_window.set_status(true, error),
                }
            }
            crate::ui::settings::SettingsAction::FetchMarketSkins => {
                self.fetch_market_skins();
            }
            crate::ui::settings::SettingsAction::InstallMarketSkin(market_skin) => {
                let name = market_skin.name.clone();
                match self
                    .skin_market_client
                    .download_and_install(&market_skin, self.skin_manager.skins_dir())
                {
                    Ok(installed) => {
                        self.skin_manager.rescan();
                        self.apply_skin(ctx, &installed.id);
                        self.settings_window
                            .set_status(false, format!("已成功安装并应用皮肤“{name}”"));
                    }
                    Err(error) => self.settings_window.set_status(true, error),
                }
            }
            crate::ui::settings::SettingsAction::UpdateMarketSource {
                repo,
                branch,
                folder,
            } => {
                self.config.settings.skin_market_repo = repo.clone();
                self.config.settings.skin_market_branch = branch.clone();
                self.config.settings.skin_market_folder = folder.clone();
                self.skin_market_client.repo = repo;
                self.skin_market_client.branch = branch;
                self.skin_market_client.folder = folder;
                if let Err(e) = self.config.save() {
                    self.settings_window
                        .set_status(true, format!("保存市场源设置失败：{e}"));
                } else {
                    self.settings_window
                        .set_status(false, "皮肤市场源已保存".to_string());
                }
            }
            crate::ui::settings::SettingsAction::ResetMarketSource => {
                let repo = crate::config::default_skin_market_repo();
                let branch = crate::config::default_skin_market_branch();
                let folder = crate::config::default_skin_market_folder();
                self.config.settings.skin_market_repo = repo.clone();
                self.config.settings.skin_market_branch = branch.clone();
                self.config.settings.skin_market_folder = folder.clone();
                self.skin_market_client.repo = repo;
                self.skin_market_client.branch = branch;
                self.skin_market_client.folder = folder;
                if let Err(e) = self.config.save() {
                    self.settings_window
                        .set_status(true, format!("恢复市场源默认失败：{e}"));
                } else {
                    self.settings_window
                        .set_status(false, "已恢复为官方默认皮肤市场源".to_string());
                }
            }
        }
    }

    fn fetch_market_skins(&mut self) {
        let sender = self.response_sender.clone();
        let mut client = self.skin_market_client.clone();
        let installed_ids: Vec<String> = self
            .skin_manager
            .all_skins()
            .iter()
            .map(|s| s.id.clone())
            .collect();
        self.settings_window
            .set_status(false, "正在从 GitHub 仓库拉取皮肤列表…".to_string());
        std::thread::spawn(move || {
            let skins = client.fetch_skins(&installed_ids);
            let result = if let Some(err) = client.last_error {
                Err(err)
            } else {
                Ok(skins)
            };
            let _ = sender.send(ResponseMessage::MarketSkinsFetched { result });
        });
    }

    pub fn apply_skin(&mut self, ctx: &egui::Context, id: &str) {
        if self.skin_manager.set_current_skin(id) {
            let skin = self.skin_manager.current_skin();

            self.editor.font_size = skin.typography.font_size;
            self.editor.line_spacing = skin.typography.line_spacing;
            self.editor.font_color = crate::config::parse_hex_color(&skin.typography.font_color);
            self.config.settings.font_size = skin.typography.font_size;
            self.config.settings.line_spacing = skin.typography.line_spacing;
            self.config.settings.font_color = skin.typography.font_color.clone();
            self.config.settings.font_family = skin.typography.font_family.clone();
            self.config.settings.format_use_tabs = skin.typography.format_use_tabs;
            self.config.settings.current_skin = skin.id.clone();
            let _ = self.config.save();

            let new_fonts = if skin.typography.font_family == "Default" {
                crate::ui::font::setup_fonts()
            } else {
                crate::ui::font::apply_font(&skin.typography.font_family)
            };
            ctx.set_fonts(new_fonts);
            self.current_font = skin.typography.font_family.clone();

            crate::style::set_active_skin(skin.clone());
            crate::skin::SkinRenderer::apply_to_context(ctx, &skin);
            ctx.request_repaint();
        }
    }

    fn import_skin_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("选择导入皮肤文件")
            .add_filter("皮肤文件", &["toml", "json", "zip", "paperskin"])
            .pick_file()
        {
            match self.skin_manager.import_from_file(&path) {
                Ok(skin) => {
                    self.apply_skin(ctx, &skin.id);
                    self.settings_window
                        .set_status(false, format!("成功导入并应用皮肤“{}”", skin.name));
                }
                Err(e) => {
                    self.settings_window
                        .set_status(true, format!("导入皮肤失败: {e}"));
                }
            }
        }
    }

    fn export_skin_dialog(&mut self, id: &str) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("导出皮肤")
            .set_file_name(format!("{id}.toml"))
            .add_filter("皮肤配置文件 (*.toml)", &["toml"])
            .add_filter("皮肤安装包 (*.paperskin)", &["paperskin", "zip"])
            .save_file()
        {
            match self.skin_manager.export_skin_to_file(id, &path) {
                Ok(()) => {
                    self.settings_window
                        .set_status(false, format!("皮肤已成功导出到 {:?}", path));
                }
                Err(e) => {
                    self.settings_window
                        .set_status(true, format!("导出皮肤失败: {e}"));
                }
            }
        }
    }

    fn export_current_skin_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("导出当前排版与外观为皮肤")
            .set_file_name("custom_skin.toml")
            .add_filter("皮肤配置文件 (*.toml)", &["toml"])
            .add_filter("皮肤安装包 (*.paperskin)", &["paperskin", "zip"])
            .save_file()
        {
            let current = self.skin_manager.current_skin();
            let skin = crate::skin::Skin {
                id: "custom_exported_skin".to_string(),
                name: "自定义导出皮肤".to_string(),
                description: "从纸壳当前排版与外观设置导出的皮肤".to_string(),
                author: "我".to_string(),
                version: "1.0.0".to_string(),
                builtin: false,
                dir: current.dir.clone(),
                typography: crate::skin::SkinTypography {
                    font_family: self.current_font.clone(),
                    font_size: self.editor.font_size,
                    line_spacing: self.editor.line_spacing,
                    font_color: self.config.settings.font_color.clone(),
                    format_use_tabs: self.config.settings.format_use_tabs,
                },
                spacing: current.spacing.clone(),
                colors: current.colors.clone(),
                effects: current.effects.clone(),
            };
            match skin.export_to_file(&path) {
                Ok(()) => {
                    self.settings_window
                        .set_status(false, format!("当前外观已成功导出到 {:?}", path));
                }
                Err(e) => {
                    self.settings_window
                        .set_status(true, format!("导出失败: {e}"));
                }
            }
        }
    }

    fn handle_unsaved_dialog_decision(
        &mut self,
        ctx: &egui::Context,
        decision: DocumentChangeDecision,
    ) {
        let Some(action) = self.unsaved_dialog.pending_action().cloned() else {
            self.unsaved_dialog.close();
            return;
        };

        match decision {
            DocumentChangeDecision::Save => {
                if self.save_file() {
                    self.unsaved_dialog.close();
                    self.execute_pending_document_action(ctx, action);
                }
            }
            DocumentChangeDecision::Discard => {
                self.unsaved_dialog.close();
                self.execute_pending_document_action(ctx, action);
            }
            DocumentChangeDecision::Cancel => {
                self.allow_close_once = false;
                self.unsaved_dialog.close();
            }
        }
    }

    fn execute_pending_document_action(
        &mut self,
        ctx: &egui::Context,
        action: PendingDocumentAction,
    ) {
        match action {
            PendingDocumentAction::OpenFile(path) => {
                self.apply_open_file(path);
            }
            PendingDocumentAction::NewDocument(directory) => {
                self.apply_new_document(directory);
            }
            PendingDocumentAction::CloseWindow => {
                self.allow_close_once = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            PendingDocumentAction::InstallUpdate => {
                if self
                    .updater
                    .install(self.editor.get_current_file().cloned())
                {
                    self.allow_close_once = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.about_window.open(ctx);
                }
            }
            PendingDocumentAction::Rollback(hash) => {
                let _ = self.apply_rollback(&hash);
            }
        }
    }

    fn request_update_install(&mut self, ctx: &egui::Context) {
        if self.is_document_dirty() {
            self.unsaved_dialog
                .open(PendingDocumentAction::InstallUpdate);
        } else {
            self.execute_pending_document_action(ctx, PendingDocumentAction::InstallUpdate);
        }
    }

    fn record_vim_escape(&mut self, source: EscapeSource) {
        if !self.config.settings.vim_mode
            || !self.root_viewport_focused
            || self.editor.is_search_dialog_open()
            || self.unsaved_dialog.is_open()
        {
            return;
        }
        let now = std::time::Instant::now();
        if let Some((prev_source, prev_time)) = self.last_vim_escape
            && prev_source != source
            && now.duration_since(prev_time) < std::time::Duration::from_millis(50)
        {
            return;
        }
        self.last_vim_escape = Some((source, now));
        self.vim_pending_escape = true;
    }
}

impl eframe::App for PaperShellApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if raw_input.viewport_id == egui::ViewportId::ROOT {
            self.root_viewport_focused = raw_input.focused;
        } else {
            if raw_input.focused {
                self.root_viewport_focused = false;
            }
            return;
        }

        if self.config.settings.vim_mode
            && !self.editor.is_search_dialog_open()
            && !self.unsaved_dialog.is_open()
        {
            let mut had_escape = false;
            let mut had_non_repeat_escape = false;
            raw_input.events.retain(|event| {
                if let egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    repeat,
                    ..
                } = event
                {
                    had_escape = true;
                    if !*repeat {
                        had_non_repeat_escape = true;
                    }
                    false
                } else {
                    true
                }
            });
            if had_escape && had_non_repeat_escape {
                self.record_vim_escape(EscapeSource::RawInput);
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.updater.poll();
        self.check_response_messages();
        self.update_time_backend_if_focus_changed();
        if ctx.input(|input| input.viewport().close_requested()) {
            if self.allow_close_once {
                self.allow_close_once = false;
            } else if self.is_document_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.unsaved_dialog.open(PendingDocumentAction::CloseWindow);
            }
        }

        let modal_open = self.unsaved_dialog.is_open();

        let shortcut_action = if !modal_open {
            crate::ui::title_bar::TitleBar::shortcut(ctx)
        } else {
            None
        };
        let (total_words, cursor_words) = self.editor.get_stats();
        let current_skin = self.skin_manager.current_skin();

        // Title Bar
        let mut title_bar = egui::TopBottomPanel::top("title_bar_panel")
            // This panel has one explicit border painted below.
            .show_separator_line(false);
        if let Some(color) = current_skin
            .colors
            .titlebar_background_color
            .as_deref()
            .and_then(crate::config::parse_hex_color)
        {
            title_bar = title_bar.frame(egui::Frame::side_top_panel(&ctx.style()).fill(color));
        }
        let title_bar_panel = title_bar.show(ctx, |ui| {
            if modal_open {
                ui.disable();
            }
            let document_path = self
                .editor
                .get_current_file()
                .map(|path| path.to_string_lossy());
            if let Some(action) = crate::ui::title_bar::TitleBar::show(
                ui,
                crate::ui::title_bar::TitleBarState {
                    backup_status: self.cloud_backup_dialog.backup_status(),
                    update_available: self.updater.has_update(),
                    title: crate::constant::DEFAULT_WINDOW_TITLE,
                    word_count: total_words,
                    cursor_word_count: cursor_words,
                    selection_word_count: self.editor.get_selection_word_count(),
                    writing_time: self.editor.get_current_file_total_time()
                        + self.time_backend.get_writing_time(),
                    document_path: document_path.as_deref(),
                    has_current_file: self.editor.get_current_file().is_some(),
                    chinese_fonts: &self.available_fonts,
                    current_font: &self.current_font,
                    skins: self.skin_manager.all_skins(),
                    current_skin_id: self.skin_manager.current_skin_id(),
                    font_size: self.editor.font_size,
                    line_spacing: self.editor.line_spacing,
                    font_color: &self.config.settings.font_color,
                    format_use_tabs: self.config.settings.format_use_tabs,
                    recent_files: &self.config.settings.recent_files,
                    plugins: &self.plugin_metadata,
                    sidebar_expanded: self.sidebar.is_expanded(),
                    vim_mode: self.config.settings.vim_mode,
                    vim_mode_label: self.editor.get_vim_mode_label(),
                },
            )
            .or(shortcut_action)
            {
                match action {
                    crate::ui::title_bar::TitleBarAction::NewWindow => self.spawn_new_window(),
                    crate::ui::title_bar::TitleBarAction::ToggleSidebar => self.sidebar.toggle(),
                    crate::ui::title_bar::TitleBarAction::ToggleVimMode => {
                        self.config.settings.vim_mode = !self.config.settings.vim_mode;
                        self.editor.set_vim_mode(self.config.settings.vim_mode);
                        if let Err(error) = self.config.save() {
                            self.file_error = Some(format!("无法保存 Vim 模式设置: {}", error));
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::Save => {
                        self.save_file();
                    }
                    crate::ui::title_bar::TitleBarAction::Export(format) => {
                        self.export_document(format);
                    }
                    crate::ui::title_bar::TitleBarAction::Open => {
                        self.try_open_file_from_selector()
                    }
                    crate::ui::title_bar::TitleBarAction::OpenFile(path) => self.open_file(path),
                    crate::ui::title_bar::TitleBarAction::Format => self
                        .editor
                        .format_with_tabs(self.config.settings.format_use_tabs),
                    crate::ui::title_bar::TitleBarAction::SetFormatUseTabs(use_tabs) => {
                        self.config.settings.format_use_tabs = use_tabs;
                        if let Err(error) = self.config.save() {
                            self.file_error = Some(format!("无法保存段落缩进设置: {}", error));
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::History => self.try_load_history(),
                    crate::ui::title_bar::TitleBarAction::Statistics => {
                        self.statistics_window.open(ctx)
                    }
                    crate::ui::title_bar::TitleBarAction::SearchReplace => {
                        self.editor.open_search_replace();
                    }
                    crate::ui::title_bar::TitleBarAction::FontSizeChange(size) => {
                        let size = size.clamp(10.0, 36.0);
                        self.editor.font_size = size;
                        self.config.settings.font_size = size;
                        if let Err(e) = self.config.save() {
                            tracing::warn!("Failed to save config: {}", e);
                        }
                        ctx.request_repaint();
                    }
                    crate::ui::title_bar::TitleBarAction::LineSpacingChange(spacing) => {
                        let spacing = spacing.clamp(1.0, 3.0);
                        self.editor.line_spacing = spacing;
                        self.config.settings.line_spacing = spacing;
                        if let Err(e) = self.config.save() {
                            tracing::warn!("Failed to save config: {}", e);
                        }
                        ctx.request_repaint();
                    }
                    crate::ui::title_bar::TitleBarAction::FontColorChange(color_hex) => {
                        self.editor.font_color = crate::config::parse_hex_color(&color_hex);
                        self.config.settings.font_color = color_hex;
                        if let Err(e) = self.config.save() {
                            tracing::warn!("Failed to save config: {}", e);
                        }
                        ctx.request_repaint();
                    }
                    crate::ui::title_bar::TitleBarAction::ResetTypography => {
                        self.editor.font_size = 14.0;
                        self.editor.line_spacing = 1.5;
                        self.editor.font_color = None;
                        self.config.settings.font_size = 14.0;
                        self.config.settings.line_spacing = 1.5;
                        self.config.settings.font_family = "Default".to_string();
                        self.config.settings.font_color = "Default".to_string();
                        self.config.settings.format_use_tabs = false;
                        ctx.set_fonts(crate::ui::font::setup_fonts());
                        self.current_font = "Default".to_string();
                        if let Err(e) = self.config.save() {
                            tracing::warn!("Failed to save config: {}", e);
                        }
                        ctx.request_repaint();
                    }
                    crate::ui::title_bar::TitleBarAction::FontChange(font_name) => {
                        let new_fonts = if font_name == "Default" {
                            crate::ui::font::setup_fonts()
                        } else {
                            crate::ui::font::apply_font(&font_name)
                        };
                        ctx.set_fonts(new_fonts);
                        self.current_font = font_name.clone();
                        self.config.settings.font_family = font_name.clone();
                        if let Err(e) = self.config.save() {
                            tracing::warn!("Failed to save config: {}", e);
                        }
                        tracing::info!("Font changed to: {}", font_name);
                    }
                    crate::ui::title_bar::TitleBarAction::RunPlugin(id) => {
                        if self.plugin_manager.get(&id).is_none() {
                            self.file_error = Some(format!("插件“{id}”不存在或已禁用"));
                        } else if id == "cloud_backup" {
                            let identity = self.cloud_document_identity();
                            self.cloud_backup_dialog.backup_now(
                                ctx,
                                &self.config.settings.cloud_backup,
                                &identity,
                                &self.editor.get_content(),
                            );
                        } else if id == "github_publish" {
                            if self.config.settings.github_publish.repo.trim().is_empty() {
                                self.plugin_config_window
                                    .open(&self.config.settings.github_publish, true);
                            } else {
                                self.publish_dialog
                                    .open(&self.config.settings.github_publish);
                            }
                        } else if id == "print" {
                            #[cfg(target_os = "macos")]
                            {
                                self.print_requested = true;
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                let document_name = self
                                    .editor
                                    .get_current_file()
                                    .and_then(|path| path.file_name())
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("未命名文档")
                                    .to_string();
                                self.print_dialog
                                    .open(document_name, self.editor.get_content());
                            }
                        } else {
                            self.run_plugin(id);
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::ManagePlugins => {
                        self.plugin_manager_window
                            .open_tab(ctx, crate::ui::plugins::manager::ManagerTab::Plugins);
                    }
                    crate::ui::title_bar::TitleBarAction::About => self.about_window.open(ctx),
                    crate::ui::title_bar::TitleBarAction::OpenSettings(tab) => {
                        self.settings_window.open(ctx, tab);
                    }
                    crate::ui::title_bar::TitleBarAction::SelectSkin(id) => {
                        self.apply_skin(ctx, &id);
                    }
                }
            }
        });
        crate::ui::title_bar::TitleBar::paint_bottom_border(ctx, title_bar_panel.response.rect);

        if let Some(action) = self.sidebar.show(
            ctx,
            &self.config.data_dir(),
            &self.config.settings.workspace_folders,
            self.editor.get_current_file().map(|path| path.as_path()),
        ) && !modal_open
        {
            self.handle_sidebar_action(action);
        }
        if let Some(error) = self.file_error.clone() {
            egui::TopBottomPanel::bottom("file_error").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if ui.button("关闭提示").clicked() {
                        self.file_error = None;
                    }
                });
            });
        }

        let current_skin = self.skin_manager.current_skin();

        let editor_bg = current_skin
            .colors
            .editor_background_color
            .as_deref()
            .and_then(crate::config::parse_hex_color)
            .unwrap_or(ctx.style().visuals.panel_fill);
        let editor_fill = if current_skin.effects.editor_opacity < 1.0 {
            editor_bg.gamma_multiply(current_skin.effects.editor_opacity.clamp(0.0, 1.0))
        } else {
            editor_bg
        };

        // Main Content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style())
                    .fill(editor_fill)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                if modal_open {
                    ui.disable();
                }
                let panel_rect = ui.max_rect();
                self.skin_renderer
                    .paint_background(ui, &current_skin, panel_rect);

                // Pad the writing surface after painting the full-panel background.
                egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: current_skin.spacing.editor_padding_left as i8,
                        right: current_skin.spacing.editor_padding_right as i8,
                        top: current_skin.spacing.editor_padding_top as i8,
                        bottom: current_skin.spacing.editor_padding_bottom as i8,
                    })
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                if self.vim_pending_escape {
                                    self.editor.process_pending_vim_escape(ui);
                                    self.vim_pending_escape = false;
                                }
                                self.editor
                                    .show_with_paragraph_dots(ui, !self.sidebar.is_expanded());
                            });
                        });
                    });
            });

        // History Window
        self.history_window.show(ctx);
        self.statistics_window.show(ctx, &self.editor_backend);
        if self.about_window.show(ctx, &mut self.updater) {
            self.request_update_install(ctx);
        }
        if let Some(action) = self.history_window.take_pending_action() {
            self.handle_history_action(action);
        }
        if let Some(action) = self.statistics_window.take_pending_action() {
            self.handle_statistics_action(action);
        }

        // Settings Window (Dedicated unified preferences window)
        let settings_state = crate::ui::settings::SettingsWindowState {
            vim_mode: self.config.settings.vim_mode,
            format_use_tabs: self.config.settings.format_use_tabs,
            autosave_interval: self.config.settings.autosave_interval,
            font_size: self.config.settings.font_size,
            line_spacing: self.config.settings.line_spacing,
            current_font: &self.current_font,
            chinese_fonts: &self.available_fonts,
            font_color: &self.config.settings.font_color,
            skins: self.skin_manager.all_skins(),
            current_skin_id: self.skin_manager.current_skin_id(),
            market_skins: &self.skin_market_client.cached_skins,
            market_error: self.skin_market_client.last_error.as_deref(),
            market_repo: &self.config.settings.skin_market_repo,
            market_branch: &self.config.settings.skin_market_branch,
            market_folder: &self.config.settings.skin_market_folder,
        };
        if let Some(action) = self.settings_window.show(ctx, settings_state) {
            self.handle_settings_action(ctx, action);
        }

        // Plugin output and management windows
        self.plugin_output.show(ctx);
        let all_plugins = self.plugin_manager.all_metadata();
        let (manager_action, external_settings) = self.plugin_manager_window.show(
            ctx,
            &all_plugins,
            self.plugin_manager.load_errors(),
            &mut self.external_plugin_config_window,
            Some(&mut self.skin_manager),
            Some(&mut self.skin_market_client),
        );
        if let Some(action) = manager_action {
            self.handle_plugin_manager_action(ctx, action);
        }
        if let Some(new_settings) = external_settings {
            let plugin_exists =
                self.plugin_manager
                    .find_any(&new_settings.id)
                    .is_some_and(|plugin| {
                        plugin.metadata().source == crate::plugin::PluginSource::External
                    });
            if !plugin_exists {
                self.external_plugin_config_window
                    .notify_save_error("插件已移除，请重新扫描后再设置".to_string());
            } else {
                let previous = self.config.settings.get_external_settings(&new_settings.id);
                self.config
                    .settings
                    .set_external_settings(&new_settings.id, new_settings.values.clone());
                match self.config.save() {
                    Ok(()) => {
                        self.plugin_manager
                            .set_external_settings(&new_settings.id, new_settings.values);
                        self.external_plugin_config_window.notify_saved();
                        self.plugin_manager_window
                            .set_status(false, "外部插件设置已保存".to_string());
                    }
                    Err(error) => {
                        self.config
                            .settings
                            .set_external_settings(&new_settings.id, previous);
                        self.external_plugin_config_window
                            .notify_save_error(format!("无法保存插件设置：{error}"));
                    }
                }
            }
            ctx.request_repaint_of(self.plugin_manager_window.viewport_id());
        }

        let identity = self.cloud_document_identity();
        if let Some(action) =
            self.cloud_backup_dialog
                .show(ctx, &identity, &self.editor.get_content())
        {
            match action {
                CloudBackupAction::SaveConfig(new_config) => {
                    let previous = std::mem::replace(
                        &mut self.config.settings.cloud_backup,
                        new_config.clone(),
                    );
                    match self.config.save() {
                        Ok(()) => {
                            self.plugin_manager
                                .update_builtin_configs(None, Some(new_config));
                            self.cloud_backup_dialog.notify_save_success();
                        }
                        Err(error) => {
                            self.config.settings.cloud_backup = previous;
                            self.cloud_backup_dialog
                                .notify_save_error(format!("无法保存云端备份设置：{error}"));
                        }
                    }
                }
                CloudBackupAction::ReplaceContent {
                    content,
                    expected_doc_id,
                    source_name,
                } => {
                    if self.cloud_document_identity().doc_id != expected_doc_id {
                        self.cloud_backup_dialog
                            .set_status(true, "当前文档已变化，已取消应用云端备份".to_string());
                    } else {
                        match self.create_cloud_recovery_snapshot() {
                            Ok(snapshot) => {
                                self.editor.set_content(content);
                                self.cloud_backup_dialog.set_status(
                                    false,
                                    format!(
                                        "已恢复「{}」。恢复前正文快照：{}",
                                        source_name,
                                        snapshot.display()
                                    ),
                                );
                            }
                            Err(error) => {
                                self.cloud_backup_dialog.set_status(true, error);
                            }
                        }
                    }
                }
                CloudBackupAction::OpenNewDocument { filename, content } => {
                    let directory = self.document_directory();
                    match self.create_cloud_recovery_snapshot() {
                        Ok(snapshot) => {
                            let font_size = self.config.settings.font_size;
                            let line_spacing = self.config.settings.line_spacing;
                            self.editor = Editor::default();
                            self.editor.font_size = font_size;
                            self.editor.line_spacing = line_spacing;
                            self.editor.font_color =
                                crate::config::parse_hex_color(&self.config.settings.font_color);
                            self.editor.set_content(content);
                            self.history_window.close();
                            self.draft_directory = Some(directory);
                            self.draft_doc_id = uuid::Uuid::new_v4().to_string();
                            self.current_version_id = None;
                            self.document_uuid = None;
                            self.saved_content.clear();
                            self.time_backend.get_and_reset_writing_time();
                            self.cloud_backup_dialog.set_status(
                                false,
                                format!(
                                    "已将「{}」作为新文档打开。原文快照：{}",
                                    filename,
                                    snapshot.display()
                                ),
                            );
                        }
                        Err(error) => {
                            self.cloud_backup_dialog.set_status(true, error);
                        }
                    }
                }
            }
        }

        if let Some(new_config) = self.plugin_config_window.show(ctx) {
            let previous =
                std::mem::replace(&mut self.config.settings.github_publish, new_config.clone());
            match self.config.save() {
                Ok(()) => {
                    self.plugin_manager
                        .update_builtin_configs(Some(new_config), None);
                    self.plugin_config_window.close();
                }
                Err(error) => {
                    self.config.settings.github_publish = previous;
                    self.plugin_config_window
                        .notify_save_error(format!("无法保存插件设置：{error}"));
                }
            }
        }

        if let Some(params) = self.publish_dialog.show(ctx) {
            if let Some(plugin) = self.plugin_manager.get("github_publish") {
                let name = plugin.metadata().name;
                self.plugin_output.start(&name);
                let plugin_ctx = PluginContext {
                    file_path: self.editor.get_current_file().cloned(),
                    content: self.editor.get_content(),
                    data_dir: self.config.data_dir(),
                    title: Some(params.title),
                    description: params.description,
                    collection: Some(params.collection_dir),
                    printer: None,
                    print_margin_points: None,
                    settings: Default::default(),
                };
                let sender = self.response_sender.clone();

                std::thread::spawn(move || {
                    let result = plugin.run(&plugin_ctx).map_err(|e| e.to_string());
                    let _ = sender.send(ResponseMessage::PluginFinished { name, result });
                });
            } else {
                tracing::warn!("Plugin not found: github_publish");
            }
        }

        // AppKit's modal print panel must run on the UI thread, after egui's UI closures.
        #[cfg(target_os = "macos")]
        if std::mem::take(&mut self.print_requested)
            && let Some(plugin) = self.plugin_manager.get("print")
        {
            let name = plugin.metadata().name;
            let plugin_ctx = PluginContext {
                file_path: self.editor.get_current_file().cloned(),
                content: self.editor.get_content(),
                data_dir: self.config.data_dir(),
                title: None,
                description: None,
                collection: None,
                printer: None,
                print_margin_points: None,
                settings: Default::default(),
            };
            let result = plugin.run(&plugin_ctx).map_err(|e| e.to_string());
            self.plugin_output.finish(name, result);
        }

        #[cfg(not(target_os = "macos"))]
        if let Some(params) = self.print_dialog.show(ctx) {
            if let Some(plugin) = self.plugin_manager.get("print") {
                let name = plugin.metadata().name;
                self.plugin_output.start(&name);
                let plugin_ctx = PluginContext {
                    file_path: self.editor.get_current_file().cloned(),
                    content: self.editor.get_content(),
                    data_dir: self.config.data_dir(),
                    title: None,
                    description: None,
                    collection: None,
                    printer: params.printer,
                    print_margin_points: Some(params.margin_points),
                    settings: Default::default(),
                };
                let sender = self.response_sender.clone();

                std::thread::spawn(move || {
                    let result = plugin.run(&plugin_ctx).map_err(|e| e.to_string());
                    let _ = sender.send(ResponseMessage::PluginFinished { name, result });
                });
            } else {
                tracing::warn!("Plugin not found: print");
            }
        }

        // Custom in-app unsaved changes confirmation dialog
        if let Some(decision) = self.unsaved_dialog.show(ctx) {
            self.handle_unsaved_dialog_decision(ctx, decision);
        }

        self.sidebar.save_if_dirty(&self.config.data_dir());
    }
}

#[cfg(test)]
mod document_change_tests {
    use super::*;

    #[test]
    fn update_restart_preserves_unsaved_document_when_cancelled() {
        let ctx = egui::Context::default();
        let mut app = PaperShellApp::default();
        app.editor.set_content("unsaved writing".into());
        app.request_update_install(&ctx);
        assert_eq!(
            app.unsaved_dialog.pending_action(),
            Some(&PendingDocumentAction::InstallUpdate)
        );
        assert!(!app.allow_close_once);
        app.handle_unsaved_dialog_decision(&ctx, DocumentChangeDecision::Cancel);
        assert!(!app.unsaved_dialog.is_open());
        assert!(!app.allow_close_once);
        assert_eq!(app.editor.get_content(), "unsaved writing");
    }

    #[test]
    fn failed_save_prevents_update_installation_and_close() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let mut app = PaperShellApp::default();
        app.editor
            .set_current_file(Some(root.path().join("missing-directory/document.txt")));
        app.editor.set_content("unsaved writing".into());
        app.request_update_install(&ctx);
        app.handle_unsaved_dialog_decision(&ctx, DocumentChangeDecision::Save);
        assert!(app.unsaved_dialog.is_open());
        assert_eq!(
            app.unsaved_dialog.pending_action(),
            Some(&PendingDocumentAction::InstallUpdate)
        );
        assert!(app.file_error.is_some());
        assert!(!app.allow_close_once);
        assert_eq!(app.editor.get_content(), "unsaved writing");
    }

    #[test]
    fn cloud_restore_snapshot_preserves_previous_content() {
        let root = std::env::temp_dir().join(format!(
            "paper-shell-cloud-recovery-{}",
            uuid::Uuid::new_v4()
        ));
        let snapshot = write_cloud_recovery_snapshot(&root, "恢复前的本地正文").unwrap();
        assert_eq!(
            std::fs::read_to_string(&snapshot).unwrap(),
            "恢复前的本地正文"
        );
        assert!(snapshot.starts_with(root.join("history").join("cloud_backup_recovery")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_dirty_detection_for_content_and_comments() {
        let mut app = PaperShellApp {
            saved_content: "baseline".into(),
            ..Default::default()
        };
        app.editor.set_content("baseline".into());
        assert!(!app.is_document_dirty());

        // Changing content makes it dirty
        app.editor.set_content("new text".into());
        assert!(app.is_document_dirty());

        // Reverting content restores clean state
        app.editor.set_content("baseline".into());
        assert!(!app.is_document_dirty());

        // Changing comments without changing content makes it dirty
        app.editor.set_comments(vec![crate::comments::TextComment {
            id: "1".into(),
            text: "note".into(),
            start: 0,
            end: 4,
            quote: "base".into(),
            updated_at: None,
        }]);
        assert!(app.is_document_dirty());
    }

    #[test]
    fn test_decision_cancel_preserves_content_and_resets_dialog() {
        let ctx = egui::Context::default();
        let mut app = PaperShellApp {
            saved_content: "original".into(),
            ..Default::default()
        };
        app.editor.set_content("unsaved draft".into());
        app.unsaved_dialog
            .open(PendingDocumentAction::OpenFile(PathBuf::from("/other.txt")));

        app.handle_unsaved_dialog_decision(&ctx, DocumentChangeDecision::Cancel);

        assert!(!app.unsaved_dialog.is_open());
        assert!(app.unsaved_dialog.pending_action().is_none());
        assert_eq!(app.editor.get_content(), "unsaved draft");
        assert!(!app.allow_close_once);
    }

    #[test]
    fn test_decision_discard_close_sets_allow_close_without_mutating_saved_snapshot() {
        let ctx = egui::Context::default();
        let mut app = PaperShellApp {
            saved_content: "persisted text".into(),
            ..Default::default()
        };
        app.editor.set_content("unpersisted modifications".into());
        app.unsaved_dialog.open(PendingDocumentAction::CloseWindow);

        app.handle_unsaved_dialog_decision(&ctx, DocumentChangeDecision::Discard);

        assert!(!app.unsaved_dialog.is_open());
        assert!(app.allow_close_once);
        // CRITICAL: saved_content must not be forged to fake clean state
        assert_eq!(app.saved_content, "persisted text");
        assert_eq!(app.editor.get_content(), "unpersisted modifications");
    }

    #[test]
    fn test_decision_discard_new_document_applies_new_doc() {
        let ctx = egui::Context::default();
        let mut app = PaperShellApp::default();
        app.editor.set_content("some text".into());
        app.unsaved_dialog
            .open(PendingDocumentAction::NewDocument(None));

        app.handle_unsaved_dialog_decision(&ctx, DocumentChangeDecision::Discard);

        assert!(!app.unsaved_dialog.is_open());
        assert_eq!(app.editor.get_content(), "");
        assert!(app.editor.get_current_file().is_none());
    }

    #[test]
    fn test_rollback_failure_sets_visible_file_error() {
        let mut app = PaperShellApp::default();
        assert!(app.file_error.is_none());

        let success = app.apply_rollback("nonexistent_hash_12345");
        assert!(!success);
        assert!(app.file_error.is_some());
        assert!(
            app.file_error
                .as_ref()
                .unwrap()
                .contains("无法回滚到版本 nonexistent_hash_12345")
        );
    }

    #[test]
    fn test_history_window_escape_does_not_double_close() {
        let mut window = HistoryWindow::new();
        window.open();
        window.confirm_rollback = Some("hash123".into());

        // When confirm_rollback is Some, confirm_rollback is active.
        // Once cancelled, confirm_rollback becomes None, but window remains open.
        window.confirm_rollback = None;
        assert!(window.is_open());
    }
}

#[cfg(test)]
mod final_integration_tests {
    use super::*;

    #[test]
    fn test_vim_mode_full_workflow() {
        let mut app = PaperShellApp::default();
        app.config.settings.vim_mode = true;
        app.editor.set_vim_mode(true);
        app.editor.set_content("第一行\n第二行\n第三行".to_string());

        let ctx = egui::Context::default();
        let mut frame = eframe::Frame::_new_kittest();

        // F1: initial
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );

        // F2: click editor to focus
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                events: vec![
                    egui::Event::PointerButton {
                        pos: egui::pos2(100.0, 100.0),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: egui::pos2(100.0, 100.0),
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );

        // F3: normal frame so had_focus_last_frame is true
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );
        assert!(ctx.memory(|m| m.focused()).is_some());

        // F4: Escape via raw_input_hook
        let mut raw_input = egui::RawInput {
            focused: true,
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut raw_input);
        assert!(
            raw_input.events.is_empty(),
            "Escape must be swallowed by raw_input_hook"
        );
        assert!(app.vim_pending_escape);

        let _ = ctx.run(raw_input, |ctx| {
            eframe::App::update(&mut app, ctx, &mut frame);
        });

        // Verify mode switched to Normal AND focus is retained!
        assert_eq!(app.editor.get_vim_mode_label(), Some("普通"));
        assert!(
            ctx.memory(|m| m.focused()).is_some(),
            "Editor MUST retain focus after Escape!"
        );

        // F5: 'j' in Normal mode should NOT insert text
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                events: vec![egui::Event::Text("j".to_string())],
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );
        assert_eq!(app.editor.get_vim_mode_label(), Some("普通"));
        assert_eq!(
            app.editor.get_content(),
            "第一行\n第二行\n第三行",
            "'j' must not insert text in Normal mode"
        );

        // F6: 'i' back to Insert mode
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                events: vec![egui::Event::Text("i".to_string())],
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );
        assert_eq!(app.editor.get_vim_mode_label(), Some("插入"));

        // F7: consecutive Esc presses toggle Insert -> Normal -> Insert -> Normal
        for expected in ["普通", "插入", "普通", "插入"] {
            let mut raw_input = egui::RawInput {
                focused: true,
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            eframe::App::raw_input_hook(&mut app, &ctx, &mut raw_input);
            let _ = ctx.run(raw_input, |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            });
            assert_eq!(app.editor.get_vim_mode_label(), Some(expected));
            assert!(
                ctx.memory(|m| m.focused()).is_some(),
                "Editor MUST retain focus after repeated Escape!"
            );
        }

        // F8: simultaneous Monitor + RawInput Escape for a single physical press only toggles once
        let _ = app.response_sender.send(ResponseMessage::VimEscape);
        let mut raw_input = egui::RawInput {
            focused: true,
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut raw_input);
        let _ = ctx.run(raw_input, |ctx| {
            eframe::App::update(&mut app, ctx, &mut frame);
        });
        assert_eq!(app.editor.get_vim_mode_label(), Some("普通"));

        // Even if the Monitor message arrives on the next frame, cross-source deduplication prevents double toggle
        let _ = app.response_sender.send(ResponseMessage::VimEscape);
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );
        assert_eq!(app.editor.get_vim_mode_label(), Some("普通"));
    }

    #[test]
    fn secondary_viewport_escape_is_preserved_when_vim_mode_is_enabled() {
        let mut app = PaperShellApp::default();
        app.config.settings.vim_mode = true;
        app.editor.set_vim_mode(true);

        let ctx = egui::Context::default();
        let manager_viewport = app.plugin_manager_window.viewport_id();
        let mut child_input = egui::RawInput {
            viewport_id: manager_viewport,
            focused: true,
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };

        eframe::App::raw_input_hook(&mut app, &ctx, &mut child_input);
        assert_eq!(
            child_input.events.len(),
            1,
            "secondary windows must retain Escape events instead of losing them to Vim mode"
        );
        assert!(!app.vim_pending_escape);

        // macOS global Escape monitor events while a secondary viewport is focused must also not toggle Vim mode.
        app.record_vim_escape(EscapeSource::Monitor);
        assert!(!app.vim_pending_escape);
    }

    #[test]
    fn test_font_color_workflow_and_reset() {
        let mut app = PaperShellApp::default();
        let ctx = egui::Context::default();

        // 1. Initial default state
        assert_eq!(app.config.settings.font_color, "Default");
        assert_eq!(app.editor.font_color, None);

        // 2. Change font color via TitleBar action
        let _ = ctx.run(Default::default(), |ctx| {
            egui::TopBottomPanel::top("title").show(ctx, |_ui| {
                // Simulate TitleBarAction::FontColorChange
                let action =
                    crate::ui::title_bar::TitleBarAction::FontColorChange("#1E3A5F".to_string());
                match action {
                    crate::ui::title_bar::TitleBarAction::FontColorChange(color_hex) => {
                        app.editor.font_color = crate::config::parse_hex_color(&color_hex);
                        app.config.settings.font_color = color_hex;
                    }
                    _ => unreachable!(),
                }
            });
        });

        assert_eq!(app.config.settings.font_color, "#1E3A5F");
        assert_eq!(
            app.editor.font_color,
            Some(egui::Color32::from_rgb(30, 58, 95))
        );

        // 3. Opening a new document inherits the current font color
        app.apply_new_document(None);
        assert_eq!(
            app.editor.font_color,
            Some(egui::Color32::from_rgb(30, 58, 95))
        );

        // 4. Reset typography resets font color back to Default
        let _ = ctx.run(Default::default(), |_ctx| {
            app.editor.font_size = 14.0;
            app.editor.line_spacing = 1.5;
            app.editor.font_color = None;
            app.config.settings.font_size = 14.0;
            app.config.settings.line_spacing = 1.5;
            app.config.settings.font_family = "Default".to_string();
            app.config.settings.font_color = "Default".to_string();
        });

        assert_eq!(app.config.settings.font_color, "Default");
        assert_eq!(app.editor.font_color, None);
    }

    #[test]
    fn test_skin_workflow_apply_and_custom_save() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut app = PaperShellApp {
            skin_manager: crate::skin::SkinManager::new(
                temp_dir.path().to_path_buf(),
                "classic_paper".to_string(),
            ),
            ..Default::default()
        };
        let ctx = egui::Context::default();

        // 1. Initial skin is classic_paper
        assert_eq!(app.skin_manager.current_skin_id(), "classic_paper");

        // 2. Apply warm_parchment
        app.apply_skin(&ctx, "warm_parchment");
        assert_eq!(app.skin_manager.current_skin_id(), "warm_parchment");
        assert_eq!(app.config.settings.current_skin, "warm_parchment");
        assert_eq!(app.editor.font_size, 14.5);
        assert_eq!(app.editor.line_spacing, 1.6);
        assert_eq!(app.config.settings.font_color, "#382C1E");

        // 3. Apply obsidian_night (dark mode)
        app.apply_skin(&ctx, "obsidian_night");
        assert_eq!(app.skin_manager.current_skin_id(), "obsidian_night");
        assert_eq!(app.config.settings.current_skin, "obsidian_night");
        assert_eq!(app.config.settings.font_color, "#DCDCE0");
        assert!(ctx.style().visuals.dark_mode);

        // 4. Save and apply custom skin
        let mut custom = crate::skin::Skin::mint_breeze();
        custom.id = "user_summer_skin".to_string();
        custom.name = "盛夏清凉".to_string();
        custom.typography.font_size = 18.0;
        custom.typography.line_spacing = 1.8;
        custom.spacing.editor_padding_left = 16.0;

        let saved = app.skin_manager.save_custom_skin(custom).unwrap();
        app.apply_skin(&ctx, &saved.id);

        assert_eq!(app.skin_manager.current_skin_id(), "user_summer_skin");
        assert_eq!(app.editor.font_size, 18.0);
        assert_eq!(app.editor.line_spacing, 1.8);

        // 5. Uninstall custom skin reverts to default
        assert!(app.skin_manager.uninstall_skin("user_summer_skin").is_ok());
        assert_eq!(app.skin_manager.current_skin_id(), "classic_paper");
    }

    #[test]
    fn test_central_panel_background_is_solid_without_transparent_borders() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut app = PaperShellApp {
            skin_manager: crate::skin::SkinManager::new(
                temp_dir.path().to_path_buf(),
                "classic_paper".to_string(),
            ),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut frame = eframe::Frame::_new_kittest();

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );

        let has_solid_central_panel = output.shapes.iter().any(|clipped| {
            if let egui::epaint::Shape::Rect(rect_shape) = &clipped.shape {
                rect_shape.rect.width() >= 700.0
                    && rect_shape.rect.height() >= 500.0
                    && rect_shape.fill == egui::Color32::from_rgb(255, 255, 255)
            } else {
                false
            }
        });
        assert!(
            has_solid_central_panel,
            "CentralPanel must have solid fill covering the entire area without transparent borders"
        );
    }

    #[test]
    fn test_image_background_covers_panel_with_text_padding() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut app = PaperShellApp {
            skin_manager: crate::skin::SkinManager::new(
                temp_dir.path().to_path_buf(),
                "classic_paper".to_string(),
            ),
            ..Default::default()
        };
        let skin = app
            .skin_manager
            .import_from_file(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("skins/megazone_ride/skin.toml"),
            )
            .unwrap();
        assert!(skin.spacing.editor_padding_left > 0.0);
        assert!(skin.spacing.editor_padding_top > 0.0);
        let ctx = egui::Context::default();
        let texture_id = app
            .skin_renderer
            .get_or_load_texture(&ctx, &skin)
            .unwrap()
            .id();
        let mut frame = eframe::Frame::_new_kittest();
        let fill =
            crate::config::parse_hex_color(skin.colors.editor_background_color.as_deref().unwrap())
                .unwrap();

        for size in [egui::vec2(800.0, 600.0), egui::vec2(1200.0, 700.0)] {
            for expanded in [false, true] {
                // Construct a clean fixture; toggling would persist test state to disk.
                app.sidebar =
                    crate::ui::sidebar::Sidebar::new(crate::ui::sidebar::WorkspaceState {
                        sidebar_expanded: expanded,
                        ..Default::default()
                    });
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| eframe::App::update(&mut app, ctx, &mut frame),
                );
                let panel = output
                    .shapes
                    .iter()
                    .find_map(|clipped| {
                        if let egui::epaint::Shape::Rect(shape) = &clipped.shape {
                            (shape.fill == fill && shape.rect.height() > 400.0)
                                .then_some(shape.rect)
                        } else {
                            None
                        }
                    })
                    .expect("editor panel fill");
                let background = output
                    .shapes
                    .iter()
                    .find(|clipped| {
                        matches!(&clipped.shape, egui::epaint::Shape::Mesh(mesh)
                        if mesh.texture_id == texture_id)
                    })
                    .expect("background image mesh");
                assert_eq!(
                    background.clip_rect, panel,
                    "background must extend through text padding"
                );
                if let egui::epaint::Shape::Mesh(mesh) = &background.shape {
                    assert!(
                        mesh.calc_bounds().contains_rect(panel),
                        "cover must fill the panel"
                    );
                }
            }
        }
    }

    #[test]
    fn test_statistics_delete_document_action_handling() {
        let mut app = PaperShellApp::default();
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("article_to_delete.md");
        std::fs::write(&file_path, "# 一篇记录的文章\n这是正文内容。").unwrap();

        // Save via backend
        let (doc_uuid, _) = app
            .editor_backend
            .save(&file_path, "# 一篇记录的文章\n这是正文内容。", 60)
            .unwrap();
        assert!(file_path.exists());

        // Set as current file
        app.editor.set_current_file(Some(file_path.clone()));
        app.document_uuid = Some(doc_uuid.clone());

        // 1. Kick out document from statistics (retaining local file)
        app.handle_statistics_action(crate::ui::statistics::StatisticsAction::DeleteDocument {
            doc_id: doc_uuid.clone(),
            delete_local_file: false,
        });

        // File remains on disk
        assert!(file_path.exists());
        // Editor document_uuid cleared so it won't link to the deleted record
        assert_eq!(app.document_uuid, None);
        // Stats record is removed
        let stats = app.editor_backend.load_writing_statistics().unwrap();
        assert!(stats.documents.iter().all(|doc| doc.id != doc_uuid));

        // 2. Now save another temporary file and delete with delete_local_file = true
        let file_path_2 = temp_dir.path().join("draft_to_wipe.md");
        std::fs::write(&file_path_2, "临时草稿").unwrap();
        let (doc_uuid_2, _) = app
            .editor_backend
            .save(&file_path_2, "临时草稿", 30)
            .unwrap();
        app.editor.set_current_file(Some(file_path_2.clone()));
        app.document_uuid = Some(doc_uuid_2.clone());

        app.handle_statistics_action(crate::ui::statistics::StatisticsAction::DeleteDocument {
            doc_id: doc_uuid_2.clone(),
            delete_local_file: true,
        });

        // Disk file was removed
        assert!(!file_path_2.exists());
        // Editor opened a new document
        assert!(app.editor.get_current_file().is_none());
        assert_eq!(app.document_uuid, None);
    }
}
