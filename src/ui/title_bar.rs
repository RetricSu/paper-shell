use egui::{Align, Layout, Ui};

pub enum TitleBarAction {
    NewWindow,
    Save,
    Open,
    Settings,
}

pub struct TitleBar;

impl TitleBar {
    pub fn show(
        ui: &mut Ui,
        _frame: &mut eframe::Frame,
        title: &str,
        word_count: usize,
        cursor_word_count: usize,
    ) -> Option<TitleBarAction> {
        let mut action = None;
        let title_bar_rect = ui.available_rect_before_wrap();

        // Dragging logic - registered BEFORE widgets so they can steal input
        let interact = ui.interact(
            title_bar_rect,
            ui.id().with("title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if interact.dragged() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if interact.double_clicked() {
            let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        ui.horizontal(|ui| {
            // Title label and actions
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.label(title);
                ui.add_space(16.0);

                if ui.button("➕").on_hover_text("New Window").clicked() {
                    action = Some(TitleBarAction::NewWindow);
                }
                if ui.button("💾").on_hover_text("Save").clicked() {
                    action = Some(TitleBarAction::Save);
                }
                if ui.button("📂").on_hover_text("Open").clicked() {
                    action = Some(TitleBarAction::Open);
                }
                if ui.button("⚙").on_hover_text("Settings").clicked() {
                    action = Some(TitleBarAction::Settings);
                }
            });

            // Window Controls
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Close button
                if ui.button("❌").on_hover_text("Close").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }

                // Maximize/Restore button
                let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                if ui.button("⛶").on_hover_text("Maximize/Restore").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                }

                // Minimize button
                if ui.button("➖").on_hover_text("Minimize").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                // Stats
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new(format!("{} / {}", cursor_word_count, word_count)).small(),
                );
            });
        });

        action
    }
}
