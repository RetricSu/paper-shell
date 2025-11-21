use egui::Ui;

#[derive(Default)]
pub struct Editor {
    content: String,
}

impl Editor {
    pub fn show(&mut self, ui: &mut Ui) {
        // We want the editor to take up the available space and look like a paper
        // For now, just a simple text edit
        let response = ui.add(
            egui::TextEdit::multiline(&mut self.content)
                //.frame(false) // No border for that clean look
                .desired_width(f32::INFINITY)
                .desired_rows(30),
        );

        if response.clicked() {
            response.request_focus();
        }
    }

    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
    }
}
