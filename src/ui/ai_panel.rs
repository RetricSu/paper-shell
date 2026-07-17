use crate::backend::ai_backend::{
    AiAgentResponse, AiChatMessage, AiError, AiProgressEvent, AiRequestId, AiSelectionContext,
    AiToolCall,
};
use egui::{Align, Color32, FontId, Frame, Layout, RichText, Sense, UiBuilder};

const COMPOSER_HEIGHT: f32 = 112.0;
const COMPOSER_HEIGHT_WITH_CONTEXT: f32 = 148.0;
const PANEL_GAP: f32 = 8.0;

#[derive(Default)]
pub struct AiPanel {
    pub is_visible: bool,
    pub is_processing: bool,
    entries: Vec<AiPanelEntry>,
    draft_message: String,
    request_snapshot: Option<String>,
    active_edit_proposal: Option<usize>,
    active_request_id: Option<AiRequestId>,
    progress_stage: String,
    partial_response: String,
    searched_chunks: usize,
    read_chunks: usize,
    composer_selection: Option<AiSelectionContext>,
    request_selection: Option<AiSelectionContext>,
    last_request: Option<PendingRequest>,
    last_error: Option<PanelError>,
}

enum AiPanelEntry {
    Message(PanelMessage),
    EditProposal(EditProposal),
    Mindmap(MindmapArtifact),
    ToolError { name: String, reason: String },
}

#[derive(Clone)]
struct PanelMessage {
    chat: AiChatMessage,
    selection: Option<AiSelectionContext>,
}

#[derive(Clone)]
struct PendingRequest {
    conversation: Vec<AiChatMessage>,
    selection: Option<AiSelectionContext>,
}

struct PanelError {
    message: String,
    retryable: bool,
}

