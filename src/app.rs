use crate::backend::EditorBackend;
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
pub enum BackendResponse {
    SaveComplete(Result<(String, u64), String>), // (uuid, total_time)
    LoadComplete(Result<(PathBuf, String, String, u64), String>), // (path, content, uuid, total_time)
    HistoryLoaded(Result<Vec<crate::backend::HistoryEntry>, String>),
    MarksLoaded(Result<HashMap<usize, Mark>, String>),
}

pub struct PaperShellApp {
    editor: Editor,
    backend: Arc<EditorBackend>,
    sidebar_backend: Arc<SidebarBackend>,
    time_backend: TimeBackend,
    current_file: Option<PathBuf>,
    response_receiver: Receiver<BackendResponse>,
    response_sender: Sender<BackendResponse>,
    history_window: HistoryWindow,
    available_fonts: Vec<String>,
    current_font: String,
    last_focus_state: bool,
    current_file_total_time: u64,
}

impl Default for PaperShellApp {
    fn default() -> Self {
        let (sender, receiver) = channel();
        let editor = Editor::default();
        let sidebar_backend = Arc::new(SidebarBackend::new().unwrap_or_else(|e| {
            eprintln!("Failed to initialize SidebarBackend: {}", e);
            panic!("Cannot continue without SidebarBackend");
        }));

        Self {
            editor,
            backend: Arc::new(EditorBackend::default()),
            sidebar_backend,
            time_backend: TimeBackend::default(),
            current_file: None,
            response_receiver: receiver,
            response_sender: sender,
            history_window: HistoryWindow::new(),
            available_fonts: Vec::new(),
            current_font: "Default".to_string(),
            last_focus_state: false,
            current_file_total_time: 0,
        }
    }
}

impl PaperShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        Self {
            available_fonts: crate::ui::font::enumerate_chinese_fonts(),
            ..Default::default()
        }
    }
}

impl eframe::App for PaperShellApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Check for backend operation responses
        if let Ok(response) = self.response_receiver.try_recv() {
            match response {
                BackendResponse::SaveComplete(result) => match result {
                    Ok((uuid, total_time)) => {
                        println!("File saved successfully");
                        self.editor.set_uuid(uuid);
                        self.current_file_total_time = total_time;
                        if let Some(path) = self.current_file.as_ref() {
                            println!("File path: {:?}", path);
                        }
                    }
                    Err(e) => eprintln!("Failed to save file: {}", e),
                },
                BackendResponse::LoadComplete(result) => match result {
                    Ok((path, content, uuid, total_time)) => {
                        self.editor.set_content(content);
                        self.current_file = Some(path.clone());
                        self.editor.set_uuid(uuid);
                        self.current_file_total_time = total_time;
                        println!("File opened: {:?}", path);
                    }
                    Err(e) => eprintln!("Failed to load file: {}", e),
                },
                BackendResponse::HistoryLoaded(result) => match result {
                    Ok(entries) => {
                        if let Err(e) = self.history_window.set_history(entries, &self.backend) {
                            eprintln!("Failed to set history: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Failed to load history: {}", e),
                },
                BackendResponse::MarksLoaded(result) => match result {
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
                                    let _ = sender.send(BackendResponse::SaveComplete(Err(
                                        format!("Failed to write file: {}", e),
                                    )));
                                    return;
                                }

                                // Then track with backend (CAS + history)
                                let result = backend
                                    .save(&path, &content, time_spent)
                                    .map_err(|e| e.to_string());
                                let _ = sender.send(BackendResponse::SaveComplete(result));
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
                                        let _ = sender.send(BackendResponse::SaveComplete(Err(
                                            format!("Failed to write file: {}", e),
                                        )));
                                        return;
                                    }

                                    // Then track with backend (CAS + history)
                                    let result = backend
                                        .save(&path, &content, time_spent)
                                        .map_err(|e| e.to_string());
                                    let _ = sender.send(BackendResponse::SaveComplete(result));
                                }
                            });
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::Open => {
                        let backend = Arc::clone(&self.backend);
                        let sidebar_backend = Arc::clone(&self.sidebar_backend);
                        let sender = self.response_sender.clone();
                        let data_dir = backend.data_dir().to_path_buf();

                        std::thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_directory(&data_dir)
                                .add_filter("Text", &["txt"])
                                .pick_file()
                            {
                                // Read file content
                                match std::fs::read_to_string(&path) {
                                    Ok(content) => {
                                        // Get UUID and total time in one operation (optimized)
                                        match backend.get_file_metadata(&path, &content) {
                                            Ok((uuid, total_time)) => {
                                                let _ = sender.send(BackendResponse::LoadComplete(
                                                    Ok((
                                                        path.clone(),
                                                        content.clone(),
                                                        uuid.clone(),
                                                        total_time,
                                                    )),
                                                ));

                                                // Load marks for this file
                                                let marks_result = sidebar_backend
                                                    .load_marks(&uuid)
                                                    .map_err(|e| e.to_string());
                                                let _ = sender.send(BackendResponse::MarksLoaded(
                                                    marks_result,
                                                ));
                                            }
                                            Err(e) => {
                                                let _ = sender.send(BackendResponse::LoadComplete(
                                                    Err(format!("Failed to get metadata: {}", e)),
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = sender.send(BackendResponse::LoadComplete(Err(
                                            format!("Failed to read file {:?}: {}", path, e),
                                        )));
                                    }
                                }
                            }
                        });
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
                                let _ = sender.send(BackendResponse::HistoryLoaded(result));
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
