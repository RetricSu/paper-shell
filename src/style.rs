use egui::{Color32, Context, Stroke, Style, Visuals};

pub fn configure_style(ctx: &Context) {
    let mut style = Style::default();

    // Elegant visual settings
    // We want a clean, white paper look.

    // Increase spacing for elegance
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(15.0);

    ctx.set_style(style);

    let mut visuals = Visuals::light();
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;

    // Minimalist colors
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.0, Color32::TRANSPARENT);
    visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.hovered.bg_fill = Color32::from_gray(240);
    visuals.widgets.active.bg_fill = Color32::from_gray(230);

    visuals.selection.bg_fill = Color32::from_rgb(200, 220, 255);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(100, 100, 100));

    ctx.set_visuals(visuals);
}
