use egui::{Color32, RichText, Vec2};

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
    pub fn new() -> Self {
        Self::default()
    }

    /// 显示AI助手窗口
    pub fn show(&mut self, ctx: &egui::Context) -> Option<AiPanelAction> {
        if !self.is_visible {
            return None;
        }

        let mut action = None;

        egui::Window::new("🤖 AI 助手")
            .default_width(280.0)
            .default_height(200.0)
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    
                    ui.label(
                        RichText::new("✨ Gemini AI 写作助手")
                            .size(16.0)
                            .strong()
                    );
                    
                    ui.add_space(15.0);

                    // 状态显示
                    if self.is_processing {
                        ui.spinner();
                        ui.label(
                            RichText::new("正在处理中...")
                                .size(14.0)
                                .color(Color32::from_rgb(255, 200, 100))
                        );
                    } else if let Some(response) = &self.last_response {
                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("最新回复:").strong());
                            ui.add_space(5.0);
                            
                            egui::ScrollArea::vertical()
                                .max_height(150.0)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(response)
                                            .size(12.0)
                                    );
                                });
                        });
                    } else {
                        ui.label(
                            RichText::new("点击下方按钮发送文本给 AI 进行改进")
                                .size(12.0)
                                .color(Color32::GRAY)
                        );
                    }
                    
                    ui.add_space(15.0);

                    // 操作按钮
                    let button_text = if self.is_processing {
                        "⏳ 处理中..."
                    } else {
                        "🚀 发送给 AI"
                    };

                    let button = egui::Button::new(
                        RichText::new(button_text)
                            .size(15.0)
                    )
                    .fill(Color32::from_rgb(70, 120, 220))
                    .min_size(Vec2::new(160.0, 40.0));

                    if ui.add_enabled(!self.is_processing, button).clicked() {
                        action = Some(AiPanelAction::SendRequest);
                    }

                    ui.add_space(10.0);
                    
                    ui.separator();
                    ui.label(
                        RichText::new("💡 提示: 需要设置 GEMINI_API_KEY 环境变量")
                            .size(10.0)
                            .color(Color32::DARK_GRAY)
                    );
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
