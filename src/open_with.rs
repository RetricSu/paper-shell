use crate::messages::ResponseMessage;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::NSApplication;
use objc2_foundation::{
    NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSString, NSUserDefaults,
    ns_string,
};
use std::ffi::CString;
use std::mem::ManuallyDrop;
use std::os::raw::c_uchar;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

// --- Global State ---

static PENDING_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static SENDER: Mutex<Option<Sender<ResponseMessage>>> = Mutex::new(None);
static REGISTER_ONCE: OnceLock<()> = OnceLock::new();

/// 1. Called early in main()
pub fn install_open_with_delegate() {
    unsafe {
        // Prevent AppKit from handling args
        let keys = &[ns_string!("NSTreatUnknownArgumentsAsOpen")];
        let objects = &[ns_string!("NO") as &AnyObject];
        let dict = NSDictionary::from_slices(keys, objects);
        NSUserDefaults::standardUserDefaults().registerDefaults(&dict);

        REGISTER_ONCE.get_or_init(|| {
            let _mtm = MainThreadMarker::new().expect("Must run on main thread");
            let center = NSNotificationCenter::defaultCenter();

            let cls_name = CString::new("PaperShellBootstrapper").unwrap();
            let nsobj_cstr = CString::new("NSObject").unwrap();
            let super_cls = AnyClass::get(nsobj_cstr.as_c_str()).unwrap();
            let mut builder = ClassBuilder::new(cls_name.as_c_str(), super_cls).unwrap();

            builder.add_method(
                sel!(onWillFinish:),
                on_will_finish_launching as unsafe extern "C-unwind" fn(_, _, _),
            );

            let cls = builder.register();

            // --- ALLOC/INIT FIX START ---
            let alloc_ptr: *mut AnyObject = msg_send![cls, alloc];
            let init_ptr: *mut AnyObject = msg_send![alloc_ptr, init];
            let listener: Retained<AnyObject> =
                Retained::from_raw(init_ptr).expect("Failed to create bootstrapper instance");

            // We must leak this because NSNotificationCenter doesn't hold a strong reference
            let listener = ManuallyDrop::new(listener);
            let listener_ref: &AnyObject = &listener;
            // --- ALLOC/INIT FIX END ---

            let name = ns_string!("NSApplicationWillFinishLaunchingNotification");
            center.addObserver_selector_name_object(
                listener_ref,
                sel!(onWillFinish:),
                Some(name),
                None,
            );

            println!("[Paper Shell] Bootstrapper registered.");
        });
    }
}

/// 2. Called from eframe closure
pub fn complete_app_setup(sender: Sender<ResponseMessage>) {
    if let Ok(mut s) = SENDER.lock() {
        *s = Some(sender.clone());
    }

    if let Ok(mut pending) = PENDING_FILES.lock()
        && !pending.is_empty()
    {
        println!("[Paper Shell] Flushing {} pending files...", pending.len());
        for path in pending.drain(..) {
            let _ = sender.send(ResponseMessage::OpenFile(path));
        }
    }
}

unsafe extern "C-unwind" fn on_will_finish_launching(
    _this: NonNull<AnyObject>,
    _sel: Sel,
    _notif: NonNull<NSNotification>,
) {
    unsafe {
        println!("[Paper Shell] Notification received: WillFinish. Swizzling now...");

        let mtm = MainThreadMarker::new_unchecked();
        let app = NSApplication::sharedApplication(mtm);

        let Some(delegate) = app.delegate() else {
            println!("[Paper Shell] Error: No delegate found to swizzle.");
            return;
        };

        let class = AnyObject::class(delegate.as_ref());
        let class_name = CString::new("PaperShellApplicationDelegate").unwrap();

        if AnyClass::get(class_name.as_c_str()).is_none()
            && let Some(mut builder) = ClassBuilder::new(class_name.as_c_str(), class)
        {
            builder.add_method(
                sel!(application:openFiles:),
                handle_open_files as unsafe extern "C-unwind" fn(_, _, _, _),
            );
            builder.add_method(
                sel!(application:openFile:),
                handle_open_file as unsafe extern "C-unwind" fn(_, _, _, _) -> c_uchar,
            );

            let new_class = builder.register();
            AnyObject::set_class(delegate.as_ref(), new_class);

            // Re-assign delegate to flush cache
            app.setDelegate(Some(delegate.as_ref()));
            println!("[Paper Shell] Swizzle complete.");
        }
    }
}

unsafe extern "C-unwind" fn handle_open_files(
    _this: NonNull<AnyObject>,
    _cmd: Sel,
    _sender: NonNull<AnyObject>,
    filenames: NonNull<NSArray<NSString>>,
) {
    unsafe {
        let filenames = filenames.as_ref();
        let mut pending_lock = PENDING_FILES.lock().unwrap();
        let sender_lock = SENDER.lock().unwrap();

        for filename in filenames.iter() {
            let path = PathBuf::from(filename.to_string());
            if let Some(s) = &*sender_lock {
                let _ = s.send(ResponseMessage::OpenFile(path));
            } else {
                pending_lock.push(path);
            }
        }
    }
}

unsafe extern "C-unwind" fn handle_open_file(
    _this: NonNull<AnyObject>,
    _cmd: Sel,
    _sender: NonNull<AnyObject>,
    filename: NonNull<NSString>,
) -> c_uchar {
    unsafe {
        let path = PathBuf::from(filename.as_ref().to_string());
        let mut pending_lock = PENDING_FILES.lock().unwrap();
        let sender_lock = SENDER.lock().unwrap();

        if let Some(s) = &*sender_lock {
            let _ = s.send(ResponseMessage::OpenFile(path));
        } else {
            pending_lock.push(path);
        }
        1
    }
}