struct EditProposal {
    base_content: String,
    original_text: String,
    replacement_text: String,
    explanation: String,
    status: EditStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiEditPreview {
    pub proposal_index: usize,
    pub base_content: String,
    pub original_text: String,
    pub replacement_text: String,
    pub explanation: String,
    pub review_position: usize,
    pub review_total: usize,
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
            let requested_composer_height = if self.composer_selection.is_some() {
                COMPOSER_HEIGHT_WITH_CONTEXT
            } else {
                COMPOSER_HEIGHT
            };
            let composer_height = requested_composer_height.min(full_rect.height());
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
                        RichText::new(
                            "讨论选区或全文，也可以提出修改、生成脑图。正文只在你确认后改变。",
                        )
                        .size(12.0)
                        .color(Color32::from_gray(112)),
                    );
                }

                for (index, entry) in self.entries.iter_mut().enumerate() {
                    ui.add_space(6.0);
                    match entry {
                        AiPanelEntry::Message(message) => show_message(ui, message),
                        AiPanelEntry::EditProposal(proposal) => {
                            let is_active = self.active_edit_proposal == Some(index);
                            if action.is_none() {
                                *action = show_edit_proposal(ui, index, proposal, is_active);
                            } else {
                                let _ = show_edit_proposal(ui, index, proposal, is_active);
                            }
                        }
                        AiPanelEntry::Mindmap(artifact) => show_mindmap(ui, artifact),
                        AiPanelEntry::ToolError { name, reason } => {
                            show_tool_error(ui, name, reason)
                        }
                    }
                }

                if !self.partial_response.trim().is_empty() {
                    ui.add_space(6.0);
                    show_streaming_message(ui, &self.partial_response);
                }

                if let Some(error) = &self.last_error {
                    ui.add_space(8.0);
                    let should_retry = show_request_error(ui, error);
                    if should_retry
                        && action.is_none()
                        && let Some(last_request) = &self.last_request
                    {
                        *action = Some(AiPanelAction::SendRequest {
                            conversation: last_request.conversation.clone(),
                            selection: last_request.selection.clone(),
                        });
                    }
                }

                if self.is_processing {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(if self.progress_stage.is_empty() {
                                "正在准备请求…"
                            } else {
                                &self.progress_stage
                            })
                            .size(12.0)
                            .color(Color32::from_gray(112)),
                        );
                    });
                    if self.searched_chunks > 0 || self.read_chunks > 0 {
                        ui.label(
                            RichText::new(format!(
                                "已检索 {} 处 · 读取 {} 段",
                                self.searched_chunks, self.read_chunks
                            ))
                            .size(10.0)
                            .color(Color32::from_gray(104)),
                        );
                    }
                }
            });
    }

    fn show_composer(&mut self, ui: &mut egui::Ui) -> Option<AiPanelAction> {
        ui.separator();
        if let Some(selection) = self.composer_selection.clone() {
            Frame::new()
                .fill(Color32::from_rgb(237, 239, 235))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(207, 211, 203)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(7, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("当前选区")
                                .size(9.0)
                                .strong()
                                .color(Color32::from_rgb(64, 72, 61)),
                        );
                        ui.label(
                            RichText::new(preview_inline(&selection.text, 36))
                                .size(9.0)
                                .color(Color32::from_gray(92)),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .small_button("×")
                                .on_hover_text("移除选区上下文")
                                .clicked()
                            {
                                self.composer_selection = None;
                            }
                        });
                    });
                });
            ui.add_space(4.0);
        }
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
            && !self.is_processing
            && ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));

        let mut should_send = shortcut_pressed;
        let mut should_stop = false;
        ui.horizontal(|ui| {
            if self.is_processing {
                if ui
                    .button(RichText::new("停止").size(11.0))
                    .on_hover_text("停止当前生成，已返回的文字会保留")
                    .clicked()
                {
                    should_stop = true;
                }
            } else if ui.button(RichText::new("发送").size(11.0)).clicked() {
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
                self.active_edit_proposal = None;
                self.composer_selection = None;
                self.last_request = None;
                self.last_error = None;
                self.partial_response.clear();
            }
        });

        if should_stop {
            self.active_request_id
                .map(|request_id| AiPanelAction::CancelRequest { request_id })
        } else if should_send && !self.is_processing {
            let user_message = if self.draft_message.trim().is_empty() {
                if self.composer_selection.is_some() {
                    "请围绕这个选区，给我一个最有用的观察。".to_string()
                } else {
                    "请检索当前文档，给我一个短的结对观察。".to_string()
                }
            } else {
                self.draft_message.trim().to_string()
            };
            self.draft_message.clear();
            Some(self.queue_user_message(user_message, self.composer_selection.clone()))
        } else {
            None
        }
    }

    fn queue_user_message(
        &mut self,
        content: String,
        selection: Option<AiSelectionContext>,
    ) -> AiPanelAction {
        self.entries.push(AiPanelEntry::Message(PanelMessage {
            chat: AiChatMessage {
                role: "user".to_string(),
                content,
            },
            selection: selection.clone(),
        }));
        let conversation = self.conversation_for(selection.as_ref());
        self.last_request = Some(PendingRequest {
            conversation: conversation.clone(),
            selection: selection.clone(),
        });
        self.last_error = None;
        AiPanelAction::SendRequest {
            conversation,
            selection,
        }
    }

    fn conversation_for(&self, selection: Option<&AiSelectionContext>) -> Vec<AiChatMessage> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                AiPanelEntry::Message(message)
                    if same_selection(message.selection.as_ref(), selection) =>
                {
                    Some(message.chat.clone())
                }
                _ => None,
            })
            .collect()
    }

    pub fn attach_selection(&mut self, selection: AiSelectionContext) {
        self.composer_selection = Some(selection);
    }

    pub fn detach_selection(&mut self, anchor_id: u64) {
        if self
            .composer_selection
            .as_ref()
            .is_some_and(|selection| selection.anchor_id == anchor_id)
        {
            self.composer_selection = None;
        }
    }

    pub fn send_selection_message(
        &mut self,
        content: String,
        selection: AiSelectionContext,
    ) -> Option<AiPanelAction> {
        if self.is_processing || content.trim().is_empty() {
            return None;
        }
        self.composer_selection = Some(selection.clone());
        Some(self.queue_user_message(content.trim().to_string(), Some(selection)))
    }

    pub fn selection_messages(&self, anchor_id: u64) -> Vec<AiChatMessage> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                AiPanelEntry::Message(message)
                    if message
                        .selection
                        .as_ref()
                        .is_some_and(|selection| selection.anchor_id == anchor_id) =>
                {
                    Some(message.chat.clone())
                }
                _ => None,
            })
            .collect()
    }

    pub fn request_status_for(&self, anchor_id: u64) -> Option<String> {
        if self.is_processing
            && self
                .request_selection
                .as_ref()
                .is_some_and(|selection| selection.anchor_id == anchor_id)
        {
            let mut status = if self.progress_stage.is_empty() {
                "正在准备请求…".to_string()
            } else {
                self.progress_stage.clone()
            };
            if self.searched_chunks > 0 || self.read_chunks > 0 {
                status.push_str(&format!(
                    "  已检索 {} 处 · 读取 {} 段",
                    self.searched_chunks, self.read_chunks
                ));
            }
            Some(status)
        } else {
            None
        }
    }

    pub fn partial_response_for(&self, anchor_id: u64) -> Option<&str> {
        (self.is_processing
            && !self.partial_response.is_empty()
            && self
                .request_selection
                .as_ref()
                .is_some_and(|selection| selection.anchor_id == anchor_id))
        .then_some(self.partial_response.as_str())
    }

    pub fn is_processing_for(&self, anchor_id: u64) -> bool {
        self.is_processing
            && self
                .request_selection
                .as_ref()
                .is_some_and(|selection| selection.anchor_id == anchor_id)
    }

    pub fn active_request_id(&self) -> Option<AiRequestId> {
        self.active_request_id
    }

    pub fn begin_request(
        &mut self,
        request_id: AiRequestId,
        editor_snapshot: String,
        selection: Option<AiSelectionContext>,
    ) {
        self.is_processing = true;
        self.request_snapshot = Some(editor_snapshot);
        self.request_selection = selection;
        self.active_request_id = Some(request_id);
        self.progress_stage = "正在准备请求…".to_string();
        self.partial_response.clear();
        self.searched_chunks = 0;
        self.read_chunks = 0;
        self.last_error = None;
    }

    pub fn apply_progress(&mut self, request_id: AiRequestId, event: AiProgressEvent) {
        if self.active_request_id != Some(request_id) {
            return;
        }
        match event {
            AiProgressEvent::Stage(stage) => self.progress_stage = stage,
            AiProgressEvent::Delta(delta) => self.partial_response.push_str(&delta),
            AiProgressEvent::Retrieval {
                searched_chunks,
                read_chunks,
            } => {
                self.searched_chunks = searched_chunks;
                self.read_chunks = read_chunks;
            }
            AiProgressEvent::Retrying { attempt, reason } => {
                self.progress_stage = format!("第 {} 次重试：{}", attempt, reason);
            }
        }
    }

    pub fn set_response(&mut self, request_id: AiRequestId, response: AiAgentResponse) {
        if self.active_request_id != Some(request_id) {
            return;
        }
        let base_content = self.request_snapshot.take().unwrap_or_default();
        let response_selection = self.request_selection.take();
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
                } => {
                    let proposal_index = self.entries.len();
                    self.entries.push(AiPanelEntry::EditProposal(EditProposal {
                        base_content: base_content.clone(),
                        original_text,
                        replacement_text,
                        explanation,
                        status: EditStatus::Ready,
                    }));
                    if self.active_edit_proposal.is_none() {
                        self.active_edit_proposal = Some(proposal_index);
                    }
                }
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
            self.entries.push(AiPanelEntry::Message(PanelMessage {
                chat: AiChatMessage {
                    role: "assistant".to_string(),
                    content,
                },
                selection: response_selection,
            }));
        }
        self.is_processing = false;
        self.active_request_id = None;
        self.progress_stage.clear();
        self.partial_response.clear();
        self.last_error = None;
    }

    pub fn set_error(&mut self, request_id: AiRequestId, error: AiError) {
        if self.active_request_id != Some(request_id) {
            return;
        }
        if !self.partial_response.trim().is_empty() {
            self.entries.push(AiPanelEntry::Message(PanelMessage {
                chat: AiChatMessage {
                    role: "assistant".to_string(),
                    content: std::mem::take(&mut self.partial_response),
                },
                selection: self.request_selection.clone(),
            }));
        }
        if !matches!(error, AiError::Cancelled) {
            self.last_error = Some(PanelError {
                message: error.to_string(),
                retryable: matches!(error, AiError::ApiError(_)),
            });
        }
        self.is_processing = false;
        self.request_snapshot = None;
        self.request_selection = None;
        self.active_request_id = None;
        self.progress_stage.clear();
    }

    pub fn cancel_request(&mut self, request_id: AiRequestId) {
        if self.active_request_id != Some(request_id) {
            return;
        }
        if !self.partial_response.trim().is_empty() {
            self.entries.push(AiPanelEntry::Message(PanelMessage {
                chat: AiChatMessage {
                    role: "assistant".to_string(),
                    content: std::mem::take(&mut self.partial_response),
                },
                selection: self.request_selection.clone(),
            }));
        }
        self.is_processing = false;
        self.request_snapshot = None;
        self.request_selection = None;
        self.active_request_id = None;
        self.progress_stage.clear();
        self.last_error = None;
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
        if self.active_edit_proposal == Some(proposal_index) {
            self.active_edit_proposal = self.next_ready_edit_proposal(proposal_index);
        }
    }

    pub fn active_edit_preview(&self) -> Option<AiEditPreview> {
        let proposal_index = self.active_edit_proposal?;
        let Some(AiPanelEntry::EditProposal(proposal)) = self.entries.get(proposal_index) else {
            return None;
        };
        if !matches!(proposal.status, EditStatus::Ready) {
            return None;
        }

        let ready = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| match entry {
                AiPanelEntry::EditProposal(proposal)
                    if matches!(proposal.status, EditStatus::Ready) =>
                {
                    Some(index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let review_position = ready.iter().position(|index| *index == proposal_index)? + 1;
        Some(AiEditPreview {
            proposal_index,
            base_content: proposal.base_content.clone(),
            original_text: proposal.original_text.clone(),
            replacement_text: proposal.replacement_text.clone(),
            explanation: proposal.explanation.clone(),
            review_position,
            review_total: ready.len(),
        })
    }

    pub fn preview_edit(&mut self, proposal_index: usize) {
        if matches!(
            self.entries.get(proposal_index),
            Some(AiPanelEntry::EditProposal(EditProposal {
                status: EditStatus::Ready,
                ..
            }))
        ) {
            self.active_edit_proposal = Some(proposal_index);
        }
    }

    pub fn reject_edit(&mut self, proposal_index: usize) {
        let Some(AiPanelEntry::EditProposal(proposal)) = self.entries.get_mut(proposal_index)
        else {
            return;
        };
        if matches!(proposal.status, EditStatus::Ready) {
            proposal.status = EditStatus::Rejected;
        }
        if self.active_edit_proposal == Some(proposal_index) {
            self.active_edit_proposal = self.next_ready_edit_proposal(proposal_index);
        }
    }

    pub fn navigate_edit(&mut self, direction: i32) {
        let ready = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| match entry {
                AiPanelEntry::EditProposal(proposal)
                    if matches!(proposal.status, EditStatus::Ready) =>
                {
                    Some(index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if ready.is_empty() {
            self.active_edit_proposal = None;
            return;
        }
        let current = self
            .active_edit_proposal
            .and_then(|active| ready.iter().position(|index| *index == active))
            .unwrap_or(0);
        let next = if direction < 0 {
            current.checked_sub(1).unwrap_or(ready.len() - 1)
        } else {
            (current + 1) % ready.len()
        };
        self.active_edit_proposal = Some(ready[next]);
    }

    pub fn ready_edit_previews(&self) -> Vec<AiEditPreview> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(proposal_index, entry)| match entry {
                AiPanelEntry::EditProposal(proposal)
                    if matches!(proposal.status, EditStatus::Ready) =>
                {
                    Some((proposal_index, proposal))
                }
                _ => None,
            })
            .enumerate()
            .map(|(position, (proposal_index, proposal))| AiEditPreview {
                proposal_index,
                base_content: proposal.base_content.clone(),
                original_text: proposal.original_text.clone(),
                replacement_text: proposal.replacement_text.clone(),
                explanation: proposal.explanation.clone(),
                review_position: position + 1,
                review_total: self
                    .entries
                    .iter()
                    .filter(|entry| {
                        matches!(
                            entry,
                            AiPanelEntry::EditProposal(EditProposal {
                                status: EditStatus::Ready,
                                ..
                            })
                        )
                    })
                    .count(),
            })
            .collect()
    }

    pub fn reject_all_edits(&mut self) {
        for entry in &mut self.entries {
            if let AiPanelEntry::EditProposal(proposal) = entry
                && matches!(proposal.status, EditStatus::Ready)
            {
                proposal.status = EditStatus::Rejected;
            }
        }
        self.active_edit_proposal = None;
    }

    fn next_ready_edit_proposal(&self, after: usize) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .skip(after.saturating_add(1))
            .find_map(|(index, entry)| match entry {
                AiPanelEntry::EditProposal(proposal)
                    if matches!(proposal.status, EditStatus::Ready) =>
                {
                    Some(index)
                }
                _ => None,
            })
    }
}

fn same_selection(left: Option<&AiSelectionContext>, right: Option<&AiSelectionContext>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.anchor_id == right.anchor_id,
        (None, None) => true,
        _ => false,
    }
}

fn preview_inline(text: &str, limit: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let preview = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", preview)
    } else {
        preview
    }
}

