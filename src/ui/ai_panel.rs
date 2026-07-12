use crate::backend::ai_backend::{AiAgentResponse, AiChatMessage, AiToolCall};
use egui::{Align, Color32, FontId, Frame, Layout, RichText, Sense, UiBuilder};

const COMPOSER_HEIGHT: f32 = 112.0;
const PANEL_GAP: f32 = 8.0;

#[derive(Default)]
pub struct AiPanel {
    pub is_visible: bool,
    pub is_processing: bool,
    entries: Vec<AiPanelEntry>,
    draft_message: String,
    request_snapshot: Option<String>,
}

enum AiPanelEntry {
    Message(AiChatMessage),
    EditProposal(EditProposal),
    Mindmap(MindmapArtifact),
    ToolError { name: String, reason: String },
}

struct EditProposal {
    base_content: String,
    original_text: String,
    replacement_text: String,
    explanation: String,
    status: EditStatus,
}

enum EditStatus {
    Ready,
    Applied,
    Rejected,
    Failed(String),
}

struct MindmapArtifact {
    title: String,
    mermaid: String,
    show_source: bool,
}

#[derive(Debug, PartialEq)]
struct MindmapNode {
    depth: usize,
    label: String,
}

impl AiPanel {
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<AiPanelAction> {
        let mut action = None;
        let panel_frame = Frame::new()
            .fill(Color32::from_gray(247))
            .inner_margin(egui::Margin::same(8))
            .stroke(egui::Stroke::NONE);

        panel_frame.show(ui, |ui| {
            let full_rect = ui.available_rect_before_wrap();
            let composer_height = COMPOSER_HEIGHT.min(full_rect.height());
            let history_bottom =
                (full_rect.max.y - composer_height - PANEL_GAP).max(full_rect.min.y);
            let history_rect = egui::Rect::from_min_max(
                full_rect.min,
                egui::pos2(full_rect.max.x, history_bottom),
            );
            let composer_rect = egui::Rect::from_min_max(
                egui::pos2(full_rect.min.x, history_bottom + PANEL_GAP),
                full_rect.max,
            );

            let mut history_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt("ai_panel_history")
                    .max_rect(history_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            self.show_history(&mut history_ui, &mut action);

            let mut composer_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt("ai_panel_composer")
                    .max_rect(composer_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            if action.is_none() {
                action = self.show_composer(&mut composer_ui);
            }

            ui.advance_cursor_after_rect(full_rect);
        });

        action
    }

    fn show_history(&mut self, ui: &mut egui::Ui, action: &mut Option<AiPanelAction>) {
        egui::ScrollArea::vertical()
            .id_salt("ai_panel_history_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                if self.entries.is_empty() && !self.is_processing {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("写作伙伴")
                            .size(13.0)
                            .strong()
                            .color(Color32::from_gray(55)),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("可以一起讨论，也可以让我提出修改或生成 Mermaid 脑图。正文只有在你确认后才会改变。")
                            .size(12.0)
                            .color(Color32::from_gray(112)),
                    );
                }

                for (index, entry) in self.entries.iter_mut().enumerate() {
                    ui.add_space(6.0);
                    match entry {
                        AiPanelEntry::Message(message) => show_message(ui, message),
                        AiPanelEntry::EditProposal(proposal) => {
                            if action.is_none() {
                                *action = show_edit_proposal(ui, index, proposal);
                            } else {
                                let _ = show_edit_proposal(ui, index, proposal);
                            }
                        }
                        AiPanelEntry::Mindmap(artifact) => show_mindmap(ui, artifact),
                        AiPanelEntry::ToolError { name, reason } => {
                            show_tool_error(ui, name, reason)
                        }
                    }
                }

                if self.is_processing {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new("正在理解文档并选择工具…")
                                .size(12.0)
                                .color(Color32::from_gray(112)),
                        );
                    });
                }
            });
    }

    fn show_composer(&mut self, ui: &mut egui::Ui) -> Option<AiPanelAction> {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("给写作伙伴的消息")
                    .size(10.0)
                    .strong()
                    .color(Color32::from_gray(92)),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new("⌘/Ctrl + Enter")
                        .size(9.0)
                        .color(Color32::from_gray(145)),
                );
            });
        });

        let input = egui::TextEdit::multiline(&mut self.draft_message)
            .hint_text("讨论文本、提出修改，或生成脑图")
            .desired_rows(2)
            .font(egui::TextStyle::Small);
        let input_response = ui.add_sized([ui.available_width(), 48.0], input);
        let shortcut_pressed = input_response.has_focus()
            && ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));

        let mut should_send = shortcut_pressed;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.is_processing,
                    egui::Button::new(RichText::new("发送").size(11.0)),
                )
                .clicked()
            {
                should_send = true;
            }

            if ui
                .add_enabled(
                    !self.is_processing
                        && (!self.draft_message.is_empty() || !self.entries.is_empty()),
                    egui::Button::new(RichText::new("清空").size(11.0)),
                )
                .clicked()
            {
                self.draft_message.clear();
                self.entries.clear();
                self.request_snapshot = None;
            }
        });

        if should_send && !self.is_processing {
            let user_message = if self.draft_message.trim().is_empty() {
                "请根据当前文本，给我一个短的结对观察。".to_string()
            } else {
                self.draft_message.trim().to_string()
            };
            self.entries.push(AiPanelEntry::Message(AiChatMessage {
                role: "user".to_string(),
                content: user_message,
            }));
            self.draft_message.clear();
            let conversation = self
                .entries
                .iter()
                .filter_map(|entry| match entry {
                    AiPanelEntry::Message(message) => Some(message.clone()),
                    _ => None,
                })
                .collect();
            Some(AiPanelAction::SendRequest { conversation })
        } else {
            None
        }
    }

    pub fn begin_request(&mut self, editor_snapshot: String) {
        self.is_processing = true;
        self.request_snapshot = Some(editor_snapshot);
    }

    pub fn set_response(&mut self, response: AiAgentResponse) {
        let base_content = self.request_snapshot.take().unwrap_or_default();
        let summary = tool_summary(&response.tool_calls);
        let content = if response.content.trim().is_empty() {
            summary
        } else {
            response.content
        };

        for tool_call in response.tool_calls {
            match tool_call {
                AiToolCall::ProposeDocumentEdit {
                    original_text,
                    replacement_text,
                    explanation,
                } => self.entries.push(AiPanelEntry::EditProposal(EditProposal {
                    base_content: base_content.clone(),
                    original_text,
                    replacement_text,
                    explanation,
                    status: EditStatus::Ready,
                })),
                AiToolCall::CreateMermaidMindmap { title, mermaid } => {
                    self.entries.push(AiPanelEntry::Mindmap(MindmapArtifact {
                        title,
                        mermaid,
                        show_source: false,
                    }));
                }
                AiToolCall::Unsupported { name, reason } => {
                    self.entries.push(AiPanelEntry::ToolError { name, reason });
                }
            }
        }
        if !content.trim().is_empty() {
            self.entries.push(AiPanelEntry::Message(AiChatMessage {
                role: "assistant".to_string(),
                content,
            }));
        }
        self.is_processing = false;
    }

    pub fn set_error(&mut self, error: String) {
        self.entries.push(AiPanelEntry::Message(AiChatMessage {
            role: "assistant".to_string(),
            content: format!("请求失败：{}", error),
        }));
        self.is_processing = false;
        self.request_snapshot = None;
    }

    pub fn set_edit_result(&mut self, proposal_index: usize, result: Result<(), String>) {
        let Some(AiPanelEntry::EditProposal(proposal)) = self.entries.get_mut(proposal_index)
        else {
            return;
        };
        proposal.status = match result {
            Ok(()) => EditStatus::Applied,
            Err(error) => EditStatus::Failed(error),
        };
    }
}

