use crate::backend::ai_backend::AiChatMessage;
use egui::{Color32, Frame, RichText};

#[derive(Default)]
pub struct AiPanel {
    pub is_visible: bool,
    pub is_processing: bool,
    messages: Vec<AiChatMessage>,
    draft_message: String,
}

impl AiPanel {
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<AiPanelAction> {
        let mut action = None;

        let panel_height = ui.available_height();
        let text_color = Color32::from_gray(48);
        let muted_text = Color32::from_gray(130);

        let panel_frame = Frame::new()
            .fill(Color32::from_gray(247))
            .inner_margin(egui::Margin::same(2))
            .stroke(egui::Stroke::NONE);

        panel_frame.show(ui, |ui| {
            ui.set_height(panel_height);

            if !self.messages.is_empty() || self.is_processing {
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height((panel_height - 150.0).max(160.0))
                    .show(ui, |ui| {
                        for message in &self.messages {
                            let is_user = message.role == "user";
                            let label = if is_user { "你" } else { "AI" };
                            ui.add_space(2.0);
                            ui.label(RichText::new(label).size(10.0).strong().color(if is_user {
                                Color32::from_gray(80)
                            } else {
                                muted_text
                            }));
                            Frame::new()
                                .fill(if is_user {
                                    Color32::from_gray(238)
                                } else {
                                    Color32::from_gray(247)
                                })
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(&message.content)
                                            .size(12.0)
                                            .color(text_color),
                                    );
                                });
                        }

                        if self.is_processing {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new("正在处理中...").size(12.0).color(muted_text),
                                );
                            });
                        }
                    });
                ui.add_space(6.0);
            }

            let input = egui::TextEdit::multiline(&mut self.draft_message)
                .hint_text("继续聊，或留空让它观察当前文本")
                .desired_rows(2)
                .font(egui::TextStyle::Small);
            ui.add_sized([ui.available_width(), 46.0], input);

            ui.horizontal(|ui| {
                let button_text = if self.is_processing {
                    "思考中..."
                } else {
                    "一起想"
                };

                let button = egui::Button::new(egui::RichText::new(button_text).size(11.0));
                if ui.add_enabled(!self.is_processing, button).clicked() {
                    let user_message = if self.draft_message.trim().is_empty() {
                        "请根据当前文本，给我一个短的结对观察。".to_string()
                    } else {
                        self.draft_message.trim().to_string()
                    };
                    self.messages.push(AiChatMessage {
                        role: "user".to_string(),
                        content: user_message.clone(),
                    });
                    let conversation = self.messages.clone();
                    self.draft_message.clear();
                    action = Some(AiPanelAction::SendRequest { conversation });
                }

                if ui
                    .add_enabled(
                        !self.is_processing
                            && (!self.draft_message.is_empty() || !self.messages.is_empty()),
                        egui::Button::new(egui::RichText::new("清空").size(11.0)),
                    )
                    .clicked()
                {
                    self.draft_message.clear();
                    self.messages.clear();
                }
            });
        });

        action
    }

    pub fn set_processing(&mut self, processing: bool) {
        self.is_processing = processing;
    }

    pub fn set_response(&mut self, response: String) {
        self.messages.push(AiChatMessage {
            role: "assistant".to_string(),
            content: response,
        });
        self.is_processing = false;
    }
}

#[derive(Debug)]
pub enum AiPanelAction {
    SendRequest { conversation: Vec<AiChatMessage> },
}