fn show_message(ui: &mut egui::Ui, message: &PanelMessage) {
    let is_user = message.chat.role == "user";
    ui.label(
        RichText::new(match (is_user, message.selection.is_some()) {
            (true, true) => "你 · 当前选区",
            (false, true) => "AI · 当前选区",
            (true, false) => "你",
            (false, false) => "AI",
        })
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
                RichText::new(&message.chat.content)
                    .size(12.0)
                    .color(Color32::from_gray(48)),
            );
        });
}

fn show_streaming_message(ui: &mut egui::Ui, content: &str) {
    ui.label(
        RichText::new("AI · 正在回复")
            .size(10.0)
            .strong()
            .color(Color32::from_gray(104)),
    );
    ui.label(
        RichText::new(content)
            .size(12.0)
            .color(Color32::from_gray(48)),
    );
}

fn show_request_error(ui: &mut egui::Ui, error: &PanelError) -> bool {
    let mut retry = false;
    Frame::new()
        .fill(Color32::from_rgb(248, 237, 235))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(222, 194, 189)))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("这次请求没有完成")
                    .size(11.0)
                    .strong()
                    .color(Color32::from_rgb(105, 50, 45)),
            );
            ui.label(
                RichText::new(&error.message)
                    .size(10.0)
                    .color(Color32::from_rgb(112, 62, 57)),
            );
            if error.retryable && ui.button("重试请求").clicked() {
                retry = true;
            }
        });
    retry
}

