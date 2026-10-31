use crate::backend::editor_backend::EditorBackend;
use crate::backend::time_backend::TimeBackend;
use crate::file::FileData;
use crate::messages::ResponseMessage;
use crate::plugin::{PluginContext, PluginManager};
use crate::style::configure_style;
use crate::ui::editor::Editor;
use crate::ui::history::{HistoryAction, HistoryWindow};
use crate::ui::plugins::{
    GithubPublishConfigWindow, PluginOutputWindow, PrintDialog, PublishDialog,
};

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

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
    saved_content: String,
    file_error: Option<String>,
    time_backend: TimeBackend,

    plugin_manager: PluginManager,
    plugin_metadata: Vec<crate::plugin::PluginMetadata>,
    plugin_output: PluginOutputWindow,
    plugin_config_window: GithubPublishConfigWindow,
    publish_dialog: PublishDialog,
    print_dialog: PrintDialog,
}

impl Default for PaperShellApp {
    fn default() -> Self {
        // Initialize tracing subscriber for console output
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_thread_ids(true)
            .init();

        let (sender, receiver) = channel();
        let editor = Editor::default();
        let available_fonts = crate::ui::font::enumerate_chinese_fonts();
        let config = crate::config::Config::default();

        let plugins_dir = config.data_dir().join("plugins");
        let plugin_manager =
            PluginManager::new(plugins_dir, config.settings.github_publish.clone());
        let plugin_metadata = plugin_manager.metadata();

        Self {
            editor,
            editor_backend: Arc::new(EditorBackend::default()),
            sidebar: crate::ui::sidebar::Sidebar::default(),
            draft_directory: None,
            saved_content: String::new(),
            file_error: None,
            time_backend: TimeBackend::default(),
            response_receiver: receiver,
            response_sender: sender,
            history_window: HistoryWindow::new(),
            available_fonts,
            current_font: "Default".to_string(),
            last_focus_state: false,
            config,
            plugin_manager,
            plugin_metadata,
            plugin_output: PluginOutputWindow::new(),
            plugin_config_window: GithubPublishConfigWindow::new(),
            publish_dialog: PublishDialog::new(),
            print_dialog: PrintDialog::new(),
        }
    }
}

