use crate::constant::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};

const APP_ICON_RGBA: &[u8] = include_bytes!("../../assets/app-icon-rgba.bin");

/// Hosts a tool in a native window, with an embedded fallback for other backends.
/// Keyboard dismissal is handled in the child viewport so it cannot close the editor.
pub fn show_tool_window(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: [f32; 2],
    open: &mut bool,
    contents: impl FnMut(&mut egui::Ui),
) {
    show_tool_window_with_min_size(ctx, id, title, size, [320.0, 160.0], open, contents);
}

pub fn show_tool_window_with_min_size(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: [f32; 2],
    min_size: [f32; 2],
    open: &mut bool,
    mut contents: impl FnMut(&mut egui::Ui),
) {
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of(id),
        egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_min_inner_size(min_size)
            .with_resizable(true),
        |ctx, class| {
            crate::style::configure_style(ctx);
            if class == egui::ViewportClass::Embedded {
                egui::Window::new(title)
                    .id(egui::Id::new(id))
                    .open(open)
                    .collapsible(false)
                    .default_size(size)
                    .min_size(min_size)
                    .show(ctx, |ui| contents(ui));
            } else {
                egui::CentralPanel::default().show(ctx, |ui| contents(ui));
                if ctx.input(|i| i.viewport().close_requested()) {
                    *open = false;
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                *open = false;
            }
        },
    );
}

pub fn build_viewport() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(egui::IconData {
                rgba: APP_ICON_RGBA.to_vec(),
                width: 256,
                height: 256,
            })
            .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
            .with_min_inner_size([300.0, 0.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(true),
        ..Default::default()
    }
}