fn show_message(ui: &mut egui::Ui, message: &AiChatMessage) {
    let is_user = message.role == "user";
    ui.label(
        RichText::new(if is_user { "你" } else { "AI" })
            .size(10.0)
            .strong()
            .color(Color32::from_gray(if is_user { 72 } else { 120 })),
    );
    Frame::new()
        .fill(if is_user {
            Color32::from_gray(238)
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(5.0)
        .inner_margin(egui::Margin::same(7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(&message.content)
                    .size(12.0)
                    .color(Color32::from_gray(48)),
            );
        });
}

fn show_edit_proposal(
    ui: &mut egui::Ui,
    index: usize,
    proposal: &mut EditProposal,
) -> Option<AiPanelAction> {
    let mut action = None;
    Frame::new()
        .fill(Color32::from_gray(243))
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(218)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("修改提案").size(11.0).strong());
            if !proposal.explanation.trim().is_empty() {
                ui.label(
                    RichText::new(&proposal.explanation)
                        .size(11.0)
                        .color(Color32::from_gray(92)),
                );
            }

            ui.add_space(4.0);
            ui.label(
                RichText::new("原文")
                    .size(9.0)
                    .strong()
                    .color(Color32::from_gray(112)),
            );
            ui.label(
                RichText::new(preview_text(&proposal.original_text))
                    .size(11.0)
                    .color(Color32::from_rgb(128, 62, 62)),
            );
            ui.label(
                RichText::new("替换为")
                    .size(9.0)
                    .strong()
                    .color(Color32::from_gray(112)),
            );
            ui.label(
                RichText::new(preview_text(&proposal.replacement_text))
                    .size(11.0)
                    .color(Color32::from_rgb(52, 104, 72)),
            );

            ui.add_space(4.0);
            match &proposal.status {
                EditStatus::Ready => {
                    ui.horizontal(|ui| {
                        if ui.button("应用修改").clicked() {
                            action = Some(AiPanelAction::ApplyEdit {
                                proposal_index: index,
                                base_content: proposal.base_content.clone(),
                                original_text: proposal.original_text.clone(),
                                replacement_text: proposal.replacement_text.clone(),
                            });
                        }
                        if ui.button("忽略").clicked() {
                            proposal.status = EditStatus::Rejected;
                        }
                    });
                }
                EditStatus::Applied => {
                    ui.label(RichText::new("已应用").size(10.0).strong());
                }
                EditStatus::Rejected => {
                    ui.label(
                        RichText::new("已忽略")
                            .size(10.0)
                            .color(Color32::from_gray(120)),
                    );
                }
                EditStatus::Failed(error) => {
                    ui.label(
                        RichText::new(format!("未应用：{}", error))
                            .size(10.0)
                            .color(Color32::from_rgb(136, 58, 58)),
                    );
                }
            }
        });
    action
}

