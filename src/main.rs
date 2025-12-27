mod app;
mod backend;
mod config;
mod constant;
mod sidebar_backend;
mod style;
mod ui;

use app::PaperShellApp;
use std::path::PathBuf;

fn main() -> eframe::Result {
    // Initialize tracing subscriber for console output
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_thread_ids(true)
        .init();

    let initial_file = std::env::args().nth(1).map(PathBuf::from);
    let options = ui::viewport::build_viewport();

    eframe::run_native(
        constant::DEFAULT_WINDOW_TITLE,
        options,
        Box::new(|cc| {
            // Setup fonts with CJK support
            let fonts = ui::font::setup_fonts();
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(PaperShellApp::new(cc, initial_file)))
        }),
    )
}
