use crate::saver::{SaverMessage, SaverResponse, spawn_saver};
use crate::style::configure_style;
use crate::ui::editor::Editor;
use crate::ui::sidebar::{Sidebar, SidebarAction};
use std::sync::mpsc::{Receiver, Sender};

pub struct PaperShellApp {
    editor: Editor,
    saver_sender: Sender<SaverMessage>,
    saver_receiver: Receiver<SaverResponse>,
}

impl Default for PaperShellApp {
    fn default() -> Self {
        let (sender, receiver) = spawn_saver();
        Self {
            editor: Editor::default(),
            saver_sender: sender,
            saver_receiver: receiver,
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
        // Check for loaded content
        if let Ok(response) = self.saver_receiver.try_recv() {
            match response {
                SaverResponse::Loaded(content) => {
                    self.editor.set_content(content);
                }
            }
        }

        // Title Bar
        egui::TopBottomPanel::top("title_bar_panel").show(ctx, |ui| {
            let (total_words, cursor_words) = self.editor.get_stats();
            crate::ui::title_bar::TitleBar::show(
                ui,
                frame,
                crate::constant::DEFAULT_WINDOW_TITLE,
                total_words,
                cursor_words,
            );
        });

        // Sidebar
        egui::SidePanel::left("sidebar_panel")
            .resizable(false)
            .default_width(40.0)
            .show(ctx, |ui| {
                if let Some(action) = Sidebar::show(ui) {
                    match action {
                        SidebarAction::Save => {
                            let content = self.editor.get_content();
                            if let Err(e) = self.saver_sender.send(SaverMessage::Save(content)) {
                                eprintln!("Failed to send save message: {}", e);
                            }
                        }
                        SidebarAction::Open => {
                            let sender = self.saver_sender.clone();
                            std::thread::spawn(move || {
                                let data_dir = if let Some(proj_dirs) =
                                    directories::ProjectDirs::from("com", "RetricSu", "Paper Shell")
                                {
                                    proj_dirs.data_dir().to_path_buf()
                                } else {
                                    std::path::PathBuf::from("data")
                                };

                                if let Some(path) = rfd::FileDialog::new()
                                    .set_directory(&data_dir)
                                    .add_filter("Text", &["txt"])
                                    .pick_file()
                                    && let Err(e) = sender.send(SaverMessage::Open(path))
                                {
                                    eprintln!("Failed to send open message: {}", e);
                                }
                            });
                        }
                        SidebarAction::Settings => {
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
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let content = self.editor.get_content();
        if !content.trim().is_empty()
            && let Err(e) = crate::saver::save_content(&content)
        {
            eprintln!("Failed to save on exit: {}", e);
        }
    }
}
