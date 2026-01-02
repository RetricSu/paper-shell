use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::NSApplication;
use objc2_foundation::{NSArray, NSDictionary, NSString, NSUserDefaults, ns_string};
use std::ffi::CString;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::ptr::NonNull;

// Use Mutex so we can receive files multiple times
pub static PENDING_FILES: Mutex<Vec<String>> = Mutex::new(Vec::new());

// Track if handler has been registered
static HANDLER_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Called early in main() - sets up defaults before anything else
pub fn install_open_with_delegate() {
    // Prevent AppKit from interpreting command line arguments as files to open
    let keys = &[ns_string!("NSTreatUnknownArgumentsAsOpen")];
    let objects = &[ns_string!("NO") as &AnyObject];
    let dict = NSDictionary::from_slices(keys, objects);
    unsafe {
        NSUserDefaults::standardUserDefaults().registerDefaults(&dict);
    }
    println!("[Paper Shell] NSUserDefaults configured");
}

/// Register file handler by subclassing winit's existing delegate.
/// This must be called AFTER eframe has created the window/event loop,
/// i.e., inside the closure passed to run_native.
pub fn setup_app_delegate(mtm: MainThreadMarker) {
    if HANDLER_REGISTERED.swap(true, Ordering::SeqCst) {
        println!("[Paper Shell] File handler already registered");
        return;
    }

    // Use raw pointers to avoid lifetime issues with MethodImplementation
    // See: https://developer.apple.com/documentation/appkit/nsapplicationdelegate/application(_:openfiles:)?language=objc
    unsafe extern "C-unwind" fn handle_open_files(
        _this: NonNull<AnyObject>,
        _sel: Sel,
        _sender: NonNull<AnyObject>,
        filenames: NonNull<NSArray<NSString>>,
    ) {
        println!("[Paper Shell] application:openFiles: called!");
        let filenames = filenames.as_ref();
        println!("[Paper Shell] Got {} files", filenames.len());
        if let Ok(mut files) = PENDING_FILES.lock() {
            for filename in filenames.iter() {
                let path = filename.to_string();
                println!("[Paper Shell] Queuing file: {}", path);
                files.push(path);
            }
        }
    }

    unsafe {
        let app = NSApplication::sharedApplication(mtm);
        
        let Some(delegate) = app.delegate() else {
            println!("[Paper Shell] ERROR: No delegate found on NSApplication!");
            return;
        };

        // Find out class of the existing delegate (set by winit)
        let class: &AnyClass = AnyObject::class(delegate.as_ref());
        let original_class_name = class.name().to_string_lossy();
        println!("[Paper Shell] Found existing delegate class: {}", original_class_name);

        // Register subclass of whatever was in delegate
        let class_name = CString::new("PaperShellApplicationDelegate").unwrap();
        
        // Check if our subclass already exists (shouldn't happen, but be safe)
        if AnyClass::get(class_name.as_c_str()).is_some() {
            println!("[Paper Shell] Subclass already exists, skipping");
            return;
        }

        let Some(mut builder) = ClassBuilder::new(class_name.as_c_str(), class) else {
            println!("[Paper Shell] ERROR: Failed to create ClassBuilder");
            return;
        };
        
        // Add our openFiles handler to the subclass
        builder.add_method(
            sel!(application:openFiles:),
            handle_open_files as unsafe extern "C-unwind" fn(NonNull<AnyObject>, Sel, NonNull<AnyObject>, NonNull<NSArray<NSString>>),
        );
        
        let new_class = builder.register();

        // Swap the delegate's class to our subclass
        // This is safe because:
        //  * our class is a subclass of the original
        //  * we don't add new ivars
        //  * overridden methods are compatible (we implement a protocol method)
        AnyObject::set_class(delegate.as_ref(), new_class);
        
        println!("[Paper Shell] Successfully registered file handler by subclassing delegate");
    }
}

/// Check if handler is registered
pub fn is_handler_registered() -> bool {
    HANDLER_REGISTERED.load(Ordering::SeqCst)
}
