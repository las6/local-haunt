// AppKit controls whether this application participates in the Dock and Cmd+Tab.
// The headless GPUI tests do not change the host application's policy.
#[cfg(all(target_os = "macos", not(test)))]
pub fn set_window_presence(has_window: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let main_thread = MainThreadMarker::new().expect("AppKit must run on the main thread");
    let app = NSApplication::sharedApplication(main_thread);
    let policy = if has_window {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    if !app.setActivationPolicy(policy) {
        eprintln!("macOS declined the application activation policy change");
    }
}

#[cfg(any(not(target_os = "macos"), test))]
pub fn set_window_presence(_: bool) {}
