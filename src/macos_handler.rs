use std::sync::Mutex;

// Global buffer to store files received from macOS
pub static PENDING_FILES: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[cfg(target_os = "macos")]
pub mod implementation {
    use super::PENDING_FILES;
    use objc2::ffi::{class_addMethod, class_getInstanceMethod, class_replaceMethod, method_getImplementation};
    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{NSArray, NSURL};
    
    use std::os::raw::c_char;

    // Function pointer type for the swizzled method
    type OpenURLsFn = extern "C" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>);

    extern "C" fn swizzled_application_open_urls(
        this: &AnyObject,
        _cmd: Sel,
        app: &AnyObject,
        urls: &NSArray<NSURL>,
    ) {
        // Call the original method if it exists
        let original_sel = Sel::register(c"original_application:openURLs:");
        let original_imp: Option<OpenURLsFn> = unsafe {
            let method = class_getInstanceMethod(this.class(), original_sel);
            if !method.is_null() {
                Some(std::mem::transmute(method_getImplementation(method)))
            } else {
                None
            }
        };

        if let Some(original) = original_imp {
            original(this, _cmd, app, urls);
        }

        // Handle our file opening logic
        if let Ok(mut files) = PENDING_FILES.lock() {
            for url in urls {
                // Only handle file URLs
                if url.isFileURL() {
                    if let Some(path) = url.path() {
                        files.push(path.to_string());
                    } else {
                        eprintln!("Warning: File URL has no path: {:?}", url);
                    }
                } else {
                    eprintln!("Warning: Non-file URL received: {:?}", url);
                }
            }
        }
    }

    /// Injects the openURLs method into winit's delegate using method swizzling.
    /// This should be called after winit has set up its delegate (e.g., in App::new).
    pub fn install() {
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);

        if let Some(delegate) = app.delegate() {
            // Get the underlying AnyObject from the ProtocolObject
            let delegate_obj = unsafe { &*(delegate.as_ref() as *const _ as *const AnyObject) };
            let delegate_class = delegate_obj.class();

            // Register our selector
            let open_urls_sel = Sel::register(c"application:openURLs:");

            // Check if the method already exists
            let existing_method = unsafe { class_getInstanceMethod(delegate_class, open_urls_sel) };

            if !existing_method.is_null() {
                // Method exists, we need to swizzle it
                let original_sel = Sel::register(c"original_application:openURLs:");
                let original_imp = unsafe { method_getImplementation(existing_method) };

                if let Some(original_imp) = original_imp {
                    // Add the original method with a different name
                    let success = unsafe {
                        class_addMethod(
                            delegate_class as *const _ as *mut AnyClass,
                            original_sel,
                            original_imp,
                            b"v@:@@\0".as_ptr() as *const c_char,
                        )
                    };

                    if success.is_true() {
                        // Replace the original method with our swizzled version
                        unsafe {
                            class_replaceMethod(
                                delegate_class as *const _ as *mut AnyClass,
                                open_urls_sel,
                                std::mem::transmute(swizzled_application_open_urls as *const ()),
                                b"v@:@@\0".as_ptr() as *const c_char,
                            );
                        }
                        println!("Successfully swizzled application:openURLs: method");
                    } else {
                        eprintln!("Failed to add original method for swizzling");
                    }
                } else {
                    eprintln!("Failed to get implementation of existing method");
                }
            } else {
                // Method doesn't exist, just add it
                let success = unsafe {
                    class_addMethod(
                        delegate_class as *const _ as *mut AnyClass,
                        open_urls_sel,
                        std::mem::transmute(swizzled_application_open_urls as *const ()),
                        b"v@:@@\0".as_ptr() as *const c_char,
                    )
                };

                if success.is_true() {
                    println!("Successfully added application:openURLs: method");
                } else {
                    eprintln!("Failed to add application:openURLs: method to delegate");
                }
            }
        } else {
            eprintln!("Warning: No delegate found on NSApplication during install");
        }
    }
}
