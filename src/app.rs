use crate::backend::EditorBackend;
use crate::sidebar_backend::SidebarBackend;
use crate::style::configure_style;
use crate::ui::editor::Editor;
use crate::ui::history::HistoryWindow;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

/// Response messages from background operations
pub enum BackendResponse {
    SaveComplete(Result<PathBuf, String>),
    LoadComplete(Result<(PathBuf, String), String>),
    HistoryLoaded(Result<Vec<crate::backend::HistoryEntry>, String>),
}

pub struct PaperShellApp {
    editor: Editor,
    backend: Arc<EditorBackend>,
    current_file: Option<PathBuf>,
    response_receiver: Receiver<BackendResponse>,
    response_sender: Sender<BackendResponse>,
    history_window: HistoryWindow,
}

impl Default for PaperShellApp {
    fn default() -> Self {
        let (sender, receiver) = channel();
        let mut editor = Editor::default();
        if let Ok(sidebar_backend) = SidebarBackend::new() {
            editor.set_sidebar_backend(Arc::new(sidebar_backend));
        } else {
            eprintln!("Failed to initialize SidebarBackend");
        }

        Self {
            editor,
            backend: Arc::new(EditorBackend::default()),
            current_file: None,
            response_receiver: receiver,
            response_sender: sender,
            history_window: HistoryWindow::new(),
        }
    }
}

impl PaperShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        Self::default()
    }
}

impl eframe::App for PaperShellApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Check for backend operation responses
        if let Ok(response) = self.response_receiver.try_recv() {
            match response {
                BackendResponse::SaveComplete(result) => match result {
                    Ok(path) => {
                        println!("File saved successfully to {:?}", path);
                        // Update UUID for the saved file
                        let content = self.editor.get_content();
                        if let Ok(uuid) = self.backend.get_uuid(&path, &content) {
                            self.editor.set_uuid(uuid);
                        }
                        self.current_file = Some(path);
                    }
                    Err(e) => eprintln!("Failed to save file: {}", e),
                },
                BackendResponse::LoadComplete(result) => match result {
                    Ok((path, content)) => {
                        self.editor.set_content(content.clone());
                        self.current_file = Some(path.clone());

                        // Update UUID for the loaded file
                        if let Ok(uuid) = self.backend.get_uuid(&path, &content) {
                            self.editor.set_uuid(uuid);
                        }

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
            }
        }

        // Title Bar
        egui::TopBottomPanel::top("title_bar_panel").show(ctx, |ui| {
            let (total_words, cursor_words) = self.editor.get_stats();
            if let Some(action) = crate::ui::title_bar::TitleBar::show(
                ui,
                frame,
                crate::constant::DEFAULT_WINDOW_TITLE,
                total_words,
                cursor_words,
                self.current_file.is_some(),
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
                                    .save(&path, &content)
                                    .map(|_| path.clone())
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
                                        .save(&path, &content)
                                        .map(|_| path.clone())
                                        .map_err(|e| e.to_string());
                                    let _ = sender.send(BackendResponse::SaveComplete(result));
                                }
                            });
                        }
                    }
                    crate::ui::title_bar::TitleBarAction::Open => {
                        let backend = Arc::clone(&self.backend);
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
                                        // Establish UUID tracking by saving
                                        if let Err(e) = backend.save(&path, &content) {
                                            eprintln!("Failed to track file with backend: {}", e);
                                        }

                                        let _ = sender.send(BackendResponse::LoadComplete(Ok((
                                            path, content,
                                        ))));
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

        // History Window
        self.history_window.show(ctx);
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
        if let Err(e) = self.backend.save(&save_path, &content) {
            eprintln!("Failed to track with backend on exit: {}", e);
        } else {
            println!("Auto-saved to {:?}", save_path);
        }
    }
}
