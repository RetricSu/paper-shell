use crate::backend::sidebar_backend::Mark;
use egui::{Color32, Galley, Pos2, Rect, Sense, Ui};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct Sidebar {
    marks: HashMap<usize, Mark>,
    popup_mark: Option<usize>,
    current_uuid: Option<String>,
    marks_changed: bool,
}

impl Sidebar {
    pub fn set_uuid(&mut self, uuid: String) {
        if self.current_uuid.as_ref() != Some(&uuid) {
            self.current_uuid = Some(uuid);
            // Clear marks when UUID changes - they will be loaded by App
            self.marks.clear();
            self.marks_changed = false;
        }
    }

    pub fn apply_marks(&mut self, marks: HashMap<usize, Mark>) {
        self.marks = marks;
        self.marks_changed = false;
    }

    pub fn marks_changed(&self) -> bool {
        self.marks_changed
    }

    pub fn get_marks(&self) -> &HashMap<usize, Mark> {
        &self.marks
    }

    pub fn get_uuid(&self) -> Option<&String> {
        self.current_uuid.as_ref()
    }

    pub fn reset_marks_changed(&mut self) {
        self.marks_changed = false;
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        content: &str, // 这个参数现在仅用于点击后的逻辑，不用于渲染循环
        galley: &Arc<Galley>,
        sidebar_rect: Rect,
        clip_rect: Rect,
        text_offset: Pos2,
    ) {
        let painter = ui.painter_at(sidebar_rect);

        // 绘制分割线
        painter.line_segment(
            [
                Pos2::new(sidebar_rect.right(), sidebar_rect.top()),
                Pos2::new(sidebar_rect.right(), sidebar_rect.bottom()),
            ],
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        // 交互处理
        let response = ui.interact(sidebar_rect, ui.id().with("sidebar"), Sense::click());
        let pointer_pos = response.interact_pointer_pos();
        let mut clicked_logical_line: Option<usize> = None;

        // --- 🚀 核心优化开始 ---

        // 我们需要维护“逻辑行号”(logical_line_idx)，因为 Galley 的 Row 包含自动换行(wrap)产生的视觉行
        let mut logical_line_idx = 0;

        // 标记当前视觉行是否是一个逻辑行的开头
        let mut is_start_of_logical_line = true;

        // 记录最后一行底部位置，用于处理文末可能存在的空行
        let mut last_row_bottom_y = text_offset.y;

        // 直接遍历 Galley 的预计算行信息 (速度极快)
        for row in &galley.rows {
            // 计算当前行的屏幕绝对位置
            // text_offset 是 TextEdit 的左上角，row.rect 是相对于 TextEdit 的
            let row_screen_top = text_offset.y + row.rect().top();
            let row_screen_bottom = text_offset.y + row.rect().bottom();
            last_row_bottom_y = row_screen_bottom;

            // 如果这是一个新逻辑行的开头，我们就需要绘制侧边栏标记
            if is_start_of_logical_line {
                // ✂️ 视锥剔除 (Culling)
                // 如果这一行完全在屏幕上方，或者完全在屏幕下方，跳过绘制
                // 加上 20.0 padding 防止边缘闪烁
                let is_visible = row_screen_bottom >= clip_rect.top() - 20.0
                    && row_screen_top <= clip_rect.bottom() + 20.0;

                if is_visible {
                    let center_y = (row_screen_top + row_screen_bottom) / 2.0;
                    let center = Pos2::new(sidebar_rect.center().x, center_y);

                    // 1. 绘制 UI (小圆点)
                    painter.circle_stroke(
                        center,
                        2.5,
                        egui::Stroke::new(1.0, ui.visuals().text_color().gamma_multiply(0.3)),
                    );

                    if self.marks.contains_key(&logical_line_idx) {
                        painter.circle_filled(center, 4.0, Color32::from_rgb(200, 100, 100));
                    }

                    // 2. 点击检测 (顺便做，省去额外遍历)
                    if response.clicked()
                        && let Some(pos) = pointer_pos
                    {
                        // 如果点击位置在当前行的高度范围内
                        if pos.y >= row_screen_top && pos.y <= row_screen_bottom {
                            clicked_logical_line = Some(logical_line_idx);
                        }
                    }
                }
            }

            // 更新状态
            if row.ends_with_newline {
                // 如果这一行以换行符结束，说明下一行是新的逻辑行
                logical_line_idx += 1;
                is_start_of_logical_line = true;
            } else {
                // 否则说明这行太长被自动折行了，下一行依然属于当前逻辑行
                is_start_of_logical_line = false;
            }
        }

        // 处理特殊的边界情况：文件末尾有换行符，导致最后有一个空的逻辑行
        // 这个空行在 galley.rows 里通常没有对应的 row
        if is_start_of_logical_line && content.ends_with('\n') {
            // 估算空行的位置（假设高度和最后一行一样，或者默认值）
            let line_height = if !galley.rows.is_empty() {
                galley.rows[0].rect().height()
            } else {
                14.0
            };
            let center_y = last_row_bottom_y + line_height / 2.0;

            // 同样检查可见性
            if center_y >= clip_rect.top() - 20.0 && center_y <= clip_rect.bottom() + 20.0 {
                let center = Pos2::new(sidebar_rect.center().x, center_y);

                painter.circle_stroke(
                    center,
                    2.5,
                    egui::Stroke::new(1.0, ui.visuals().text_color().gamma_multiply(0.3)),
                );

                if self.marks.contains_key(&logical_line_idx) {
                    painter.circle_filled(center, 4.0, Color32::from_rgb(200, 100, 100));
                }

                if response.clicked()
                    && let Some(pos) = pointer_pos
                    && (pos.y - center_y).abs() < line_height / 2.0
                {
                    clicked_logical_line = Some(logical_line_idx);
                }
            }
        }

        // --- 🚀 核心优化结束 ---

        // 处理点击事件结果
        if let Some(line_idx) = clicked_logical_line {
            if let std::collections::hash_map::Entry::Vacant(e) = self.marks.entry(line_idx) {
                e.insert(Mark::default());
                self.popup_mark = Some(line_idx);
                self.marks_changed = true;
            } else if self.popup_mark == Some(line_idx) {
                self.popup_mark = None;
            } else {
                self.popup_mark = Some(line_idx);
            }
        }

        // 渲染弹窗
        self.show_popup(ui, content);
    }

    fn show_popup(&mut self, ui: &Ui, content: &str) {
        if let Some(line_idx) = self.popup_mark {
            let mut open = true;

            // Calculate word count before this mark
            let words_before = self.calculate_words_before(content, line_idx);

            let mut changed = false;
            {
                let mark_note = self.marks.get_mut(&line_idx).map(|m| &mut m.note);

                if let Some(note) = mark_note {
                    egui::Window::new(
                        egui::RichText::new(format!(
                            "{} {} 词",
                            egui_phosphor::regular::NOTE_PENCIL,
                            words_before
                        ))
                        .size(11.0),
                    )
                    .open(&mut open)
                    .resizable(true)
                    .collapsible(false)
                    .default_width(300.0)
                    .title_bar(true)
                    .show(ui.ctx(), |ui| {
                        // Reduce spacing in the window
                        ui.spacing_mut().item_spacing.y = 4.0;

                        if ui
                            .add(
                                egui::TextEdit::multiline(note)
                                    .desired_rows(8)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                }
            }

            if changed {
                self.marks_changed = true;
            }

            if !open {
                self.popup_mark = None;
            }
        }
    }

    fn calculate_words_before(&self, content: &str, line_idx: usize) -> usize {
        let mut byte_count = 0;

        for (current_line, line) in content.split_inclusive('\n').enumerate() {
            if current_line >= line_idx {
                break;
            }
            byte_count += line.len();
        }

        // Use the same word counting logic
        let text_before = &content[..byte_count.min(content.len())];
        let mut count = 0;
        let mut in_word = false;
        for c in text_before.chars() {
            if c.is_whitespace() {
                in_word = false;
            } else if is_cjk(c) {
                count += 1;
                in_word = false;
            } else if !in_word {
                count += 1;
                in_word = true;
            }
        }
        count
    }
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{2F800}'..='\u{2FA1F}').contains(&c)
}