fn show_mindmap(ui: &mut egui::Ui, artifact: &mut MindmapArtifact) {
    Frame::new()
        .fill(Color32::from_gray(243))
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(218)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mermaid 脑图").size(11.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("复制源码").clicked() {
                        ui.ctx().copy_text(artifact.mermaid.clone());
                    }
                    if ui
                        .button(if artifact.show_source {
                            "查看脑图"
                        } else {
                            "查看源码"
                        })
                        .clicked()
                    {
                        artifact.show_source = !artifact.show_source;
                    }
                });
            });
            ui.label(
                RichText::new(&artifact.title)
                    .size(10.0)
                    .color(Color32::from_gray(104)),
            );
            ui.add_space(5.0);
            if artifact.show_source {
                ui.add(
                    egui::Label::new(
                        RichText::new(preview_text(&artifact.mermaid))
                            .monospace()
                            .size(10.0)
                            .color(Color32::from_gray(64)),
                    )
                    .selectable(true)
                    .wrap(),
                );
            } else {
                paint_mindmap(ui, &parse_mindmap(&artifact.mermaid));
            }
        });
}

fn parse_mindmap(source: &str) -> Vec<MindmapNode> {
    let mut indent_stack = Vec::<usize>::new();
    let mut nodes = Vec::new();

    for line in source.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("%%")
            || trimmed.starts_with("::")
            || trimmed.starts_with("classDef ")
        {
            continue;
        }
        let indent = line
            .chars()
            .take_while(|character| character.is_whitespace())
            .count();
        let depth = match indent_stack.last().copied() {
            None => {
                indent_stack.push(indent);
                0
            }
            Some(last) if indent > last => {
                indent_stack.push(indent);
                indent_stack.len() - 1
            }
            Some(last) if indent == last => indent_stack.len() - 1,
            Some(_) => {
                while indent_stack.last().is_some_and(|level| *level > indent) {
                    indent_stack.pop();
                }
                if indent_stack.last().copied() != Some(indent) {
                    indent_stack.push(indent);
                }
                indent_stack.len() - 1
            }
        };

        nodes.push(MindmapNode {
            depth,
            label: mindmap_label(trimmed),
        });
        if nodes.len() == 48 {
            break;
        }
    }
    nodes
}

fn mindmap_label(raw: &str) -> String {
    let raw = raw.split(":::").next().unwrap_or(raw).trim();
    for (opening, closing) in [("((", "))"), ("{{", "}}"), ("[", "]"), ("(", ")")] {
        if let (Some(start), Some(end)) = (raw.find(opening), raw.rfind(closing)) {
            let start = start + opening.len();
            if start <= end {
                return raw[start..end]
                    .trim()
                    .trim_matches(['\"', '\''])
                    .to_string();
            }
        }
    }
    raw.trim_matches(['\"', '\'']).to_string()
}

