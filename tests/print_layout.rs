//! Exercise the production AppKit renderer without a printer or interactive panel.
#[cfg(target_os = "macos")]
#[path = "../src/plugin/builtin/print/macos.rs"]
mod macos;

#[cfg(target_os = "macos")]
fn main() {
    use objc2::MainThreadMarker;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSApplication, NSPrintJobSavingURL, NSPrintOperation, NSPrintSaveJob};
    use objc2_foundation::{NSRange, NSSize, NSString, NSURL};

    let mtm = MainThreadMarker::new().expect("AppKit regression test requires the main thread");
    let _app = NSApplication::sharedApplication(mtm);
    // Calling AppKit from a worker must fail before touching any native UI.
    assert!(
        std::thread::spawn(|| macos::print("text", "test"))
            .join()
            .unwrap()
            .is_err()
    );
    let content = format!(
        "{}\n\n{}\r\n{}\nEND-OF-DOCUMENT",
        "中文长段落，需要自动换行，不能超出纸张边界。".repeat(300),
        "Long English paragraphs must wrap within the printable width. ".repeat(100),
        "https://example.com/".to_string() + &"unbrokentoken".repeat(200),
    );
    for (name, paper_size) in [
        ("a4", NSSize::new(595.28, 841.89)),
        ("letter", NSSize::new(612.0, 792.0)),
        ("landscape", NSSize::new(841.89, 595.28)),
    ] {
        let info = macos::print_info();
        info.setPaperSize(paper_size);
        let view = macos::print_view(mtm, &content, &info);
        assert_eq!(view.string().to_string(), content);
        assert_eq!(info.leftMargin(), info.rightMargin());
        assert_eq!(info.topMargin(), info.bottomMargin());
        assert!(!info.isHorizontallyCentered());
        assert!(!info.isVerticallyCentered());
        let width = paper_size.width - info.leftMargin() - info.rightMargin();
        // SAFETY: these objects belong to this main-thread view and outlive all access.
        let container = unsafe { view.textContainer() }.unwrap();
        let layout = unsafe { view.layoutManager() }.unwrap();
        layout.ensureLayoutForTextContainer(&container);
        let mut index = 0;
        let mut lines = 0;
        while index < layout.numberOfGlyphs() {
            let mut range = NSRange::new(0, 0);
            // SAFETY: range points to a live stack value; index is a valid glyph.
            let rect = unsafe {
                layout.lineFragmentUsedRectForGlyphAtIndex_effectiveRange(index, &mut range)
            };
            assert!(rect.origin.x >= -0.01, "{name}: text before the left edge");
            assert!(
                rect.origin.x + rect.size.width <= width + 0.01,
                "{name}: line overflows the right edge: {rect:?} > {width}"
            );
            assert!(range.length > 0);
            index = range.location + range.length;
            lines += 1;
        }
        assert!(lines > 100, "long paragraphs did not wrap");

        let path = std::env::temp_dir().join(format!(
            "paper-shell-print-layout-{}-{name}.pdf",
            std::process::id()
        ));
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str().unwrap()));
        // SAFETY: NSPrintJobSavingURL expects an NSURL in the print settings dictionary.
        unsafe {
            info.setJobDisposition(NSPrintSaveJob);
            info.dictionary()
                .setObject_forKey(&url, ProtocolObject::from_ref(NSPrintJobSavingURL));
        }
        let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
        operation.setShowsPrintPanel(false);
        operation.setShowsProgressPanel(false);
        assert!(operation.runOperation(), "{name}: PDF generation failed");
        assert!(
            operation.pageRange().length > 1,
            "{name}: missing pagination"
        );
        let pdf = std::fs::read(&path).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        println!(
            "{name}: {lines} wrapped lines, {} pages, PDF {}",
            operation.pageRange().length,
            path.display()
        );
        // Keep fixtures only when explicitly requested for visual inspection.
        if std::env::var_os("PAPER_SHELL_KEEP_PRINT_PDFS").is_none() {
            std::fs::remove_file(path).unwrap();
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("AppKit print layout checks only run on macOS");
}
