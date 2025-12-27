use crate::backend::EditorBackend;
use crate::file::FileData;
use crate::sidebar_backend::{Mark, SidebarBackend};
use crate::style::configure_style;
use crate::ui::editor::Editor;
use crate::ui::history::HistoryWindow;
use paper_shell::time_backend::TimeBackend;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

/// Response messages from background operations
pub enum ResponseMessage {
    SaveComplete(Result<(String, u64), String>), // (uuid, total_time)
    LoadComplete(Result<(PathBuf, String, String, u64), String>), // (path, content, uuid, total_time), error
    HistoryLoaded(Result<Vec<crate::backend::HistoryEntry>, String>),
    MarksLoaded(Result<HashMap<usize, Mark>, String>),
    OpenFile(PathBuf),
}

pub struct PaperShellApp {
    editor: Editor,
    current_file: Option<PathBuf>,
    current_file_total_time: u64,

    response_sender: Sender<ResponseMessage>,
    response_receiver: Receiver<ResponseMessage>,

    history_window: HistoryWindow,

    current_font: String,
    available_fonts: Vec<String>,

    last_focus_state: bool,
    config: crate::config::Config,

    backend: Arc<EditorBackend>,
    sidebar_backend: Arc<SidebarBackend>,
    time_backend: TimeBackend,
}

impl Default for PaperShellApp {
    fn default() -> Self {
        let (sender, receiver) = channel();
        let editor = Editor::default();
        let sidebar_backend = Arc::new(SidebarBackend::new().unwrap_or_else(|e| {
            tracing::error!("Failed to initialize SidebarBackend: {}", e);
            panic!("Cannot continue without SidebarBackend");
        }));
        let available_fonts = crate::ui::font::enumerate_chinese_fonts();

        Self {
            editor,
            backend: Arc::new(EditorBackend::default()),
            sidebar_backend,
            time_backend: TimeBackend::default(),
            current_file: None,
            response_receiver: receiver,
            response_sender: sender,
            history_window: HistoryWindow::new(),
            available_fonts,
            current_font: "Default".to_string(),
            last_focus_state: false,
            current_file_total_time: 0,
            config: crate::config::Config::default(),
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

    // this is mostly the same process with load_file_data but in a thread with messaging
    fn try_load_file_data(&mut self, path: PathBuf) {
        let backend = Arc::clone(&self.backend);
        let sidebar_backend = Arc::clone(&self.sidebar_backend);
        let sender = self.response_sender.clone();

        std::thread::spawn(move || match std::fs::read_to_string(&path) {
            Ok(content) => match backend.get_file_metadata(&path, &content) {
                Ok((uuid, total_time)) => {
                    let _ = sender.send(ResponseMessage::LoadComplete(Ok((
                        path,
                        content,
                        uuid.clone(),
                        total_time,
                    ))));

                    let marks_result = sidebar_backend.load_marks(&uuid).map_err(|e| e.to_string());
                    let _ = sender.send(ResponseMessage::MarksLoaded(marks_result));
                }
                Err(e) => {
                    let _ = sender.send(ResponseMessage::LoadComplete(Err(format!(
                        "Failed to get metadata: {}",
                        e
                    ))));
                }
            },
            Err(e) => {
                let _ = sender.send(ResponseMessage::LoadComplete(Err(format!(
                    "Failed to read file {:?}: {}",
                    path, e
                ))));
            }
        });
    }

    fn open_file(&mut self, path: PathBuf) {
        match self.load_file_data(&path) {
            Ok(data) => {
                self.apply_file_data(data.0, data.1);
            }
            Err(e) => {
                tracing::error!("{}", e);
            }
        }
    }

    fn apply_file_data(&mut self, data: FileData, marks: HashMap<usize, Mark>) {
        self.editor.set_content(data.content);
        self.current_file = Some(data.path.clone());
        self.editor.set_uuid(data.uuid);
        self.current_file_total_time = data.total_time;
        self.config.add_recent_file(data.path);
        self.editor.apply_marks(marks);
    }

    fn load_file_data(
        &self,
        path: &PathBuf,
    ) -> Result<(FileData, HashMap<usize, Mark>), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e: std::io::Error| format!("Failed to read file {:?}: {}", path, e))?;

        let (uuid, total_time) = self
            .backend
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
}

impl eframe::App for PaperShellApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Check for backend operation responses
        if let Ok(response) = self.response_receiver.try_recv() {
            match response {
                ResponseMessage::SaveComplete(result) => match result {
                    Ok((uuid, total_time)) => {
                        println!("File saved successfully");
                        self.editor.set_uuid(uuid);
                        self.current_file_total_time = total_time;
                        if let Some(path) = self.current_file.as_ref() {
                            println!("File path: {:?}", path);
                            self.config.add_recent_file(path.clone());
                        }
                    }
                    Err(e) => eprintln!("Failed to save file: {}", e),
                },
                ResponseMessage::LoadComplete(result) => match result {
                    Ok((path, content, uuid, total_time)) => {
                        if !content.is_empty() {
                            self.editor.set_content(content);
                        }
                        self.current_file = Some(path.clone());
                        if !uuid.is_empty() {
                            self.editor.set_uuid(uuid);
                        }
                        if total_time > 0 {
                            self.current_file_total_time = total_time;
                        }
                        self.config.add_recent_file(path.clone());
                        println!("File opened: {:?}", path);
                    }
                    Err(e) => eprintln!("Failed to load file: {}", e),
                },
                ResponseMessage::OpenFile(path) => {
                    self.try_load_file_data(path);
                }
                ResponseMessage::HistoryLoaded(result) => match result {
                    Ok(entries) => {
                        if let Err(e) = self.history_window.set_history(entries, &self.backend) {
                            eprintln!("Failed to set history: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Failed to load history: {}", e),
                },
                ResponseMessage::MarksLoaded(result) => match result {
                    Ok(marks) => {
                        self.editor.apply_marks(marks);
                    }
                    Err(e) => eprintln!("Failed to load marks: {}", e),
                },
            }
        }

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
                    writing_time: self.current_file_total_time
                        + self.time_backend.get_writing_time(),
                    has_current_file: self.current_file.is_some(),
                    chinese_fonts: &self.available_fonts,
                    current_font: &self.current_font,
                    recent_files: &self.config.settings.recent_files,
                },
            ) {
                match action {
                    crate::ui::title_bar::TitleBarAction::NewWindow => {
                        // Spawn a new instance of the application
                        if let Err(e) =
                            std::process::Command::new(std::env::current_exe().unwrap()).spawn()
                        {
                            eprintln!("Failed to spawn new window: {}", e);
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::Save => {
                        let content = self.editor.get_content();
                        let backend = Arc::clone(&self.backend);
                        let sender = self.response_sender.clone();
                        let time_spent = self.time_backend.get_and_reset_writing_time();

                        if let Some(ref path) = self.current_file {
                            // Save to existing file in background thread
                            let path = path.clone();
                            std::thread::spawn(move || {
                                // First write the actual file content
                                if let Err(e) = std::fs::write(&path, &content) {
                                    let _ = sender.send(ResponseMessage::SaveComplete(Err(
                                        format!("Failed to write file: {}", e),
                                    )));
                                    return;
                                }

                                // Then track with backend (CAS + history)
                                let result = backend
                                    .save(&path, &content, time_spent)
                                    .map_err(|e| e.to_string());
                                let _ = sender.send(ResponseMessage::SaveComplete(result));
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
                                        let _ = sender.send(ResponseMessage::SaveComplete(Err(
                                            format!("Failed to write file: {}", e),
                                        )));
                                        return;
                                    }

                                    // Then track with backend (CAS + history)
                                    let result = backend
                                        .save(&path, &content, time_spent)
                                        .map_err(|e| e.to_string());

                                    // Add to recent files on successful save
                                    if result.is_ok() {
                                        let _ = sender.send(ResponseMessage::SaveComplete(
                                            result.inspect(|_res| {
                                                // Ensure the path is updated on Save As
                                                let _ = sender.send(ResponseMessage::LoadComplete(
                                                    Ok((
                                                        path.clone(),
                                                        "".to_string(), // Empty content means don't update editor content
                                                        "".to_string(),
                                                        0,
                                                    )),
                                                ));
                                            }),
                                        ));
                                    } else {
                                        let _ = sender.send(ResponseMessage::SaveComplete(result));
                                    }
                                }
                            });
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::Open => {
                        let backend = Arc::clone(&self.backend);
                        let data_dir = backend.data_dir().to_path_buf();
                        let ctx = ctx.clone();

                        // Keep a reference to the sender to use in the outer scope
                        let sender = self.response_sender.clone();
                        std::thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_directory(&data_dir)
                                .add_filter("Text", &["txt"])
                                .pick_file()
                            {
                                // Instead of calling self.open_file_at_path (impossible),
                                // we can just perform the load here or send a message.
                                // Let's just do what open_file_at_path does but in this thread.
                                let _ = sender.send(ResponseMessage::OpenFile(path));
                                ctx.request_repaint();
                            }
                        });
                    }
                    crate::ui::title_bar::TitleBarAction::OpenFile(path) => {
                        self.try_load_file_data(path);
                    }
                    crate::ui::title_bar::TitleBarAction::Format => {
                        self.editor.format();
                    }
                    crate::ui::title_bar::TitleBarAction::History => {
                        if let Some(ref path) = self.current_file {
                            let backend = Arc::clone(&self.backend);
                            let sender = self.response_sender.clone();
                            let path = path.clone();

                            std::thread::spawn(move || {
                                let result = backend.load_history(&path).map_err(|e| e.to_string());
                                let _ = sender.send(ResponseMessage::HistoryLoaded(result));
                            });

                            self.history_window.open();
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::Settings => {
                        // TODO: Settings logic
                    }
                    crate::ui::title_bar::TitleBarAction::FontChange(font_name) => {
                        let new_fonts = crate::ui::font::apply_font(&font_name);
                        ctx.set_fonts(new_fonts);
                        self.current_font = font_name.clone();
                        tracing::info!("Font changed to: {}", font_name);
                    }
                }
            }
        });

