use crate::config::AiPanelConfig;

#[derive(Default)]
pub struct SettingsWindow {
    is_open: bool,
    draft: AiPanelConfig,
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, ai_config: &AiPanelConfig) {
        self.draft = ai_config.clone();
        self.is_open = true;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<AiPanelConfig> {
        if !self.is_open {
            return None;
        }

        let mut saved = None;
        let mut is_open = self.is_open;
        let mut should_close = false;

        egui::Window::new("设置")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("AI 助手").strong());
                ui.add_space(8.0);

                egui::ComboBox::from_label("Provider")
                    .selected_text(provider_label(&self.draft.provider))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut self.draft.provider,
                                "ollama".to_string(),
                                "Ollama 本地",
                            )
                            .clicked()
                        {
                            apply_provider_defaults(&mut self.draft);
                        }
                        if ui
                            .selectable_value(
                                &mut self.draft.provider,
                                "kimi".to_string(),
                                "Kimi for Coding",
                            )
                            .clicked()
                        {
                            apply_provider_defaults(&mut self.draft);
                        }
                    });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("API URL");
                    ui.text_edit_singleline(&mut self.draft.api_url);
                });

                ui.horizontal(|ui| {
                    ui.label("Model");
                    ui.text_edit_singleline(&mut self.draft.model_name);
                });

                ui.horizontal(|ui| {
                    ui.label("API Key");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.api_key)
                            .password(true)
                            .hint_text("Ollama 可留空"),
                    );
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("保存").clicked() {
                        saved = Some(self.draft.clone());
                        should_close = true;
                    }
                    if ui.button("取消").clicked() {
                        should_close = true;
                    }
                });
            });

        if should_close {
            is_open = false;
        }
        self.is_open = is_open;
        saved
    }
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "kimi" => "Kimi for Coding",
        _ => "Ollama 本地",
    }
}

fn apply_provider_defaults(config: &mut AiPanelConfig) {
    match config.provider.as_str() {
        "kimi" => {
            config.api_url = "https://api.moonshot.ai/v1/chat/completions".to_string();
            config.model_name = "kimi-k2.7-code".to_string();
        }
        _ => {
            config.provider = "ollama".to_string();
            config.api_url = "http://localhost:11434/api/chat".to_string();
            config.model_name = "qwen3:8b".to_string();
        }
    }
}
