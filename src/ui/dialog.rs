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
    InstallUpdate,
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
    let outer_width = config.width.min((screen_rect.width() - 32.0).max(220.0));
    let panel_fill = ctx.style().visuals.panel_fill;
    let text_color = ctx.style().visuals.text_color();
    let modal_frame = egui::Frame::window(&ctx.style())
        .fill(panel_fill)
        .stroke(egui::Stroke::new(1.0_f32, text_color.gamma_multiply(0.18)))
        .corner_radius(egui::CornerRadius::ZERO)
        .inner_margin(egui::Margin::ZERO)
        .shadow(egui::epaint::Shadow::NONE);
    // `Window::fixed_size` sizes the content, excluding the frame's margin and border.
    let content_width = outer_width
        - f32::from(modal_frame.inner_margin.left + modal_frame.inner_margin.right)
        - modal_frame.stroke.width * 2.0;

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
        .fade_in(false)
        .fade_out(false)
        .fixed_size(egui::vec2(content_width, 0.0))
        .collapsible(false)
        .resizable(false)
        .movable(false)
        .title_bar(false)
        .frame(modal_frame)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                // Compact rhythm is explicit below; do not add the global 8 px gap between every row.
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                ui.spacing_mut().interact_size.y = 20.0;
                modal_content_inset(ui, 8, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(config.icon)
                                .size(14.0)
                                .color(config.icon_color),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(config.title)
                                .strong()
                                .size(14.0)
                                .color(text_color.gamma_multiply(0.82)),
                        );
                    });
                });
                modal_separator(ui);
                ui.add_space(4.0);

                modal_content_inset(ui, 12, |ui| {
                    ui.label(
                        RichText::new(config.message)
                            .size(13.0)
                            .color(text_color.gamma_multiply(0.65)),
                    );
                });

                ui.add_space(6.0);
                modal_separator(ui);
                inner = Some(modal_content_inset(ui, 8, add_buttons));
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

/// Text and actions align with the main UI; separators remain full-bleed.
fn modal_content_inset<R>(
    ui: &mut Ui,
    horizontal: i8,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(horizontal, 0))
        .show(ui, add_contents)
        .inner
}

/// A hairline rule matches the window chrome without introducing heavy card sections.
fn modal_separator(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    let color = ui.visuals().text_color().gamma_multiply(0.18);
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0_f32, color),
    );
}

/// Quiet, frameless actions match the editor's existing controls while retaining a generous target.
pub fn modal_button(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(
        egui::Button::new(text)
            .frame(false)
            .min_size(egui::vec2(0.0, 20.0)),
    )
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
            icon_color: Color32::from_rgb(173, 120, 24),
            message: "当前文章有未保存的内容，是否先保存？\n选择“放弃修改”将丢失这些内容。",
            width: 320.0,
        };

        let mut clicked_decision = None;

        let response = show_modal(ctx, config, |ui| {
            ui.horizontal(|ui| {
                let discard_clicked = modal_button(
                    ui,
                    RichText::new("放弃修改").color(Color32::from_rgb(145, 64, 64)),
                )
                .on_hover_text("放弃未保存的内容")
                .clicked();

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_clicked = modal_button(
                        ui,
                        RichText::new(format!("{} 保存", egui_phosphor::regular::CHECK)).strong(),
                    )
                    .clicked();

                    let cancel_clicked = modal_button(ui, "取消").clicked();

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

    fn any_rect<F>(shape: &egui::Shape, predicate: &F) -> bool
    where
        F: Fn(&egui::epaint::RectShape) -> bool,
    {
        match shape {
            egui::Shape::Rect(rect) => predicate(rect),
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| any_rect(shape, predicate)),
            _ => false,
        }
    }

    fn output_has_rect<F>(output: &egui::FullOutput, predicate: F) -> bool
    where
        F: Fn(&egui::epaint::RectShape) -> bool,
    {
        output
            .shapes
            .iter()
            .any(|shape| any_rect(&shape.shape, &predicate))
    }

    fn hairline_widths(shape: &egui::Shape, expected_stroke: egui::Stroke) -> Vec<f32> {
        match shape {
            egui::Shape::LineSegment { points, stroke }
                if points[0].y == points[1].y && *stroke == expected_stroke =>
            {
                vec![(points[1].x - points[0].x).abs()]
            }
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .flat_map(|shape| hairline_widths(shape, expected_stroke))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn render_unsaved_dialog(viewport_width: f32) -> (egui::Context, egui::FullOutput) {
        let ctx = egui::Context::default();
        crate::style::configure_style(&ctx);
        let mut dialog = UnsavedChangesDialog::new();
        dialog.open(PendingDocumentAction::CloseWindow);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(viewport_width, 600.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input.clone(), |ctx| {
            assert_eq!(dialog.show(ctx), None);
        });
        let output = ctx.run(input, |ctx| {
            assert_eq!(dialog.show(ctx), None);
        });
        (ctx, output)
    }

    #[test]
    fn unsaved_dialog_uses_app_palette_without_default_button_chrome() {
        let (ctx, output) = render_unsaved_dialog(900.0);
        let panel_fill = ctx.style().visuals.panel_fill;
        let border = egui::Stroke::new(
            1.0_f32,
            ctx.style().visuals.text_color().gamma_multiply(0.18),
        );

        assert!(
            output_has_rect(&output, |rect| rect.fill == panel_fill
                && rect.stroke == border),
            "modal should use the same surface and hairline vocabulary as the app"
        );
        assert!(
            !output_has_rect(&output, |rect| {
                rect.fill.a() > 0
                    && rect.rect.width() < 200.0
                    && (20.0..=32.0).contains(&rect.rect.height())
            }),
            "resting modal actions must not paint generic gray button backgrounds"
        );
    }

    #[test]
    fn unsaved_dialog_stays_compact_with_full_width_hairline_rules() {
        let (ctx, output) = render_unsaved_dialog(900.0);
        let modal_rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("unsaved_dialog_modal_window")))
            .expect("unsaved dialog should be rendered");
        let rule_stroke = egui::Stroke::new(
            1.0_f32,
            ctx.style().visuals.text_color().gamma_multiply(0.18),
        );
        let rule_widths = output
            .shapes
            .iter()
            .flat_map(|shape| hairline_widths(&shape.shape, rule_stroke))
            .collect::<Vec<_>>();

        assert_eq!(modal_rect.width(), 320.0, "dialog should stay compact");
        assert!(
            modal_rect.height() <= 100.0,
            "dialog is too tall for the compact UI: {modal_rect:?}"
        );
        assert_eq!(
            rule_widths.len(),
            2,
            "header and action area should each have one hairline rule"
        );
        assert!(
            rule_widths
                .iter()
                .all(|width| (*width - (modal_rect.width() - 2.0)).abs() <= 1.0),
            "rules must span the full dialog instead of stopping at the content inset: {rule_widths:?}"
        );
    }

    #[test]
    fn unsaved_dialog_stays_inside_narrow_viewports() {
        for viewport_width in [300.0, 360.0, 900.0] {
            let (ctx, _) = render_unsaved_dialog(viewport_width);
            let modal_rect = ctx
                .memory(|memory| memory.area_rect(egui::Id::new("unsaved_dialog_modal_window")))
                .expect("unsaved dialog should be rendered");

            assert!(modal_rect.left() >= 0.0, "dialog crosses left edge");
            assert!(
                modal_rect.right() <= viewport_width,
                "dialog crosses right edge at viewport width {viewport_width}"
            );
            assert!(
                modal_rect.width() <= viewport_width - 31.0,
                "dialog should retain breathing room at viewport width {viewport_width}"
            );
        }
    }

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