        // Main Content
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    self.editor.show(ui);
                });
            });
        });

        // Update time backend with current focus state if changed
        let is_focused = self.editor.is_focused();
        if is_focused != self.last_focus_state {
            self.time_backend.update_focus(is_focused);
            self.last_focus_state = is_focused;
        }

        // History Window
        self.history_window.show(ctx);

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
                    eprintln!("Failed to save marks in background: {}", e);
                }
            });
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let content = self.editor.get_content();
        if content.trim().is_empty() {
            return;
        }

        // Auto-save to current file or create new timestamped file
        // This is blocking, but acceptable since the app is closing
        let save_path = if let Some(ref path) = self.current_file {
            path.clone()
        } else {
            // Create timestamped file in data directory
            let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
            self.backend.data_dir().join(format!("{}.txt", timestamp))
        };

        // First write the actual file content
        if let Err(e) = std::fs::write(&save_path, &content) {
            eprintln!("Failed to write file on exit: {}", e);
            return;
        }

        // Then track with backend (CAS + history)
        let time_spent = self.time_backend.get_writing_time();
        if let Err(e) = self.backend.save(&save_path, &content, time_spent) {
            eprintln!("Failed to track with backend on exit: {}", e);
        } else {
            println!("Auto-saved to {:?}", save_path);
        }
    }
}
