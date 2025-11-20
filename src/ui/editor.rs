use egui::Ui;

#[derive(Default)]
pub struct Editor {
    content: String,
}

impl Editor {
    pub fn show(&mut self, ui: &mut Ui) {
        // We want the editor to take up the available space and look like a paper
        // For now, just a simple text edit
        ui.add(
            egui::TextEdit::multiline(&mut self.content)
                .frame(false) // No border for that clean look
                .lock_focus(true) // Keep focus
                .desired_width(f32::INFINITY)
                .desired_rows(30),
        );
    }
}
