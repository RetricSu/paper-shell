use egui::{Align, Layout, Ui};

pub enum TitleBarAction {
    NewWindow,
    Save,
    Open,
    History,
    Settings,
    Format,
    FontChange(String),
}

pub struct TitleBar;

pub struct TitleBarState<'a> {
    pub title: &'a str,
    pub word_count: usize,
    pub cursor_word_count: usize,
    pub writing_time: u64,
    pub has_current_file: bool,
    pub chinese_fonts: &'a [String],
    pub current_font: &'a str,
}

impl TitleBar {
    pub fn show(
        ui: &mut Ui,
        _frame: &mut eframe::Frame,
        state: TitleBarState<'_>,
    ) -> Option<TitleBarAction> {
        let TitleBarState {
            title,
            word_count,
            cursor_word_count,
            writing_time,
            has_current_file,
            chinese_fonts,
            current_font,
        } = state;

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

                if ui.button("📂").on_hover_text("Open").clicked() {
                    action = Some(TitleBarAction::Open);
                }
                if ui.button("💾").on_hover_text("Save").clicked() {
                    action = Some(TitleBarAction::Save);
                }
                if ui.button("➕").on_hover_text("New Window").clicked() {
                    action = Some(TitleBarAction::NewWindow);
                }
                ui.menu_button("📝", |ui| {
                    if ui.button("Format").clicked() {
                        action = Some(TitleBarAction::Format);
                        ui.close();
                    }
                });
                ui.menu_button("🔤", |ui| {
                    ui.label("Chinese Fonts:");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for font_name in chinese_fonts {
                                let is_selected = font_name == current_font;
                                if ui.selectable_label(is_selected, font_name).clicked() {
                                    action = Some(TitleBarAction::FontChange(font_name.clone()));
                                    ui.close();
                                }
                            }
                        });
                });
                if ui
                    .add_enabled(has_current_file, egui::Button::new("📜"))
                    .on_hover_text("History")
                    .on_disabled_hover_text("No file opened")
                    .clicked()
                {
                    action = Some(TitleBarAction::History);
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
                let time_str = Self::format_writing_time(writing_time);
                ui.label(
                    egui::RichText::new(format!(
                        "{} / {} | {}",
                        cursor_word_count, word_count, time_str
                    ))
                    .small(),
                );
            });
        });

        action
    }

    /// Format writing time in seconds to a readable string (MM:SS or HH:MM:SS)
    fn format_writing_time(seconds: u64) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        if hours > 0 {
            format!("{:02}:{:02}:{:02}", hours, minutes, secs)
        } else {
            format!("{:02}:{:02}", minutes, secs)
        }
    }
}