fn paint_mindmap(ui: &mut egui::Ui, nodes: &[MindmapNode]) {
    if nodes.is_empty() {
        ui.label(
            RichText::new("脑图没有可显示的节点")
                .size(10.0)
                .color(Color32::from_gray(112)),
        );
        return;
    }

    const ROW_HEIGHT: f32 = 32.0;
    const NODE_HEIGHT: f32 = 24.0;
    const DEPTH_STEP: f32 = 18.0;
    let width = ui.available_width().max(80.0);
    let height = ROW_HEIGHT * nodes.len() as f32;
    let (response, painter) = ui.allocate_painter(egui::vec2(width, height), Sense::hover());
    let origin = response.rect.min;
    let mut parent_rows = Vec::<usize>::new();

    for (row, node) in nodes.iter().enumerate() {
        while parent_rows.len() <= node.depth {
            parent_rows.push(row);
        }
        parent_rows[node.depth] = row;
        parent_rows.truncate(node.depth + 1);

        let x = origin.x + (node.depth.min(7) as f32 * DEPTH_STEP);
        let y = origin.y + row as f32 * ROW_HEIGHT;
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, y),
            egui::vec2((origin.x + width - x).max(48.0), NODE_HEIGHT),
        );

        if node.depth > 0 {
            let parent_row = parent_rows[node.depth - 1];
            let parent_x = origin.x + ((node.depth - 1).min(7) as f32 * DEPTH_STEP);
            let parent_y = origin.y + parent_row as f32 * ROW_HEIGHT + NODE_HEIGHT;
            let joint_x = x - DEPTH_STEP * 0.5;
            painter.line_segment(
                [
                    egui::pos2(parent_x + 8.0, parent_y),
                    egui::pos2(joint_x, parent_y),
                ],
                egui::Stroke::new(1.0, Color32::from_gray(196)),
            );
            painter.line_segment(
                [
                    egui::pos2(joint_x, parent_y),
                    egui::pos2(joint_x, y + NODE_HEIGHT / 2.0),
                ],
                egui::Stroke::new(1.0, Color32::from_gray(196)),
            );
            painter.line_segment(
                [
                    egui::pos2(joint_x, y + NODE_HEIGHT / 2.0),
                    egui::pos2(x, y + NODE_HEIGHT / 2.0),
                ],
                egui::Stroke::new(1.0, Color32::from_gray(196)),
            );
        }

        let fill = if node.depth == 0 {
            Color32::from_rgb(224, 226, 222)
        } else if node.depth == 1 {
            Color32::from_rgb(235, 236, 232)
        } else {
            Color32::from_gray(241)
        };
        painter.rect(
            rect,
            4.0,
            fill,
            egui::Stroke::new(1.0, Color32::from_gray(211)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.left_center() + egui::vec2(7.0, 0.0),
            egui::Align2::LEFT_CENTER,
            elide_label(&node.label, rect.width()),
            FontId::proportional(if node.depth == 0 { 11.0 } else { 10.0 }),
            Color32::from_gray(52),
        );
    }
}

fn elide_label(label: &str, width: f32) -> String {
    let limit = ((width - 14.0) / 8.0).floor().max(4.0) as usize;
    if label.chars().count() <= limit {
        label.to_string()
    } else {
        format!(
            "{}…",
            label
                .chars()
                .take(limit.saturating_sub(1))
                .collect::<String>()
        )
    }
}

fn show_tool_error(ui: &mut egui::Ui, name: &str, reason: &str) {
    Frame::new()
        .fill(Color32::from_gray(243))
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(218)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(RichText::new("工具未执行").size(11.0).strong());
            ui.label(
                RichText::new(format!("{}：{}", name, reason))
                    .size(10.0)
                    .color(Color32::from_gray(96)),
            );
        });
}

fn tool_summary(tool_calls: &[AiToolCall]) -> String {
    if tool_calls
        .iter()
        .any(|call| matches!(call, AiToolCall::ProposeDocumentEdit { .. }))
    {
        "我准备了一个修改提案，请先查看差异。".to_string()
    } else if tool_calls
        .iter()
        .any(|call| matches!(call, AiToolCall::CreateMermaidMindmap { .. }))
    {
        "我根据当前文档生成了一份 Mermaid 脑图。".to_string()
    } else if !tool_calls.is_empty() {
        "模型请求了一个当前不可用的工具。".to_string()
    } else {
        String::new()
    }
}

fn preview_text(text: &str) -> String {
    const MAX_CHARS: usize = 900;
    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{}\n…", preview)
    } else {
        preview
    }
}

#[derive(Debug)]
pub enum AiPanelAction {
    SendRequest {
        conversation: Vec<AiChatMessage>,
    },
    ApplyEdit {
        proposal_index: usize,
        base_content: String,
        original_text: String,
        replacement_text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mermaid_mindmap_indentation_and_shapes() {
        let nodes = parse_mindmap(
            "mindmap\n  root((写作))\n    结构\n      a[论点]\n    证据\n      b{{案例}}",
        );

        assert_eq!(
            nodes,
            vec![
                MindmapNode {
                    depth: 0,
                    label: "写作".to_string(),
                },
                MindmapNode {
                    depth: 1,
                    label: "结构".to_string(),
                },
                MindmapNode {
                    depth: 2,
                    label: "论点".to_string(),
                },
                MindmapNode {
                    depth: 1,
                    label: "证据".to_string(),
                },
                MindmapNode {
                    depth: 2,
                    label: "案例".to_string(),
                },
            ]
        );
    }
}
