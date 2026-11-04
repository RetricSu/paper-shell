use crate::backend::editor_backend::EditorBackend;
use crate::backend::time_backend::TimeBackend;
use crate::file::FileData;
use crate::messages::ResponseMessage;
use crate::plugin::builtin::cloud_backup::DocumentIdentity;
use crate::plugin::{PluginContext, PluginManager};
use crate::style::configure_style;
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

pub struct PaperShellApp {
    editor: Editor,
    pub response_sender: Sender<ResponseMessage>,
    response_receiver: Receiver<ResponseMessage>,

    history_window: HistoryWindow,

    current_font: String,
    available_fonts: Vec<String>,

    last_focus_state: bool,
    config: crate::config::Config,

    editor_backend: Arc<EditorBackend>,
    sidebar: crate::ui::sidebar::Sidebar,
    draft_directory: Option<PathBuf>,
    draft_doc_id: String,
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
        let workspace_state = crate::ui::sidebar::WorkspaceState::load(&config.data_dir());
        let sidebar = crate::ui::sidebar::Sidebar::new(workspace_state);

        Self {
            editor,
            editor_backend: Arc::new(EditorBackend::default()),
            sidebar,
            draft_directory: None,
            draft_doc_id: uuid::Uuid::new_v4().to_string(),
            saved_content: String::new(),
            saved_comments: Vec::new(),
            file_error: None,
            time_backend: TimeBackend::default(),
            response_receiver: receiver,
            response_sender: sender,
            history_window: HistoryWindow::new(),
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
            #[cfg(target_os = "macos")]
            print_requested: false,
        }
    }
}

impl PaperShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        configure_style(&cc.egui_ctx);

        let mut app = Self::default();
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

            std::thread::spawn(move || {
                let result = backend.load_history(&path).map_err(|e| e.to_string());
                let _ = sender.send(ResponseMessage::HistoryLoaded(result));
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
        self.editor.set_vim_mode(self.config.settings.vim_mode);
        self.editor.set_content(String::new());
        self.draft_doc_id = uuid::Uuid::new_v4().to_string();
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
        match self
            .editor_backend
            .save(&path, &content, self.time_backend.get_writing_time())
        {
            Ok((uuid, total_time)) => {
                // The body/history save has consumed this interval even if comment
                // persistence fails and needs to be retried.
                self.time_backend.get_and_reset_writing_time();
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
                true
            }
            Err(error) => {
                self.file_error = Some(format!("正文已写入，但无法保存版本记录: {}", error));
                false
            }
        }
    }

    fn apply_load_file_data(&mut self, data: FileData) {
        self.draft_directory = None;
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
                ResponseMessage::HistoryLoaded(result) => match result {
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
                },
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
                ResponseMessage::VimEscape => {
                    if self.config.settings.vim_mode {
                        self.vim_pending_escape = true;
                    }
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
        }
    }

    fn apply_rollback(&mut self, hash: &str) -> bool {
        match self.editor_backend.restore_version(hash) {
            Ok(content) => {
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

    fn handle_plugin_manager_action(&mut self, action: PluginManagerAction) {
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
                "cloud_backup" => self
                    .cloud_backup_dialog
                    .open(&self.config.settings.cloud_backup),
                "github_publish" => self
                    .plugin_config_window
                    .open(&self.config.settings.github_publish, false),
                _ => {
                    let Some(plugin) = self.plugin_manager.find_any(&id) else {
                        self.plugin_manager_window
                            .set_status(true, format!("未找到插件“{id}”，请重新扫描"));
                        return;
                    };
                    let metadata = plugin.metadata();
                    let definitions = plugin.settings_definitions();
                    let saved = self.plugin_manager.external_settings_for(&id);
                    self.external_plugin_config_window
                        .open(&metadata, definitions, &saved);
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
            PendingDocumentAction::Rollback(hash) => {
                let _ = self.apply_rollback(&hash);
            }
        }
    }
}

impl eframe::App for PaperShellApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.config.settings.vim_mode && !self.editor.is_search_dialog_open() {
            let had_escape = raw_input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Escape,
                        pressed: true,
                        ..
                    }
                )
            });
            if had_escape {
                raw_input.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        }
                    )
                });
                self.vim_pending_escape = true;
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
        // Title Bar
        let title_bar = egui::TopBottomPanel::top("title_bar_panel")
            // This panel has one explicit border painted below.
            .show_separator_line(false);
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
                    font_size: self.editor.font_size,
                    line_spacing: self.editor.line_spacing,
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
                    crate::ui::title_bar::TitleBarAction::Open => {
                        self.try_open_file_from_selector()
                    }
                    crate::ui::title_bar::TitleBarAction::OpenFile(path) => self.open_file(path),
                    crate::ui::title_bar::TitleBarAction::Format => self.editor.format(),
                    crate::ui::title_bar::TitleBarAction::History => self.try_load_history(),
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
                    crate::ui::title_bar::TitleBarAction::ResetTypography => {
                        self.editor.font_size = 14.0;
                        self.editor.line_spacing = 1.5;
                        self.config.settings.font_size = 14.0;
                        self.config.settings.line_spacing = 1.5;
                        self.config.settings.font_family = "Default".to_string();
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
                            self.cloud_backup_dialog
                                .open(&self.config.settings.cloud_backup);
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
                    crate::ui::title_bar::TitleBarAction::OpenPluginsFolder => {
                        self.open_plugins_folder_with_feedback();
                    }
                    crate::ui::title_bar::TitleBarAction::ManagePlugins => {
                        self.plugin_manager_window.open();
                    }
                    crate::ui::title_bar::TitleBarAction::ConfigurePlugin(id) => {
                        if id == "cloud_backup" {
                            self.cloud_backup_dialog
                                .open(&self.config.settings.cloud_backup);
                        } else if id == "github_publish" {
                            self.plugin_config_window
                                .open(&self.config.settings.github_publish, false);
                        }
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

        // Main Content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin {
                    left: 2,
                    right: 6,
                    top: 6,
                    bottom: 6,
                }),
            )
            .show(ctx, |ui| {
                if modal_open {
                    ui.disable();
                }
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

        // History Window
        self.history_window.show(ctx);
        if let Some(action) = self.history_window.take_pending_action() {
            self.handle_history_action(action);
        }

        // Plugin output and management windows
        self.plugin_output.show(ctx);
        let all_plugins = self.plugin_manager.all_metadata();
        if let Some(action) =
            self.plugin_manager_window
                .show(ctx, &all_plugins, self.plugin_manager.load_errors())
        {
            self.handle_plugin_manager_action(action);
        }
        if let Some(new_settings) = self.external_plugin_config_window.show(ctx) {
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
                            self.editor.set_content(content);
                            self.draft_directory = Some(directory);
                            self.draft_doc_id = uuid::Uuid::new_v4().to_string();
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
    }
}
