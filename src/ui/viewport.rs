use crate::constant::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};

pub fn build_viewport() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
            .with_min_inner_size([300.0, 0.0])
            //.with_decorations(false)
            //.with_transparent(true)
            .with_resizable(true),
        ..Default::default()
    }
}