fn show_edit_proposal(
    ui: &mut egui::Ui,
    index: usize,
    proposal: &mut EditProposal,
    is_active: bool,
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
            match &proposal.status {
                EditStatus::Ready => {
                    if is_active {
                        ui.label(
                            RichText::new("已在正文中展开差异")
                                .size(10.0)
                                .strong()
                                .color(Color32::from_gray(88)),
                        );
                    } else if ui.button("在正文中预览").clicked() {
                        action = Some(AiPanelAction::PreviewEdit {
                            proposal_index: index,
                        });
                    }
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
        selection: Option<AiSelectionContext>,
    },
    CancelRequest {
        request_id: AiRequestId,
    },
    ApplyEdit {
        proposal_index: usize,
        base_content: String,
        original_text: String,
        replacement_text: String,
    },
    PreviewEdit {
        proposal_index: usize,
    },
    RejectEdit {
        proposal_index: usize,
    },
    NavigateEdit {
        direction: i32,
    },
    ApplyAllEdits,
    RejectAllEdits,
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

    #[test]
    fn edit_proposals_are_previewed_in_the_document_one_at_a_time() {
        let mut panel = AiPanel::default();
        panel.begin_request(1, "原文一。原文二。".to_string(), None);
        panel.set_response(
            1,
            AiAgentResponse {
                content: String::new(),
                tool_calls: vec![
                    AiToolCall::ProposeDocumentEdit {
                        original_text: "原文一".to_string(),
                        replacement_text: "改文一".to_string(),
                        explanation: "第一处".to_string(),
                    },
                    AiToolCall::ProposeDocumentEdit {
                        original_text: "原文二".to_string(),
                        replacement_text: "改文二".to_string(),
                        explanation: "第二处".to_string(),
                    },
                ],
            },
        );

        let first = panel.active_edit_preview().unwrap();
        assert_eq!(first.proposal_index, 0);
        assert_eq!(first.explanation, "第一处");

        panel.reject_edit(first.proposal_index);

        let second = panel.active_edit_preview().unwrap();
        assert_eq!(second.proposal_index, 1);
        assert_eq!(second.explanation, "第二处");
    }

    #[test]
    fn selection_threads_keep_their_conversation_isolated() {
        let mut panel = AiPanel::default();
        let first = AiSelectionContext {
            anchor_id: 1,
            start_char: 0,
            end_char: 2,
            text: "第一".to_string(),
        };
        let second = AiSelectionContext {
            anchor_id: 2,
            start_char: 3,
            end_char: 5,
            text: "第二".to_string(),
        };
        let first_action = panel
            .send_selection_message("谈第一处".to_string(), first.clone())
            .unwrap();
        let second_action = panel
            .send_selection_message("谈第二处".to_string(), second.clone())
            .unwrap();

        assert!(matches!(
            first_action,
            AiPanelAction::SendRequest { conversation, .. }
                if conversation.len() == 1 && conversation[0].content == "谈第一处"
        ));
        assert!(matches!(
            second_action,
            AiPanelAction::SendRequest { conversation, .. }
                if conversation.len() == 1 && conversation[0].content == "谈第二处"
        ));
    }
}
