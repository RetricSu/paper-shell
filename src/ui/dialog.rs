use egui::{Color32, Context, RichText, Ui};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentChangeDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingDocumentAction {
    OpenFile(PathBuf),
    NewDocument(Option<PathBuf>),
    CloseWindow,
    Rollback(String),
}

pub struct ModalConfig<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub icon: &'a str,
    pub icon_color: Color32,
    pub message: &'a str,
    pub width: f32,
}

pub struct ModalResponse<R> {
    pub inner: Option<R>,
    pub escape_pressed: bool,
    pub enter_pressed: bool,
}

/// Renders a centered modal dialog with Paper Shell styling and background dimming scrim.
/// Consumes `Escape` and `Enter` shortcuts so they do not leak to background widgets.
pub fn show_modal<R>(
    ctx: &Context,
    config: ModalConfig<'_>,
    add_buttons: impl FnOnce(&mut Ui) -> R,
) -> ModalResponse<R> {
    let screen_rect = ctx.input(|i| i.content_rect());

    // 1. Modal backdrop scrim to dim background and capture clicks and drags
    egui::Area::new(egui::Id::new(format!("{}_scrim", config.id)))
        .order(egui::Order::Middle)
        .fixed_pos(screen_rect.min)
        .show(ctx, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(screen_rect.size(), egui::Sense::click_and_drag());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(45));
        });

    let mut inner = None;

    // 2. Centered modal card
    egui::Window::new(config.title)
        .id(egui::Id::new(format!("{}_modal_window", config.id)))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size(egui::vec2(config.width, 0.0))
        .collapsible(false)
        .resizable(false)
        .movable(false)
        .title_bar(false)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(Color32::WHITE)
                .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(215, 215, 215)))
                .corner_radius(egui::CornerRadius::ZERO)
                .inner_margin(egui::Margin::symmetric(24, 20))
                .shadow(egui::epaint::Shadow::NONE),
        )
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(config.icon)
                            .size(20.0)
                            .color(config.icon_color),
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(config.title).strong().size(15.0));
                });

                ui.add_space(12.0);

                ui.label(
                    RichText::new(config.message)
                        .size(13.0)
                        .color(Color32::from_gray(90)),
                );

                ui.add_space(20.0);

                inner = Some(add_buttons(ui));
            });
        });

    let escape_pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let enter_pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));

    ModalResponse {
        inner,
        escape_pressed,
        enter_pressed,
    }
}

#[derive(Default)]
pub struct UnsavedChangesDialog {
    open: bool,
    pending_action: Option<PendingDocumentAction>,
}

impl UnsavedChangesDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, action: PendingDocumentAction) {
        self.pending_action = Some(action);
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.pending_action = None;
    }

    pub fn pending_action(&self) -> Option<&PendingDocumentAction> {
        self.pending_action.as_ref()
    }

    /// Renders the modal confirmation dialog when open.
    /// Exclusively consumes keyboard shortcuts (Esc, Enter, Cmd/Ctrl+S, Cmd/Ctrl+D) so they cannot leak.
    pub fn show(&mut self, ctx: &Context) -> Option<DocumentChangeDecision> {
        if !self.open {
            return None;
        }

        let config = ModalConfig {
            id: "unsaved_dialog",
            title: "未保存的文章",
            icon: egui_phosphor::regular::WARNING,
            icon_color: Color32::from_rgb(217, 119, 6),
            message: "当前文章有未保存的内容，是否先保存？\n选择“放弃修改”将丢失这些内容。",
            width: 380.0,
        };

        let mut clicked_decision = None;

        let response = show_modal(ctx, config, |ui| {
            ui.horizontal(|ui| {
                let discard_clicked = ui
                    .button(RichText::new("放弃修改").color(Color32::from_rgb(200, 60, 60)))
                    .on_hover_text("放弃未保存的内容")
                    .clicked();

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_clicked = ui
                        .button(
                            RichText::new(format!("{} 保存", egui_phosphor::regular::CHECK))
                                .strong(),
                        )
                        .clicked();

                    let cancel_clicked = ui.button("取消").clicked();

                    if save_clicked {
                        clicked_decision = Some(DocumentChangeDecision::Save);
                    } else if cancel_clicked {
                        clicked_decision = Some(DocumentChangeDecision::Cancel);
                    } else if discard_clicked {
                        clicked_decision = Some(DocumentChangeDecision::Discard);
                    }
                });
            });
        });

        if let Some(decision) = clicked_decision {
            Some(decision)
        } else if response.escape_pressed {
            Some(DocumentChangeDecision::Cancel)
        } else if response.enter_pressed
            || ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)
                    || i.consume_key(egui::Modifiers::CTRL, egui::Key::S)
            })
        {
            Some(DocumentChangeDecision::Save)
        } else if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::D)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::D)
        }) {
            Some(DocumentChangeDecision::Discard)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unsaved_dialog_state() {
        let mut dialog = UnsavedChangesDialog::new();
        assert!(!dialog.is_open());
        assert!(dialog.pending_action().is_none());

        dialog.open(PendingDocumentAction::CloseWindow);
        assert!(dialog.is_open());
        assert_eq!(
            dialog.pending_action(),
            Some(&PendingDocumentAction::CloseWindow)
        );

        dialog.close();
        assert!(!dialog.is_open());
        assert!(dialog.pending_action().is_none());

        dialog.open(PendingDocumentAction::OpenFile(PathBuf::from("/test.txt")));
        assert!(dialog.is_open());
        assert_eq!(
            dialog.pending_action(),
            Some(&PendingDocumentAction::OpenFile(PathBuf::from("/test.txt")))
        );

        dialog.open(PendingDocumentAction::Rollback("hash123".into()));
        assert_eq!(
            dialog.pending_action(),
            Some(&PendingDocumentAction::Rollback("hash123".into()))
        );
    }
}
