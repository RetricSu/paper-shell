use crate::constant::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};

const APP_ICON_RGBA: &[u8] = include_bytes!("../../assets/app-icon-rgba.bin");

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