impl PaperShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        configure_style(&cc.egui_ctx);

        let mut app = Self::default();
        if let Some(path) = initial_file {
            app.open_file(path);
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

        Ok(FileData {
            uuid,
            path: path.to_path_buf(),
            total_time,
            content,
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

    fn open_file(&mut self, path: PathBuf) {
        if self.editor.get_current_file() == Some(&path) || !self.confirm_document_switch() {
            return;
        }
        match self.load_file_data(&path) {
            Ok(data) => self.apply_load_file_data(data),
            Err(e) => self.file_error = Some(e),
        }
    }

    fn confirm_document_switch(&mut self) -> bool {
        if self.editor.get_content() == self.saved_content {
            return true;
        }
        match rfd::MessageDialog::new()
            .set_title("未保存的文章")
            .set_description("是否先保存当前文章？选择“否”将放弃未保存的修改。")
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show()
        {
            rfd::MessageDialogResult::Yes => self.save_file(),
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
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
                self.sidebar.refresh();
            }
            SidebarAction::NewDocument(directory) => {
                if self.confirm_document_switch() {
                    self.draft_directory =
                        Some(directory.unwrap_or_else(|| self.document_directory()));
                    self.editor = Editor::default();
                    self.editor.set_content(String::new());
                    self.saved_content.clear();
                    self.time_backend.get_and_reset_writing_time();
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
            Ok((_uuid, total_time)) => {
                self.time_backend.get_and_reset_writing_time();
                self.draft_directory = None;
                self.editor.set_current_file(Some(path.clone()));
                self.editor.set_current_file_total_time(total_time);
                self.saved_content = content;
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
        self.time_backend.get_and_reset_writing_time();
        self.config.add_recent_file(data.path.clone());
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
        if let Ok(response) = self.response_receiver.try_recv() {
            match response {
                ResponseMessage::HistoryLoaded(result) => match result {
                    Ok(entries) => {
                        if let Err(e) = self
                            .history_window
                            .set_history(entries, &self.editor_backend)
                        {
                            tracing::info!("Failed to set history: {}", e);
                        }
                    }
                    Err(e) => tracing::error!("Failed to load history: {}", e),
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
            }
        }
    }

    fn handle_history_action(&mut self, action: HistoryAction) {
        match action {
            HistoryAction::RollbackToVersion(hash) => {
                match self.editor_backend.restore_version(&hash) {
                    Ok(content) => {
                        self.editor.set_content(content);
                        tracing::info!("Rolled back to version: {}", hash);
                    }
                    Err(e) => {
                        tracing::error!("Failed to rollback to version {}: {}", hash, e);
                    }
                }
            }
        }
    }

    /// Runs an installed plugin by id on a background thread, snapshotting the
    /// current editing context so the plugin stays decoupled from live state.
    fn run_plugin(&mut self, id: String) {
        let Some(plugin) = self.plugin_manager.get(&id) else {
            tracing::warn!("Plugin not found: {}", id);
            return;
        };

        let name = plugin.metadata().name;
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
        };
        let sender = self.response_sender.clone();

        std::thread::spawn(move || {
            let result = plugin.run(&ctx).map_err(|e| e.to_string());
            let _ = sender.send(ResponseMessage::PluginFinished { name, result });
        });
    }

    /// Ensures the plugins directory exists and opens it in the system file
    /// manager so users can install plugins by dropping folders into it.
    fn open_plugins_folder(&self) {
        let dir = self.config.data_dir().join("plugins");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::error!("Failed to create plugins dir {:?}: {}", dir, e);
            return;
        }

        #[cfg(target_os = "macos")]
        let opener = "open";
        #[cfg(target_os = "windows")]
        let opener = "explorer";
        #[cfg(all(unix, not(target_os = "macos")))]
        let opener = "xdg-open";

        if let Err(e) = std::process::Command::new(opener).arg(&dir).spawn() {
            tracing::error!("Failed to open plugins dir: {}", e);
        }
    }
}

impl eframe::App for PaperShellApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_response_messages();
        self.update_time_backend_if_focus_changed();
        if ctx.input(|input| input.viewport().close_requested()) && !self.confirm_document_switch()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        let shortcut_action = crate::ui::title_bar::TitleBar::shortcut(ctx);
        let (total_words, cursor_words) = self.editor.get_stats();
        // Title Bar
        egui::TopBottomPanel::top("title_bar_panel").show(ctx, |ui| {
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
                    recent_files: &self.config.settings.recent_files,
                    plugins: &self.plugin_metadata,
                    sidebar_expanded: self.sidebar.is_expanded(),
                },
            )
            .or(shortcut_action)
            {
                match action {
                    crate::ui::title_bar::TitleBarAction::NewWindow => self.spawn_new_window(),
                    crate::ui::title_bar::TitleBarAction::ToggleSidebar => self.sidebar.toggle(),
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
                    crate::ui::title_bar::TitleBarAction::FontChange(font_name) => {
                        let new_fonts = if font_name == "Default" {
                            crate::ui::font::setup_fonts()
                        } else {
                            crate::ui::font::apply_font(&font_name)
                        };
                        ctx.set_fonts(new_fonts);
                        self.current_font = font_name.clone();
                        tracing::info!("Font changed to: {}", font_name);
                    }
                    crate::ui::title_bar::TitleBarAction::RunPlugin(id) => {
                        if id == "github_publish" {
                            if self.config.settings.github_publish.repo.trim().is_empty() {
                                self.plugin_config_window
                                    .open(&self.config.settings.github_publish, true);
                            } else {
                                self.publish_dialog
                                    .open(&self.config.settings.github_publish);
                            }
                        } else if id == "print" {
                            let document_name = self
                                .editor
                                .get_current_file()
                                .and_then(|path| path.file_name())
                                .and_then(|name| name.to_str())
                                .unwrap_or("未命名文档")
                                .to_string();
                            self.print_dialog
                                .open(document_name, self.editor.get_content());
                        } else {
                            self.run_plugin(id);
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::OpenPluginsFolder => {
                        self.open_plugins_folder();
                    }
                    crate::ui::title_bar::TitleBarAction::ConfigurePlugin(id) => {
                        if id == "github_publish" {
                            self.plugin_config_window
                                .open(&self.config.settings.github_publish, false);
                        }
                    }
                }
            }
        });

        if let Some(action) = self.sidebar.show(
            ctx,
            &self.config.data_dir(),
            &self.config.settings.workspace_folders,
            self.editor.get_current_file().map(|path| path.as_path()),
        ) {
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.vertical_centered(|ui| {
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

        // Plugin output window
        self.plugin_output.show(ctx);

        if let Some(new_config) = self.plugin_config_window.show(ctx) {
            self.config.settings.github_publish = new_config.clone();
            if let Err(error) = self.config.save() {
                self.file_error = Some(format!("无法保存插件设置: {}", error));
            }
            let plugins_dir = self.config.data_dir().join("plugins");
            self.plugin_manager = crate::plugin::PluginManager::new(plugins_dir, new_config);
            self.plugin_metadata = self.plugin_manager.metadata();
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
    }
}
