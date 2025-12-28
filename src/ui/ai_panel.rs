use egui::{Align2, Color32, CornerRadius, Frame, RichText, Vec2};

pub struct AiPanel {
    pub is_visible: bool,
    pub is_processing: bool,
    pub last_response: Option<String>,
    is_collapsed: bool,
}

impl Default for AiPanel {
    fn default() -> Self {
        Self {
            is_visible: true,
            is_processing: false,
            last_response: None,
            is_collapsed: false,
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
        let margin = 20.0;
        let top_margin = 20.0; // 留出标题栏空间

        // 半透明背景样式
        let panel_frame = Frame::new()
            .fill(Color32::from_rgba_unmultiplied(200, 200, 200, 160)) // 均衡的淡灰色
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(16))
            .stroke(egui::Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 30),
            ));

        egui::Area::new(egui::Id::new("ai_panel_overlay"))
            .anchor(Align2::RIGHT_TOP, egui::vec2(-margin, top_margin))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                panel_frame.show(ui, |ui| {
                    ui.set_width(panel_width);

                    // 标题栏 - 可折叠
                    ui.horizontal(|ui| {
                        let collapse_icon = if self.is_collapsed { "▶" } else { "▼" };
                        if ui.small_button(collapse_icon).clicked() {
                            self.is_collapsed = !self.is_collapsed;
                        }

                        ui.label(
                            RichText::new("🤖 AI 助手")
                                .size(14.0)
                                .strong()
                                .color(Color32::WHITE),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                self.is_visible = false;
                            }
                        });
                    });

                    if !self.is_collapsed {
                        ui.add_space(10.0);

                        // 状态显示
                        if self.is_processing {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new("正在处理中...")
                                        .size(12.0)
                                        .color(Color32::from_rgb(255, 200, 100)),
                                );
                            });
                        } else if let Some(response) = &self.last_response {
                            // 显示回复区域
                            let response_frame = Frame::new()
                                .fill(Color32::from_rgba_unmultiplied(50, 50, 55, 200))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(egui::Margin::same(10));

                            response_frame.show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new("最新回复:")
                                        .size(11.0)
                                        .strong()
                                        .color(Color32::LIGHT_GRAY),
                                );
                                ui.add_space(4.0);

                                egui::ScrollArea::vertical()
                                    .max_height(120.0)
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(response)
                                                .size(11.0)
                                                .color(Color32::from_rgb(220, 220, 220)),
                                        );
                                    });
                            });
                        } else {
                            ui.label(
                                RichText::new("点击下方按钮发送文本给 AI")
                                    .size(11.0)
                                    .color(Color32::GRAY),
                            );
                        }

                        ui.add_space(12.0);

                        // 操作按钮
                        let button_text = if self.is_processing {
                            "⏳ Generating..."
                        } else {
                            "🚀 Generate Outline"
                        };

                        let button = egui::Button::new(
                            RichText::new(button_text).size(13.0).color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgba_unmultiplied(70, 120, 220, 220))
                        .corner_radius(CornerRadius::same(8))
                        .min_size(Vec2::new(ui.available_width(), 36.0));

                        if ui.add_enabled(!self.is_processing, button).clicked() {
                            action = Some(AiPanelAction::SendRequest);
                        }

                        ui.add_space(8.0);

                        ui.label(
                            RichText::new("💡 需要设置 GEMINI_API_KEY")
                                .size(9.0)
                                .color(Color32::from_rgb(120, 120, 120)),
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
