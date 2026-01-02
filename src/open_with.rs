//! 在 egui 启动前劫持 NSApp，收到"Open With"文件后通过
//! OnceCell 传给 egui 的 NativeOptions::initial_window_info
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{NSArray, MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use once_cell::sync::OnceCell;

static OPENED_PATHS: OnceCell<Vec<String>> = OnceCell::new();

// ------------------ 1. 声明 Delegate ------------------

define_class! {
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug)]
    pub struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(application:openFiles:))]
        fn application_open_files(&self, _application: &NSApplication, filenames: &NSArray<NSString>) {
            let paths: Vec<String> = filenames.iter().map(|nsstr| nsstr.to_string()).collect();
            OPENED_PATHS.set(paths).ok(); // Ignore if already set
        }
    }
}

impl AppDelegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![mtm.alloc(), init] }
    }
}

// ------------------ 2. 提前挂 Delegate ------------------
pub fn install_open_with_delegate() {
    let mtm = MainThreadMarker::new().expect("must be main thread");
    let delegate = AppDelegate::new(mtm);
    let app = NSApplication::sharedApplication(mtm);
    // winit 保证它自己没设 delegate，所以我们是第一个
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
}

pub fn get_opened_paths() -> Option<&'static Vec<String>> {
    OPENED_PATHS.get()
}
