use egui::{Color32, Context, Stroke, Style, Visuals};
use std::sync::RwLock;

/// Shared accent for the sidebar state and the active paragraph indicator.
pub const STATUS_GREEN: Color32 = Color32::from_rgb(46, 151, 83);

static ACTIVE_SKIN: RwLock<Option<crate::skin::Skin>> = RwLock::new(None);

pub fn set_active_skin(skin: crate::skin::Skin) {
    if let Ok(mut lock) = ACTIVE_SKIN.write() {
        *lock = Some(skin);
    }
}

pub fn get_active_skin() -> Option<crate::skin::Skin> {
    ACTIVE_SKIN.read().ok().and_then(|lock| lock.clone())
}

pub fn configure_style(ctx: &Context) {
    if let Some(skin) = get_active_skin() {
        crate::skin::SkinRenderer::apply_to_context(ctx, &skin);
    } else {
        configure_default_style(ctx);
    }
}

pub fn configure_default_style(ctx: &Context) {
    let mut style = Style::default();

    // Elegant visual settings
    // We want a clean, white paper look.

    // Increase spacing for elegance
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(6.0, 3.0);
    style.spacing.window_margin = egui::Margin::same(12);

    ctx.set_style(style);

    let mut visuals = Visuals::light();
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;

    // Square edges throughout, including menus, text fields, and open controls.
    visuals.window_corner_radius = egui::CornerRadius::ZERO;
    visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
    }

    // Minimalist colors. Keep noninteractive.bg_stroke visible: egui also uses
    // it for widget separators and panel edges.
    visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.hovered.bg_fill = Color32::from_gray(240);
    visuals.widgets.active.bg_fill = Color32::from_gray(230);

    visuals.selection.bg_fill = Color32::from_rgb(200, 220, 255);
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(100, 100, 100));

    ctx.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_style_keeps_egui_separators_visible() {
        let ctx = Context::default();
        configure_style(&ctx);

        let separator_stroke = ctx.style().visuals.widgets.noninteractive.bg_stroke;
        assert!(separator_stroke.width > 0.0);
        assert!(separator_stroke.color.a() > 0);
    }
}
