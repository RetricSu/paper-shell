use egui::Ui;

pub struct Sidebar;

impl Sidebar {
    pub fn show(ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            if ui.button("💾").on_hover_text("Save").clicked() {
                // TODO: Save logic
            }
            ui.add_space(10.0);
            if ui.button("📂").on_hover_text("Open").clicked() {
                // TODO: Open logic
            }
            ui.add_space(10.0);
            if ui.button("⚙").on_hover_text("Settings").clicked() {
                // TODO: Settings logic
            }
        });
    }
}
