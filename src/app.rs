use crate::backend::ai_backend::AiBackend;
use crate::backend::editor_backend::EditorBackend;
use crate::backend::sidebar_backend::{Mark, SidebarBackend};
use crate::backend::time_backend::TimeBackend;
use crate::file::FileData;
use crate::messages::ResponseMessage;
use crate::plugin::{PluginContext, PluginManager};
use crate::style::configure_style;
use crate::ui::ai_panel::AiPanelAction;
use crate::ui::editor::Editor;
use crate::ui::history::{HistoryAction, HistoryWindow};
use crate::ui::plugins::{
    GithubPublishConfigWindow, PluginOutputWindow, PrintDialog, PublishDialog,
};
use crate::ui::settings::SettingsWindow;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

type LoadFileResult = (FileData, HashMap<usize, Mark>);

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
    sidebar_backend: Arc<SidebarBackend>,
    time_backend: TimeBackend,
    ai_backend: Arc<AiBackend>,

    plugin_manager: PluginManager,
    plugin_metadata: Vec<crate::plugin::PluginMetadata>,
    plugin_output: PluginOutputWindow,
    plugin_config_window: GithubPublishConfigWindow,
    publish_dialog: PublishDialog,
    print_dialog: PrintDialog,
    settings_window: SettingsWindow,
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
        let sidebar_backend = Arc::new(SidebarBackend::new().unwrap_or_else(|e| {
            tracing::error!("Failed to initialize SidebarBackend: {}", e);
            panic!("Cannot continue without SidebarBackend");
        }));
        let available_fonts = crate::ui::font::enumerate_chinese_fonts();
        let config = crate::config::Config::default();
        let ai_backend = Arc::new(AiBackend::from_config(&config.settings.ai_panel));

        let plugins_dir = config.data_dir().join("plugins");
        let plugin_manager =
            PluginManager::new(plugins_dir, config.settings.github_publish.clone());
        let plugin_metadata = plugin_manager.metadata();

        Self {
            editor,
            editor_backend: Arc::new(EditorBackend::default()),
            sidebar_backend,
            time_backend: TimeBackend::default(),
            ai_backend,
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
            settings_window: SettingsWindow::new(),
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
    fn load_file_data(&self, path: &PathBuf) -> Result<LoadFileResult, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e: std::io::Error| format!("Failed to read file {:?}: {}", path, e))?;

        let (uuid, total_time) = self
            .editor_backend
            .get_file_metadata(path, &content)
            .map_err(|e| format!("Failed to get metadata: {}", e))?;

        let marks = self
            .sidebar_backend
            .load_marks(&uuid)
            .map_err(|e| format!("Failed to load marks: {}", e))?;

        Ok((
            FileData {
                uuid,
                path: path.to_path_buf(),
                total_time,
                content,
            },
            marks,
        ))
    }

    // this is mostly the same process with load_file_data but in a thread with messaging
    fn try_load_file_data(&mut self, path: PathBuf) {
        let backend = Arc::clone(&self.editor_backend);
        let sidebar_backend = Arc::clone(&self.sidebar_backend);
        let sender = self.response_sender.clone();

        std::thread::spawn(move || match std::fs::read_to_string(&path) {
            Ok(content) => match backend.get_file_metadata(&path, &content) {
                Ok((uuid, total_time)) => {
                    let _ = sender.send(ResponseMessage::FileLoaded(Ok(FileData {
                        path,
                        content,
                        uuid: uuid.clone(),
                        total_time,
                    })));

                    let marks_result = sidebar_backend.load_marks(&uuid).map_err(|e| e.to_string());
                    let _ = sender.send(ResponseMessage::MarksLoaded(marks_result));
                }
                Err(e) => {
                    let _ = sender.send(ResponseMessage::FileLoaded(Err(format!(
                        "Failed to get metadata: {}",
                        e
                    ))));
                }
            },
            Err(e) => {
                let _ = sender.send(ResponseMessage::FileLoaded(Err(format!(
                    "Failed to read file {:?}: {}",
                    path, e
                ))));
            }
        });
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
        match self.load_file_data(&path) {
            Ok((file_data, marks)) => {
                self.apply_load_file_data(file_data, Some(marks));
            }
            Err(e) => {
                tracing::error!("{}", e);
            }
        }
    }

    fn try_open_file_from_selector(&self) {
        let backend = Arc::clone(&self.editor_backend);
        let data_dir = backend.data_dir().to_path_buf();

        // Keep a reference to the sender to use in the outer scope
        let sender = self.response_sender.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .set_directory(&data_dir)
                .add_filter("Text", &["txt"])
                .pick_file()
            {
                let _ = sender.send(ResponseMessage::OpenFile(path));
            }
        });
    }

    fn try_save_marks_if_changed(&mut self) {
        // Check if marks have changed and save in background if needed
        if self.editor.marks_changed()
            && let Some(uuid) = self.editor.get_sidebar_uuid()
        {
            let marks = self.editor.get_marks().clone();
            let uuid = uuid.clone();
            let sidebar_backend = Arc::clone(&self.sidebar_backend);

            // Reset the changed flag immediately to avoid duplicate saves
            self.editor.reset_marks_changed();

            std::thread::spawn(move || {
                if let Err(e) = sidebar_backend.save_marks(&uuid, &marks) {
                    tracing::error!("Failed to save marks in background: {}", e);
                }
            });
        }
    }

    fn save_file(&mut self) {
        let current_file = self.editor.get_current_file().cloned();
        let content = self.editor.get_content();
        if content.trim().is_empty() {
            return;
        }
        let time_spent = self.time_backend.get_and_reset_writing_time();

        if let Some(path) = current_file {
            // First write the actual file content
            if let Err(e) = std::fs::write(&path, &content) {
                tracing::error!("Failed to write file: {}", e);
                return;
            }

            // Then track with backend (CAS + history)
            let result = self
                .editor_backend
                .save(&path, &content, time_spent)
                .map_err(|e| e.to_string());
            if let Ok((uuid, total_time)) = result.as_ref() {
                self.apply_save_file(uuid.clone(), *total_time);
            } else {
                tracing::error!("Failed to save file: {}", result.err().unwrap());
            }
        } else {
            // Show save dialog for new file
            let data_dir = self.editor_backend.data_dir().to_path_buf();
            if let Some(path) = rfd::FileDialog::new()
                .set_directory(&data_dir)
                .add_filter("Text", &["txt"])
                .save_file()
            {
                // First write the actual file content
                if let Err(e) = std::fs::write(&path, &content) {
                    tracing::error!("Failed to write file: {}", e);
                    return;
                }

                // Then track with backend (CAS + history)
                let result = self
                    .editor_backend
                    .save(&path, &content, time_spent)
                    .map_err(|e| e.to_string());

                // Add to recent files on successful save
                if let Ok((uuid, total_time)) = result.as_ref() {
                    self.apply_save_file(uuid.clone(), *total_time);
                    self.apply_load_file_data(
                        FileData {
                            uuid: uuid.clone(),
                            path,
                            total_time: *total_time,
                            content,
                        },
                        None,
                    );
                } else {
                    tracing::error!("Failed to save file: {}", result.err().unwrap());
                }
            }
        }
    }

    fn try_save_file(&self) {
        let current_file = self.editor.get_current_file().cloned();
        let content = self.editor.get_content();
        if content.trim().is_empty() {
            return;
        }

        let backend = Arc::clone(&self.editor_backend);
        let sender = self.response_sender.clone();
        let time_spent = self.time_backend.get_and_reset_writing_time();

        if let Some(path) = current_file {
            // Save to existing file in background thread
            std::thread::spawn(move || {
                // First write the actual file content
                if let Err(e) = std::fs::write(&path, &content) {
                    let _ = sender.send(ResponseMessage::FileSaved(Err(format!(
                        "Failed to write file: {}",
                        e
                    ))));
                    return;
                }

                // Then track with backend (CAS + history)
                let result = backend
                    .save(&path, &content, time_spent)
                    .map_err(|e| e.to_string());
                let _ = sender.send(ResponseMessage::FileSaved(result));
            });
        } else {
            // Show save dialog for new file
            let data_dir = backend.data_dir().to_path_buf();
            std::thread::spawn(move || {
                if let Some(path) = rfd::FileDialog::new()
                    .set_directory(&data_dir)
                    .add_filter("Text", &["txt"])
                    .save_file()
                {
                    // First write the actual file content
                    if let Err(e) = std::fs::write(&path, &content) {
                        let _ = sender.send(ResponseMessage::FileSaved(Err(format!(
                            "Failed to write file: {}",
                            e
                        ))));
                        return;
                    }

                    // Then track with backend (CAS + history)
                    let result = backend
                        .save(&path, &content, time_spent)
                        .map_err(|e| e.to_string());

                    // Add to recent files on successful save
                    if result.is_ok() {
                        let _ = sender.send(ResponseMessage::FileSaved(result.inspect(|_res| {
                            // Ensure the path is updated on Save As
                            let _ = sender.send(ResponseMessage::FileLoaded(Ok({
                                FileData {
                                    uuid: "".to_string(),
                                    path,
                                    total_time: 0,
                                    content: "".to_string(),
                                }
                            })));
                        })));
                    } else {
                        let _ = sender.send(ResponseMessage::FileSaved(result));
                    }
                }
            });
        }
    }

    fn apply_save_file(&mut self, uuid: String, total_time: u64) {
        self.editor.set_uuid(uuid);
        self.editor.set_current_file_total_time(total_time);
        if let Some(path) = self.editor.get_current_file() {
            tracing::info!("File saved path: {:?}", path);
            self.config.add_recent_file(path.clone());
        }
    }

    fn apply_load_file_data(&mut self, data: FileData, marks: Option<HashMap<usize, Mark>>) {
        if !data.content.is_empty() {
            self.editor.set_content(data.content);
        }
        self.editor.set_current_file(Some(data.path.clone()));
        if !data.uuid.is_empty() {
            self.editor.set_uuid(data.uuid);
        }
        if data.total_time > 0 {
            self.editor.set_current_file_total_time(data.total_time);
        }
        self.config.add_recent_file(data.path.clone());
        if let Some(data) = marks {
            self.editor.apply_marks(data);
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
        if let Ok(response) = self.response_receiver.try_recv() {
            match response {
                ResponseMessage::FileSaved(result) => match result {
                    Ok((uuid, total_time)) => {
                        self.apply_save_file(uuid, total_time);
                    }
                    Err(e) => tracing::error!("Failed to save file: {}", e),
                },
                ResponseMessage::FileLoaded(result) => match result {
                    Ok(data) => {
                        self.apply_load_file_data(data, None);
                    }
                    Err(e) => tracing::error!("Failed to load file: {}", e),
                },
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
                ResponseMessage::MarksLoaded(result) => match result {
                    Ok(marks) => {
                        self.editor.apply_marks(marks);
                    }
                    Err(e) => tracing::error!("Failed to load marks: {}", e),
                },
                ResponseMessage::OpenFile(path) => {
                    self.try_load_file_data(path);
                }
                ResponseMessage::AiResponse(result) => match result {
                    Ok(response) => {
                        tracing::info!(
                            "AI response received: content_chars={}, tool_calls={}",
                            response.content.chars().count(),
                            response.tool_calls.len()
                        );
                        self.editor.set_ai_response(response);
                    }
                    Err(e) => {
                        self.editor.set_ai_error(e.to_string());
                        tracing::error!("AI request failed: {}", e);
                    }
                },
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

    fn handle_ai_panel_action(&mut self, action: AiPanelAction) {
        match action {
            AiPanelAction::SendRequest { conversation } => {
                let content = self.editor.get_content();

                self.editor.begin_ai_request(content.clone());
                tracing::info!("Sending AI request");

                let ai_backend = Arc::clone(&self.ai_backend);
                let response_sender = self.response_sender.clone();

                ai_backend.discuss_writing_context(
                    content.as_str(),
                    &conversation,
                    response_sender,
                );
            }
            AiPanelAction::ApplyEdit {
                proposal_index,
                base_content,
                original_text,
                replacement_text,
            } => {
                let result =
                    self.editor
                        .apply_ai_edit(&base_content, &original_text, &replacement_text);
                if let Err(error) = &result {
                    tracing::warn!("AI edit was not applied: {}", error);
                } else {
                    tracing::info!("AI edit applied after user confirmation");
                }
                self.editor.set_ai_edit_result(proposal_index, result);
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.check_response_messages();
        self.try_save_marks_if_changed();
        self.update_time_backend_if_focus_changed();

        // Title Bar
        egui::TopBottomPanel::top("title_bar_panel").show(ctx, |ui| {
            let (total_words, cursor_words) = self.editor.get_stats();
            if let Some(action) = crate::ui::title_bar::TitleBar::show(
                ui,
                frame,
                crate::ui::title_bar::TitleBarState {
                    title: crate::constant::DEFAULT_WINDOW_TITLE,
                    word_count: total_words,
                    cursor_word_count: cursor_words,
                    writing_time: self.editor.get_current_file_total_time()
                        + self.time_backend.get_writing_time(),
                    has_current_file: self.editor.get_current_file().is_some(),
                    chinese_fonts: &self.available_fonts,
                    current_font: &self.current_font,
                    recent_files: &self.config.settings.recent_files,
                    is_ai_panel_visible: self.editor.get_ai_panel_mut().is_visible,
                    plugins: &self.plugin_metadata,
                },
            ) {
                match action {
                    crate::ui::title_bar::TitleBarAction::NewWindow => self.spawn_new_window(),
                    crate::ui::title_bar::TitleBarAction::Save => self.try_save_file(),
                    crate::ui::title_bar::TitleBarAction::Open => {
                        self.try_open_file_from_selector()
                    }
                    crate::ui::title_bar::TitleBarAction::OpenFile(path) => self.open_file(path),
                    crate::ui::title_bar::TitleBarAction::Format => self.editor.format(),
                    crate::ui::title_bar::TitleBarAction::History => self.try_load_history(),
                    crate::ui::title_bar::TitleBarAction::SearchReplace => {
                        self.editor.open_search_replace();
                    }
                    crate::ui::title_bar::TitleBarAction::Settings => {
                        self.settings_window.open(&self.config.settings.ai_panel);
                    }
                    crate::ui::title_bar::TitleBarAction::FontChange(font_name) => {
                        let new_fonts = crate::ui::font::apply_font(&font_name);
                        ctx.set_fonts(new_fonts);
                        self.current_font = font_name.clone();
                        tracing::info!("Font changed to: {}", font_name);
                    }
                    crate::ui::title_bar::TitleBarAction::ToggleAiPanel => {
                        let panel = self.editor.get_ai_panel_mut();
                        panel.is_visible = !panel.is_visible;
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
                        } else {
                            if id == "print" {
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

        let mut ai_panel_action = None;
        if self.editor.get_ai_panel_mut().is_visible {
            egui::SidePanel::right("ai_panel_side")
                .default_width(340.0)
                .min_width(260.0)
                .max_width(520.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ai_panel_action = self.editor.get_ai_panel_mut().show(ui);
                });
        }
        if let Some(action) = ai_panel_action {
            self.handle_ai_panel_action(action);
        }

        // Main Content
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    if let Some(action) = self.editor.show(ui) {
                        self.handle_ai_panel_action(action);
                    }
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

        if let Some(ai_config) = self.settings_window.show(ctx) {
            self.config.settings.ai_panel = ai_config;
            self.ai_backend = Arc::new(AiBackend::from_config(&self.config.settings.ai_panel));
            let settings = self.config.settings.clone();
            std::thread::spawn(move || {
                if let Err(e) = confy::store(crate::constant::APP_NAME, None, &settings) {
                    tracing::error!("Failed to save settings: {}", e);
                }
            });
        }

        if let Some(new_config) = self.plugin_config_window.show(ctx) {
            self.config.settings.github_publish = new_config.clone();
            let settings = self.config.settings.clone();
            std::thread::spawn(move || {
                if let Err(e) = confy::store(crate::constant::APP_NAME, None, &settings) {
                    tracing::error!("Failed to save plugin config: {}", e);
                }
            });
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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_file();
    }
}
