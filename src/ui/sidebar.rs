use egui::Ui;

pub enum SidebarAction {
    Save,
    Open,
    Settings,
}

pub struct Sidebar;

impl Sidebar {
    pub fn show(ui: &mut Ui) -> Option<SidebarAction> {
        let mut action = None;
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            if ui.button("💾").on_hover_text("Save").clicked() {
                action = Some(SidebarAction::Save);
            }
            ui.add_space(10.0);
            if ui.button("📂").on_hover_text("Open").clicked() {
                action = Some(SidebarAction::Open);
            }
            ui.add_space(10.0);
            if ui.button("⚙").on_hover_text("Settings").clicked() {
                action = Some(SidebarAction::Settings);
            }
        });
        action
    }
}
