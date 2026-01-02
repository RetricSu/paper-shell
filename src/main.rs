use paper_shell::app::PaperShellApp;
use paper_shell::constant;
use paper_shell::ui;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use paper_shell::open_with::{install_open_with_delegate, setup_app_delegate};

#[cfg(target_os = "macos")]
use objc2_foundation::MainThreadMarker;

fn main() -> eframe::Result {
    // On macOS, set up early defaults
    #[cfg(target_os = "macos")]
    install_open_with_delegate();
    
    let initial_file = std::env::args().nth(1).map(PathBuf::from);
    let options = ui::viewport::build_viewport();
    
    eframe::run_native(
        constant::DEFAULT_WINDOW_TITLE,
        options,
        Box::new(|cc| {
            // On macOS, set up our app delegate NOW (after winit has initialized NSApplication)
            // This is the critical timing - after EventLoop creation but before processing events
            #[cfg(target_os = "macos")]
            {
                if let Some(mtm) = MainThreadMarker::new() {
                    setup_app_delegate(mtm);
                }
            }
            
            // Setup fonts with CJK support
            let fonts = ui::font::setup_fonts();
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(PaperShellApp::new(cc, initial_file)))
        }),
    )
}
