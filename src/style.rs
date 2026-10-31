use egui::{Color32, Context, Stroke, Style, Visuals};

/// Shared accent for the sidebar state and the active paragraph indicator.
pub const STATUS_GREEN: Color32 = Color32::from_rgb(46, 151, 83);

pub fn configure_style(ctx: &Context) {
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

    // Minimalist colors
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.0, Color32::TRANSPARENT);
    visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.hovered.bg_fill = Color32::from_gray(240);
    visuals.widgets.active.bg_fill = Color32::from_gray(230);

    visuals.selection.bg_fill = Color32::from_rgb(200, 220, 255);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(100, 100, 100));

    ctx.set_visuals(visuals);
}
