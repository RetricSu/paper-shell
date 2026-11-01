//! AppKit owns pagination, paper selection, PDF preview, and printer delivery.
//! Never send plain text through CUPS: its text filter has different layout rules.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSColor, NSFont, NSPrintInfo, NSPrintOperation, NSPrintPanelOptions, NSPrintingPaginationMode,
    NSTextView,
};
use objc2_foundation::{NSCopying, NSPoint, NSRect, NSSize, NSString};

pub fn print(content: &str, document_name: &str) -> Result<String, String> {
    let mtm = MainThreadMarker::new().ok_or("系统打印面板必须在主线程打开")?;
    let info = print_info();
    let view = print_view(mtm, content, &info);
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    operation.setJobTitle(Some(&NSString::from_str(document_name)));
    operation.setShowsPrintPanel(true);
    operation.setShowsProgressPanel(true);
    let panel = operation.printPanel();
    panel.setOptions(
        panel.options()
            | NSPrintPanelOptions::ShowsPreview
            | NSPrintPanelOptions::ShowsPaperSize
            | NSPrintPanelOptions::ShowsOrientation
            | NSPrintPanelOptions::ShowsScaling,
    );
    if operation.runOperation() {
        // The user may have saved a PDF instead of sending a job to a printer.
        Ok(format!("系统打印操作已完成：{document_name}"))
    } else {
        // AppKit reports both cancellation and an unsuccessful operation as false.
        Ok("打印已取消或未完成".to_string())
    }
}

pub(super) fn print_info() -> Retained<NSPrintInfo> {
    // Copy the system's printer/paper defaults without changing global settings.
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setLeftMargin(72.0);
    info.setRightMargin(72.0);
    info.setTopMargin(72.0);
    info.setBottomMargin(72.0);
    info.setHorizontallyCentered(false);
    info.setVerticallyCentered(false);
    info.setScalingFactor(1.0);
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
    info
}

pub(super) fn print_view(
    mtm: MainThreadMarker,
    content: &str,
    info: &NSPrintInfo,
) -> Retained<NSTextView> {
    let width = (info.paperSize().width - info.leftMargin() - info.rightMargin()).max(1.0);
    let view = NSTextView::initWithFrame(
        NSTextView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, 1.0)),
    );
    view.setRichText(false);
    view.setHorizontallyResizable(false);
    view.setVerticallyResizable(true);
    view.setMinSize(NSSize::new(0.0, 0.0));
    view.setMaxSize(NSSize::new(f64::MAX, f64::MAX));
    view.setTextContainerInset(NSSize::new(0.0, 0.0));
    // SAFETY: the view owns its text container; all access stays on the main thread.
    if let Some(container) = unsafe { view.textContainer() } {
        container.setWidthTracksTextView(true);
        container.setHeightTracksTextView(false);
        container.setContainerSize(NSSize::new(width, f64::MAX));
        container.setLineFragmentPadding(0.0);
    }
    view.setString(&NSString::from_str(content));
    view.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    // Printed ink must not depend on the editor or system dark appearance.
    view.setTextColor(Some(&NSColor::blackColor()));
    view.setBackgroundColor(&NSColor::whiteColor());
    view.sizeToFit();
    view
}
