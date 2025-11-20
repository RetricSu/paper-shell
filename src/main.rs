mod app;
mod constant;
mod style;
mod ui;

use app::PaperShellApp;

fn main() -> eframe::Result {
    let options = ui::viewport::build_viewport();

    eframe::run_native(
        constant::DEFAULT_WINDOW_TITLE,
        options,
        Box::new(|cc| {
            // Setup fonts with CJK support
            let fonts = ui::font::setup_fonts();
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(PaperShellApp::new(cc)))
        }),
    )
}
