use egui::{Context, RichText, Ui};

const VIEWPORT_ID: &str = "about_window";

#[derive(Default)]
pub struct AboutWindow {
    open: bool,
}

impl AboutWindow {
    pub fn open(&mut self, ctx: &Context) {
        if self.open {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of(VIEWPORT_ID),
                egui::ViewportCommand::Focus,
            );
        } else {
            self.open = true;
        }
        ctx.request_repaint();
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(VIEWPORT_ID),
            egui::ViewportBuilder::default()
                .with_title("关于纸壳")
                .with_inner_size([360.0, 320.0])
                .with_resizable(false),
            |ctx, _class| {
                crate::style::configure_style(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(24))
                    .show(ctx, |ui| self.show_content(ui));

                if ctx.input(|input| input.viewport().close_requested())
                    || ctx.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    self.open = false;
                }
            },
        );
    }

    fn show_content(&self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new(egui_phosphor::regular::INFO)
                    .size(44.0)
                    .color(crate::style::STATUS_GREEN),
            );
            ui.add_space(8.0);
            ui.heading(crate::constant::DEFAULT_WINDOW_TITLE);
            ui.label(RichText::new(crate::constant::APP_NAME).size(16.0).strong());
            ui.add_space(12.0);
            ui.label(RichText::new(format!("版本 {}", env!("CARGO_PKG_VERSION"))).monospace());
            ui.add_space(16.0);
            ui.label(env!("CARGO_PKG_DESCRIPTION"));
            ui.add_space(16.0);
            ui.label(RichText::new("© 2025 RetricSu").small().weak());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_text(shape: &egui::Shape, needle: &str) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text().contains(needle),
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has_text(shape, needle)),
            _ => false,
        }
    }

    #[test]
    fn about_content_uses_package_metadata() {
        let ctx = Context::default();
        crate::style::configure_style(&ctx);
        let window = AboutWindow::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 320.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(24))
                    .show(ctx, |ui| window.show_content(ui));
            },
        );

        for expected in [
            crate::constant::DEFAULT_WINDOW_TITLE,
            crate::constant::APP_NAME,
            &format!("版本 {}", env!("CARGO_PKG_VERSION")),
            env!("CARGO_PKG_DESCRIPTION"),
            "© 2025 RetricSu",
        ] {
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|shape| has_text(&shape.shape, expected)),
                "about window should contain {expected:?}"
            );
        }
    }

    #[test]
    fn about_window_opens_and_escape_closes_it() {
        let ctx = Context::default();
        let mut window = AboutWindow::default();
        assert!(!window.open);

        window.open(&ctx);
        assert!(window.open);

        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| window.show(ctx),
        );
        assert!(!window.open);
    }
}
