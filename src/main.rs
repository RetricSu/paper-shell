use paper_shell::app::PaperShellApp;
use paper_shell::constant;
use paper_shell::ui;
use std::path::PathBuf;
use paper_shell::open_with::install_open_with_delegate;

fn main() -> eframe::Result {
    install_open_with_delegate();
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
