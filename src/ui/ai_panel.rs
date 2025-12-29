use egui::{Align2, Color32, Frame, RichText};

pub struct AiPanel {
    pub is_visible: bool,
    pub is_processing: bool,
    pub last_response: Option<String>,
}

impl Default for AiPanel {
    fn default() -> Self {
        Self {
            is_visible: true,
            is_processing: false,
            last_response: None,
        }
    }
}

impl AiPanel {
    pub fn show(&mut self, ctx: &egui::Context) -> Option<AiPanelAction> {
        if !self.is_visible {
            return None;
        }

        let mut action = None;

        // 计算面板位置 - 右上角，留出边距
        let panel_width = 150.0;
        let panel_height = 300.0;
        let margin = 0.0;
        let top_margin = 25.0; // 留出标题栏空间

        // 半透明背景样式
        let panel_frame = Frame::new()
            .fill(Color32::from_rgba_unmultiplied(200, 200, 200, 255)) // 均衡的淡灰色
            .inner_margin(egui::Margin::same(1))
            .stroke(egui::Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 255),
            ));

        egui::Area::new(egui::Id::new("ai_panel_overlay"))
            .anchor(Align2::RIGHT_TOP, egui::vec2(-margin, top_margin))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                panel_frame.show(ui, |ui| {
                    ui.set_width(panel_width);
                    ui.set_height(panel_height);

                    // 操作按钮
                    let button_text = if self.is_processing {
                        "⏳ Generating..."
                    } else {
                        "Generate"
                    };

                    let button = egui::Button::new(button_text);
                    if ui.add_enabled(!self.is_processing, button).clicked() {
                        action = Some(AiPanelAction::SendRequest);
                    }

                    ui.add_space(2.0);

                    // 状态显示
                    if self.is_processing {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("正在处理中...").size(12.0), //.color(Color32::from_rgb(255, 200, 100)),
                            );
                        });
                    } else if let Some(response) = &self.last_response {
                        egui::ScrollArea::vertical()
                            .max_height(panel_height)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(response).size(11.0), //.color(Color32::from_rgb(220, 220, 220)),
                                );
                            });
                    } else {
                        ui.label(
                            RichText::new("点击下方按钮发送文本给 AI").size(11.0), //.color(Color32::GRAY),
                        );
                    }
                });
            });

        action
    }

    pub fn set_processing(&mut self, processing: bool) {
        self.is_processing = processing;
    }

    pub fn set_response(&mut self, response: String) {
        self.last_response = Some(response);
        self.is_processing = false;
    }
}

#[derive(Debug)]
pub enum AiPanelAction {
    SendRequest,
}
