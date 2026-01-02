use egui::{Align2, Color32, Frame, RichText};

#[derive(Default)]
pub struct AiPanel {
    pub is_visible: bool,
    pub is_processing: bool,
    pub last_response: Option<Vec<String>>,
    narrative_map_changed: bool,
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

        // 检测鼠标是否在面板上方
        let screen_size = ctx.content_rect();
        let panel_rect = egui::Rect::from_min_size(
            egui::pos2(screen_size.right() - panel_width - margin, top_margin),
            egui::vec2(panel_width, panel_height),
        );

        let is_hovered = ctx.input(|i| {
            i.pointer
                .latest_pos()
                .map(|pos| panel_rect.contains(pos))
                .unwrap_or(false)
        });

        // 根据鼠标状态设置透明度：鼠标在上方时完全不透明，离开时高度透明
        let fill_alpha = if is_hovered { 220 } else { 120 };
        let text_color_alpha = if is_hovered { 255 } else { 30 };

        // 半透明背景样式
        let panel_frame = Frame::new()
            .fill(Color32::from_rgba_unmultiplied(200, 200, 200, fill_alpha)) // 动态透明度
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
                        "⏳ 生成中..."
                    } else {
                        "导览地图"
                    };

                    let button = egui::Button::new(egui::RichText::new(button_text).size(10.0));
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
                                egui::Frame::new().inner_margin(8.0).show(ui, |ui| {
                                    let text = response.iter().enumerate().fold(
                                        String::new(),
                                        |mut acc, (i, line)| {
                                            acc.push_str(&format!("{}. {}\n", i + 1, line));
                                            acc
                                        },
                                    );
                                    ui.label(
                                        RichText::new(text).size(11.0).color(
                                            Color32::from_rgba_unmultiplied(
                                                0,
                                                0,
                                                0,
                                                text_color_alpha,
                                            ),
                                        ), //.color(Color32::from_rgb(220, 220, 220)),
                                    );
                                });
                            });
                    }
                });
            });

        action
    }

    pub fn set_processing(&mut self, processing: bool) {
        self.is_processing = processing;
    }

    pub fn set_response(&mut self, response: Vec<String>) {
        self.last_response = Some(response);
        self.is_processing = false;
        self.narrative_map_changed = true;
    }

    pub fn apply_narrative_map(&mut self, map: Vec<String>) {
        self.last_response = Some(map);
        // Don't set narrative_map_changed since this is loading from disk
    }

    pub fn narrative_map_changed(&self) -> bool {
        self.narrative_map_changed
    }

    pub fn reset_narrative_map_changed(&mut self) {
        self.narrative_map_changed = false;
    }

    pub fn get_narrative_map(&self) -> Option<&Vec<String>> {
        self.last_response.as_ref()
    }
}

#[derive(Debug)]
pub enum AiPanelAction {
    SendRequest,
}
