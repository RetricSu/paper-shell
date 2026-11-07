use crate::updater::{UpdateState, Updater};
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

    pub fn show(&mut self, ctx: &Context, updater: &mut Updater) -> bool {
        if !self.open {
            return false;
        }

        let mut install_requested = false;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(VIEWPORT_ID),
            egui::ViewportBuilder::default()
                .with_title("关于纸壳")
                .with_inner_size([380.0, 460.0])
                .with_resizable(false),
            |ctx, _class| {
                crate::style::configure_style(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(24))
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            install_requested = self.show_content(ui, updater);
                        });
                    });

                if ctx.input(|input| input.viewport().close_requested())
                    || ctx.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    self.open = false;
                }
            },
        );
        if install_requested {
            self.open = false;
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
        install_requested
    }

    fn show_content(&self, ui: &mut Ui, updater: &mut Updater) -> bool {
        let mut install_requested = false;
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
            ui.separator();
            ui.add_space(8.0);
            match &updater.state {
                UpdateState::Idle => {
                    if ui.button("检查更新").clicked() {
                        updater.check(ui.ctx());
                    }
                }
                UpdateState::Checking => {
                    ui.spinner();
                    ui.label("正在检查更新…");
                }
                UpdateState::Current => {
                    ui.label("当前已是最新稳定版");
                    if ui.button("重新检查").clicked() {
                        updater.check(ui.ctx());
                    }
                }
                UpdateState::Available(release) => {
                    ui.label(format!("发现新版本 {}", release.version));
                    ui.hyperlink_to("查看更新说明", &release.notes_url);
                    if ui.button("下载更新").clicked() {
                        updater.download(ui.ctx());
                    }
                }
                UpdateState::Downloading { downloaded, total } => {
                    let fraction = if *total == 0 {
                        0.0
                    } else {
                        *downloaded as f32 / *total as f32
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .text(format!("下载更新 {:.0}%", fraction * 100.0)),
                    );
                    ui.label(RichText::new("下载后会校验更新包").small().weak());
                }
                UpdateState::Ready(update) => {
                    ui.label(format!("版本 {} 已就绪", update.version));
                    ui.label(RichText::new("重启前会确认未保存的内容").small().weak());
                    install_requested = ui.button("安装并重启").clicked();
                }
                UpdateState::Failed(error) => {
                    ui.label(error);
                    if ui.button("重试检查").clicked() {
                        updater.check(ui.ctx());
                    }
                }
            }
            ui.hyperlink_to("GitHub 发布页面", crate::updater::RELEASES_URL);
            ui.add_space(12.0);
            ui.label(RichText::new("© 2025 RetricSu").small().weak());
        });
        install_requested
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
                    egui::vec2(380.0, 460.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(24))
                    .show(ctx, |ui| window.show_content(ui, &mut Updater::default()));
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
            |ctx| {
                window.show(ctx, &mut Updater::default());
            },
        );
        assert!(!window.open);
    }
}
