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
        content: &str,
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

        // 处理点击交互
        let response = ui.interact(sidebar_rect, ui.id().with("sidebar"), Sense::click());
        let pointer_pos = response.interact_pointer_pos(); // 获取点击位置
        let mut clicked_line_index: Option<usize> = None;

        // 状态累加器 (核心优化：避免 O(N^2))
        let mut current_char_idx: usize = 0;

        // 预计算可视范围的上下界，留一点 buffer 防止边缘闪烁
        let view_min_y = clip_rect.min.y - 50.0;
        let view_max_y = clip_rect.max.y + 50.0;

        // 遍历所有逻辑行
        for (line_idx, line) in content.split_inclusive('\n').enumerate() {
            // 1. 获取当前行在 Galley 中的位置
            // egui 的 cursor 是基于 char index 的
            let cursor = egui::text::CCursor::new(current_char_idx);
            let rect_in_galley = galley.pos_from_cursor(cursor);

            // 转换为屏幕绝对坐标
            // rect_in_galley.center().y 是相对于文本开头的偏移
            // text_offset.y 是文本开头在屏幕上的 Y 坐标（滚动会改变这个值）
            let line_center_y = text_offset.y + rect_in_galley.center().y;

            // 2. 更新累加器 (为下一次循环做准备)
            // 必须在 continue/break 之前计算好这一行的长度
            let char_count = line.chars().count();
            current_char_idx += char_count;

            // 3. 视锥剔除 (Culling) - 性能优化的关键
            if line_center_y < view_min_y {
                continue; // 在屏幕上方，跳过绘制
            }
            if line_center_y > view_max_y {
                break; // 在屏幕下方，剩下的都不用看了，直接退出循环！
            }

            // 4. 计算绘制中心点
            // sidebar_rect.center().x 是侧边栏的中心 X
            let center = Pos2::new(sidebar_rect.center().x, line_center_y);

            // 5. 点击检测 (Hit Test)
            // 如果刚刚发生了点击，并且点击位置在当前行附近
            if response.clicked()
                && let Some(pos) = pointer_pos
            {
                // 简单的距离检测，高度的一半作为判定范围
                let half_height = rect_in_galley.height() / 2.0;
                if (pos.y - line_center_y).abs() <= half_height {
                    clicked_line_index = Some(line_idx);
                }
            }

            // 6. 绘制 UI
            // 绘制提示小圆圈
            painter.circle_stroke(
                center,
                2.5,
                egui::Stroke::new(1.0, ui.visuals().text_color().gamma_multiply(0.3)),
            );

            // 如果有标记，绘制实心圆
            if self.marks.contains_key(&line_idx) {
                painter.circle_filled(center, 4.0, Color32::from_rgb(200, 100, 100));
            }
        }

        // 处理点击事件逻辑
        if let Some(idx) = clicked_line_index {
            if let std::collections::hash_map::Entry::Vacant(e) = self.marks.entry(idx) {
                e.insert(Mark::default());
                self.popup_mark = Some(idx);
                self.marks_changed = true;
            } else if self.popup_mark == Some(idx) {
                self.popup_mark = None;
            } else {
                self.popup_mark = Some(idx);
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
                        egui::RichText::new(format!("{} words", words_before)).size(11.0),
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
