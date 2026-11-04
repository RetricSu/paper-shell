//! macOS native NSEvent monitor for capturing Escape at the OS level.
//!
//! This is the only way to reliably intercept Escape when macOS IME is active:
//! `interpretKeyEvents` may consume the key event before winit ever sees it,
//! so we install an `NSEvent` local monitor that fires on every key-down
//! before any IME or window server routing.

#[cfg(target_os = "macos")]
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};
#[cfg(target_os = "macos")]
use std::sync::mpsc::Sender;
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "macos")]
static ESCAPE_SENDER: Mutex<Option<Sender<()>>> = Mutex::new(None);
#[cfg(target_os = "macos")]
static ESCAPE_MONITOR_INSTALLED: OnceLock<()> = OnceLock::new();

#[cfg(target_os = "macos")]
pub fn set_escape_sender(sender: Sender<()>) {
    if let Ok(mut s) = ESCAPE_SENDER.lock() {
        *s = Some(sender);
    }
}

#[cfg(target_os = "macos")]
pub fn install_escape_monitor() {
    ESCAPE_MONITOR_INSTALLED.get_or_init(|| unsafe {
        use block2::RcBlock;

        let block = RcBlock::new(|event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
            let event = event.as_ref();
            if event.r#type() == NSEventType::KeyDown
                && event.keyCode() == 53
                && let Ok(s) = ESCAPE_SENDER.lock()
                && let Some(sender) = &*s
            {
                let _ = sender.send(());
            }
            event as *const NSEvent as *mut NSEvent
        });

        // NSEventMaskKeyDown = 1 << 10
        let mask = NSEventMask::from_bits_retain(1 << 10);
        let _monitor = NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block);
        // Leak the monitor intentionally: it must live for the entire app lifetime.
        if let Some(monitor) = _monitor {
            let _ = objc2::rc::Retained::into_raw(monitor);
        }
        // Leak the block as well so the monitor can call it forever.
        let _ = RcBlock::into_raw(block);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn install_escape_monitor() {}

#[cfg(not(target_os = "macos"))]
pub fn set_escape_sender(_sender: std::sync::mpsc::Sender<()>) {}
