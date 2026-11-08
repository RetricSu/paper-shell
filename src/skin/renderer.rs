use super::model::Skin;
use crate::config::parse_hex_color;
use base64::Engine;
use egui::{
    Color32, Context, Pos2, Rect, Stroke, Style, TextureHandle, TextureOptions, Ui, Vec2, Visuals,
};

pub struct SkinRenderer {
    cached_texture: Option<(String, TextureHandle)>,
}

impl Default for SkinRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl SkinRenderer {
    pub fn new() -> Self {
        Self {
            cached_texture: None,
        }
    }

    /// Load or retrieve the background image texture for this skin.
    pub fn get_or_load_texture(&mut self, ctx: &Context, skin: &Skin) -> Option<&TextureHandle> {
        let key = if let Some(ref data) = skin.effects.background_image_data {
            format!("{}:b64:{}", skin.id, data.len())
        } else if let Some(ref name) = skin.effects.background_image {
            format!("{}:file:{}", skin.id, name)
        } else {
            self.cached_texture = None;
            return None;
        };

        if let Some((ref cached_key, _)) = self.cached_texture
            && cached_key == &key
        {
            return self.cached_texture.as_ref().map(|(_, tex)| tex);
        }

        // Texture needs loading
        let bytes_opt: Option<Vec<u8>> = if let Some(ref b64) = skin.effects.background_image_data {
            base64::engine::general_purpose::STANDARD.decode(b64).ok()
        } else if let Some(ref img_name) = skin.effects.background_image
            && let Some(ref dir) = skin.dir
        {
            let path = dir.join(img_name);
            std::fs::read(&path).ok()
        } else {
            None
        };

        let Some(bytes) = bytes_opt else {
            self.cached_texture = None;
            return None;
        };

        if bytes.len() > 10 * 1024 * 1024 {
            tracing::warn!("Background image exceeds 10MB limit, skipping");
            self.cached_texture = None;
            return None;
        }

        if let Ok(img) = image::load_from_memory(&bytes) {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            if width > 4096 || height > 4096 {
                tracing::warn!(
                    "Background image dimensions {}x{} exceed 4096 limit, skipping",
                    width,
                    height
                );
                self.cached_texture = None;
                return None;
            }
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                rgba.as_raw(),
            );
            let handle = ctx.load_texture(
                format!("skin_bg_{}", skin.id),
                color_image,
                TextureOptions::LINEAR,
            );
            self.cached_texture = Some((key, handle));
            self.cached_texture.as_ref().map(|(_, tex)| tex)
        } else {
            self.cached_texture = None;
            None
        }
    }

    /// Paint optional background image behind editor content.
    pub fn paint_background(&mut self, ui: &mut Ui, skin: &Skin, rect: Rect) {
        // Paint background image if available
        if let Some(texture) = self.get_or_load_texture(ui.ctx(), skin) {
            let tex_size = texture.size_vec2();
            if tex_size.x > 0.0 && tex_size.y > 0.0 {
                let opacity = skin.effects.background_image_opacity.clamp(0.0, 1.0);
                let alpha = (opacity * 255.0) as u8;
                let tint = Color32::from_white_alpha(alpha);
                let fit = skin.effects.background_image_fit.as_str();

                if fit == "tile" {
                    let painter = ui.painter().with_clip_rect(rect);
                    let mut y = rect.min.y;
                    while y < rect.max.y {
                        let mut x = rect.min.x;
                        while x < rect.max.x {
                            let tile_rect = Rect::from_min_size(Pos2::new(x, y), tex_size);
                            painter.image(
                                texture.id(),
                                tile_rect,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                tint,
                            );
                            x += tex_size.x;
                        }
                        y += tex_size.y;
                    }
                    return;
                }

                let (dest_rect, uv) = match fit {
                    "contain" => {
                        let scale = (rect.width() / tex_size.x).min(rect.height() / tex_size.y);
                        let draw_size = tex_size * scale;
                        let min_pos = rect.center() - draw_size * 0.5;
                        (
                            Rect::from_min_size(min_pos, draw_size),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        )
                    }
                    "center" => {
                        let min_pos = rect.center() - tex_size * 0.5;
                        (
                            Rect::from_min_size(min_pos, tex_size),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        )
                    }
                    _ => {
                        // Fit to cover entire rect with clipping
                        let scale = (rect.width() / tex_size.x).max(rect.height() / tex_size.y);
                        let draw_size = tex_size * scale;
                        let min_pos = rect.center() - draw_size * 0.5;
                        (
                            Rect::from_min_size(min_pos, draw_size),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        )
                    }
                };

                // Paint clipped inside rect bounds
                let painter = ui.painter().with_clip_rect(rect);
                painter.image(texture.id(), dest_rect, uv, tint);
            }
        }
    }

    /// Apply skin spacing and colors to egui global context.
    pub fn apply_to_context(ctx: &Context, skin: &Skin) {
        let mut style = Style::default();

        // Spacing
        style.spacing.item_spacing = Vec2::new(
            skin.spacing.item_spacing_x.max(0.0),
            skin.spacing.item_spacing_y.max(0.0),
        );
        style.spacing.button_padding = Vec2::new(
            skin.spacing.button_padding_x.max(0.0),
            skin.spacing.button_padding_y.max(0.0),
        );
        style.spacing.window_margin = egui::Margin::same(skin.spacing.window_margin as i8);

        ctx.set_style(style);

        // Visuals
        let mut visuals = if skin.colors.dark_mode {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        if !skin.effects.shadow {
            visuals.window_shadow = egui::epaint::Shadow::NONE;
            visuals.popup_shadow = egui::epaint::Shadow::NONE;
        }

        let radius = egui::CornerRadius::same(skin.effects.corner_radius as u8);
        visuals.window_corner_radius = radius;
        visuals.menu_corner_radius = radius;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = radius;
        }

        // Colors
        if let Some(ref hex) = skin.colors.background_color
            && let Some(color) = parse_hex_color(hex)
        {
            visuals.panel_fill = color;
            visuals.window_fill = color;
        }

        let win_opacity = skin.effects.window_opacity.clamp(0.1, 1.0);
        if win_opacity < 1.0 {
            visuals.window_fill = visuals.window_fill.gamma_multiply(win_opacity);
            visuals.panel_fill = visuals.panel_fill.gamma_multiply(win_opacity);
        }

        visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;

        if let Some(ref hex) = skin.colors.selection_color
            && let Some(color) = parse_hex_color(hex)
        {
            visuals.selection.bg_fill = color;
        }

        if let Some(ref hex) = skin.colors.accent_color
            && let Some(color) = parse_hex_color(hex)
        {
            visuals.selection.stroke = Stroke::new(1.0, color);
        }

        if let Some(ref hex) = skin.colors.text_color
            && let Some(color) = parse_hex_color(hex)
        {
            visuals.override_text_color = Some(color);
        }

        ctx.set_visuals(visuals);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_skin_style_to_context() {
        let ctx = Context::default();
        let mut skin = Skin::warm_parchment();
        skin.spacing.item_spacing_x = 12.0;
        skin.spacing.item_spacing_y = 10.0;
        skin.effects.corner_radius = 4.0;
        skin.colors.dark_mode = true;

        SkinRenderer::apply_to_context(&ctx, &skin);

        assert_eq!(ctx.style().spacing.item_spacing.x, 12.0);
        assert_eq!(ctx.style().spacing.item_spacing.y, 10.0);
        assert!(ctx.style().visuals.dark_mode);
        assert_eq!(
            ctx.style().visuals.window_corner_radius,
            egui::CornerRadius::same(4)
        );
    }

    #[test]
    fn background_texture_loading_from_base64() {
        let ctx = Context::default();
        let mut renderer = SkinRenderer::new();
        let mut skin = Skin::classic_paper();

        // 1x1 transparent png
        let png_1x1_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        skin.effects.background_image_data = Some(png_1x1_base64.to_string());

        let tex = renderer.get_or_load_texture(&ctx, &skin);
        assert!(tex.is_some());
        let tex_size = tex.unwrap().size_vec2();
        assert_eq!(tex_size.x, 1.0);
        assert_eq!(tex_size.y, 1.0);
    }
}
