use crate::style::configure_style;
use crate::ui::editor::Editor;
use crate::ui::sidebar::Sidebar;

#[derive(Default)]
pub struct PaperShellApp {
    editor: Editor,
}

impl PaperShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        Self::default()
    }
}

impl eframe::App for PaperShellApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.style_mut(|style| {
            // Set the width of the blinking text cursor
            style.visuals.text_cursor.stroke.width = 1.0; // Default is usually 2.0
        });

        // Title Bar
        egui::TopBottomPanel::top("title_bar_panel").show(ctx, |ui| {
            crate::ui::title_bar::TitleBar::show(ui, frame, crate::constant::DEFAULT_WINDOW_TITLE);
        });

        // Sidebar
        egui::SidePanel::left("sidebar_panel")
            .resizable(false)
            .default_width(40.0)
            .show(ctx, |ui| {
                Sidebar::show(ui);
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
}
