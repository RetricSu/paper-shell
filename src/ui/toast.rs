use egui::{Align2, Color32, Context, Frame, Id, Margin, Order, RichText, Stroke, Vec2, Visuals};
use std::time::{Duration, Instant};

/// Standard duration for floating toast messages.
pub const DEFAULT_TOAST_DURATION: Duration = Duration::from_millis(2200);

/// Fade-in ramp at the beginning of a toast's lifetime.
pub const FADE_IN_DURATION: Duration = Duration::from_millis(150);

/// Fade-out ramp before a toast expires.
pub const FADE_OUT_DURATION: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

impl ToastKind {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Success => egui_phosphor::regular::CHECK,
            Self::Info => egui_phosphor::regular::INFO,
            Self::Warning => egui_phosphor::regular::WARNING,
            Self::Error => egui_phosphor::regular::WARNING_CIRCLE,
        }
    }

    pub fn icon_color(&self, visuals: &Visuals) -> Color32 {
        match self {
            Self::Success => crate::style::STATUS_GREEN,
            Self::Info => visuals.text_color().gamma_multiply(0.75),
            Self::Warning => Color32::from_rgb(217, 143, 29),
            Self::Error => visuals.error_fg_color,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: Id,
    pub kind: ToastKind,
    pub message: String,
    pub duration: Duration,
    pub created_at: Instant,
    pub pause_start: Option<Instant>,
    pub paused_duration: Duration,
    #[cfg(test)]
    pub test_elapsed: Option<Duration>,
}

impl Toast {
    pub fn new(kind: ToastKind, message: impl Into<String>) -> Self {
        let message = message.into();
        let id = Id::new("toast").with(&message);
        Self {
            id,
            kind,
            message,
            duration: DEFAULT_TOAST_DURATION,
            created_at: Instant::now(),
            pause_start: None,
            paused_duration: Duration::ZERO,
            #[cfg(test)]
            test_elapsed: None,
        }
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn elapsed(&self) -> Duration {
        #[cfg(test)]
        if let Some(elapsed) = self.test_elapsed {
            return elapsed;
        }

        let total = self.created_at.elapsed();
        let current_pause = self.pause_start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);
        total.saturating_sub(self.paused_duration + current_pause)
    }

    #[cfg(test)]
    pub fn set_test_elapsed(&mut self, elapsed: Duration) {
        self.test_elapsed = Some(elapsed);
    }

    pub fn alpha(&self) -> f32 {
        let elapsed = self.elapsed();
        if elapsed >= self.duration {
            return 0.0;
        }

        let fade_in = if elapsed < FADE_IN_DURATION {
            (elapsed.as_secs_f32() / FADE_IN_DURATION.as_secs_f32()).clamp(0.15, 1.0)
        } else {
            1.0
        };

        let remaining = self.duration.saturating_sub(elapsed);
        let fade_out = if remaining < FADE_OUT_DURATION {
            (remaining.as_secs_f32() / FADE_OUT_DURATION.as_secs_f32()).clamp(0.0, 1.0)
        } else {
            1.0
        };

        (fade_in * fade_out).clamp(0.0, 1.0)
    }

    pub fn is_expired(&self) -> bool {
        self.elapsed() >= self.duration
    }
}

/// Manages floating in-app toast notifications.
///
/// Follows Paper Shell's design language:
/// - Minimal, compact, clean styling without decorative shadows
/// - Square corners (`CornerRadius::ZERO`) matching the app chrome
/// - Subtle hairline border matching the active skin
/// - Auto-fades out and pauses on mouse hover
#[derive(Debug, Default)]
pub struct ToastManager {
    toasts: Vec<Toast>,
    anchor: Option<(Align2, Vec2)>,
}

impl ToastManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_anchor(&mut self, align: Align2, offset: Vec2) {
        self.anchor = Some((align, offset));
    }

    pub fn show_toast(&mut self, toast: Toast) {
        if let Some(existing) = self
            .toasts
            .iter_mut()
            .find(|t| t.kind == toast.kind && t.message == toast.message)
        {
            existing.created_at = Instant::now();
            existing.pause_start = None;
            existing.paused_duration = Duration::ZERO;
            existing.duration = toast.duration;
            #[cfg(test)]
            {
                existing.test_elapsed = None;
            }
        } else {
            self.toasts.push(toast);
        }
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.show_toast(Toast::new(ToastKind::Success, message));
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.show_toast(Toast::new(ToastKind::Info, message));
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.show_toast(Toast::new(ToastKind::Warning, message));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.show_toast(Toast::new(ToastKind::Error, message));
    }

    pub fn clear(&mut self) {
        self.toasts.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    pub fn count(&self) -> usize {
        self.toasts.len()
    }

    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    pub fn toasts_mut(&mut self) -> &mut [Toast] {
        &mut self.toasts
    }

    /// Renders active toasts. Place at the end of the update pass.
    pub fn show(&mut self, ctx: &Context) {
        if self.toasts.is_empty() {
            return;
        }

        let (anchor_align, anchor_offset) = self
            .anchor
            .unwrap_or((Align2::CENTER_TOP, egui::vec2(0.0, 36.0)));

        let mut dismissed_indices = Vec::new();

        egui::Area::new(Id::new("paper_shell_toast_area"))
            .order(Order::Foreground)
            .anchor(anchor_align, anchor_offset)
            .interactable(true)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);

                for (idx, toast) in self.toasts.iter_mut().enumerate() {
                    if toast.is_expired() {
                        dismissed_indices.push(idx);
                        continue;
                    }

                    let alpha = toast.alpha();
                    let visuals = ui.visuals();
                    let panel_fill = visuals.panel_fill;
                    let text_color = visuals.text_color();
                    let stroke_color = text_color.gamma_multiply(0.18 * alpha);

                    let frame = Frame::window(ui.style())
                        .fill(panel_fill.gamma_multiply(alpha.max(0.05)))
                        .stroke(Stroke::new(1.0_f32, stroke_color))
                        .corner_radius(egui::CornerRadius::ZERO)
                        .inner_margin(Margin::symmetric(10, 5))
                        .shadow(egui::epaint::Shadow::NONE);

                    let frame_res = frame.show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                        ui.spacing_mut().interact_size.y = 18.0;
                        ui.horizontal(|ui| {
                            let icon_color =
                                toast.kind.icon_color(ui.visuals()).gamma_multiply(alpha);
                            ui.label(
                                RichText::new(toast.kind.icon())
                                    .size(13.0)
                                    .color(icon_color),
                            );
                            ui.label(
                                RichText::new(&toast.message)
                                    .size(12.0)
                                    .color(text_color.gamma_multiply(0.85 * alpha)),
                            );
                        });
                    });

                    let interact_response = ui.interact(
                        frame_res.response.rect,
                        toast.id,
                        egui::Sense::click(),
                    );

                    if interact_response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if toast.pause_start.is_none() {
                            toast.pause_start = Some(Instant::now());
                        }
                    } else if let Some(start) = toast.pause_start.take() {
                        toast.paused_duration += start.elapsed();
                    }

                    if interact_response.clicked() {
                        dismissed_indices.push(idx);
                    }
                }
            });

        // Remove dismissed toasts in reverse index order
        dismissed_indices.sort_unstable();
        dismissed_indices.dedup();
        for &idx in dismissed_indices.iter().rev() {
            if idx < self.toasts.len() {
                self.toasts.remove(idx);
            }
        }

        // Keep repainting while active toasts exist to drive smooth fade animations
        if !self.toasts.is_empty() {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_kind_properties() {
        let visuals = Visuals::light();
        assert_eq!(ToastKind::Success.icon(), egui_phosphor::regular::CHECK);
        assert_eq!(
            ToastKind::Success.icon_color(&visuals),
            crate::style::STATUS_GREEN
        );
        assert_eq!(ToastKind::Info.icon(), egui_phosphor::regular::INFO);
        assert_eq!(ToastKind::Warning.icon(), egui_phosphor::regular::WARNING);
        assert_eq!(
            ToastKind::Error.icon(),
            egui_phosphor::regular::WARNING_CIRCLE
        );
    }

    #[test]
    fn test_toast_alpha_fade() {
        let mut toast = Toast::new(ToastKind::Success, "保存成功")
            .with_duration(Duration::from_millis(2000));

        // Start: alpha >= 0.15 and ramping up
        toast.set_test_elapsed(Duration::from_millis(0));
        assert!(toast.alpha() >= 0.15 && toast.alpha() <= 0.2);

        // Middle: fully opaque
        toast.set_test_elapsed(Duration::from_millis(1000));
        assert_eq!(toast.alpha(), 1.0);
        assert!(!toast.is_expired());

        // Near end: fading out
        toast.set_test_elapsed(Duration::from_millis(1850));
        let fade_alpha = toast.alpha();
        assert!(fade_alpha > 0.0 && fade_alpha < 1.0);

        // Expired: zero alpha and is_expired
        toast.set_test_elapsed(Duration::from_millis(2000));
        assert_eq!(toast.alpha(), 0.0);
        assert!(toast.is_expired());
    }

    #[test]
    fn test_toast_manager_deduplication() {
        let mut manager = ToastManager::new();
        assert!(manager.is_empty());

        manager.success("保存成功");
        assert_eq!(manager.count(), 1);

        // Spanning the exact same message should refresh the timer rather than duplicating
        manager.success("保存成功");
        assert_eq!(manager.count(), 1);

        // A different message or kind will be added as a separate toast
        manager.info("其他提示");
        assert_eq!(manager.count(), 2);
    }

    fn has_text(shape: &egui::Shape, needle: &str) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text().contains(needle),
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has_text(shape, needle)),
            _ => false,
        }
    }

    fn text_position(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        output.shapes.iter().find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text().contains(needle)
            {
                Some(text.pos + text.galley.size() * 0.5)
            } else {
                None
            }
        })
    }

    #[test]
    fn test_toast_manager_click_to_dismiss() {
        let ctx = Context::default();
        crate::style::configure_style(&ctx);

        let mut manager = ToastManager::new();
        manager.success("保存成功");

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };

        // Frame 1 sizes the Area
        let _ = ctx.run(input.clone(), |ctx| {
            manager.show(ctx);
        });
        // Frame 2 paints the anchored Area
        let output = ctx.run(input.clone(), |ctx| {
            manager.show(ctx);
        });

        let pos = text_position(&output, "保存成功").expect("Toast text position should exist");
        let click_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };

        // Clicking the toast dismisses it
        let _ = ctx.run(click_input, |ctx| {
            manager.show(ctx);
        });

        assert!(manager.is_empty(), "Clicked toast should be dismissed");
    }

    #[test]
    fn test_toast_manager_renders_without_panic() {
        let ctx = Context::default();
        crate::style::configure_style(&ctx);

        let mut manager = ToastManager::new();
        manager.success("保存成功");

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };

        // Frame 1 sizes the Area
        let _ = ctx.run(input.clone(), |ctx| {
            manager.show(ctx);
        });
        // Frame 2 paints the anchored Area
        let output = ctx.run(input, |ctx| {
            manager.show(ctx);
        });

        // Ensure painting produced text output
        let found_saved = output
            .shapes
            .iter()
            .any(|clipped| has_text(&clipped.shape, "保存成功"));
        assert!(found_saved, "Expected '保存成功' in output shapes");
    }

    #[test]
    fn test_toast_manager_removes_expired_toasts() {
        let ctx = Context::default();
        crate::style::configure_style(&ctx);

        let mut manager = ToastManager::new();
        manager.success("保存成功");
        manager.toasts_mut()[0].set_test_elapsed(Duration::from_millis(3000));

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            manager.show(ctx);
        });

        assert!(manager.is_empty(), "Expired toast should be removed");
    }
}
