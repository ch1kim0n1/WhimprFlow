//! Frontmost-app detection: which app is the paste target right now, so the
//! cleanup layer can format the output for the medium (email vs. text vs. chat).
//!
//! On macOS this reads `NSWorkspace.frontmostApplication`  -  the identity of the
//! focused app is public information and needs **no** extra TCC permission
//! (unlike reading the window's text, which would need Accessibility). The pill
//! overlay is non-activating, so the app the user is dictating into stays
//! frontmost; we capture its bundle id at record-start (Fn down).

/// Bundle id of the frontmost application  -  the paste target  -  e.g.
/// `com.apple.mail`. Returns `None` when it can't be determined or when
/// WhimprFlow itself is frontmost (so we don't format for our own Hub window).
#[cfg(target_os = "macos")]
#[allow(unused_unsafe)]
pub fn frontmost_bundle_id() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    // NSWorkspace reads are thread-safe; safe to call from the tap thread.
    let bid = unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        app.bundleIdentifier()?
    };
    let bid = bid.to_string();
    if bid == "com.whimpr.whimprflow" {
        None
    } else {
        Some(bid)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_bundle_id() -> Option<String> {
    None
}

/// Bring the app with `bundle_id` frontmost and wait (up to ~1 s) for the
/// switch to land, so a synthesized Cmd+V paste goes to it and not to whoever
/// is frontmost right now. Used by the approve-pending path: clicking Approve
/// in the Hub made the Hub frontmost, so a Paste-destination workflow would
/// otherwise paste into the Hub itself. Returns whether the app actually
/// became frontmost; callers should fall back to a clipboard delivery when it
/// didn't (app quit, activation denied, ...).
#[cfg(target_os = "macos")]
#[allow(unused_unsafe)]
pub fn activate_app(bundle_id: &str) -> bool {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    use objc2_foundation::NSString;
    let requested = unsafe {
        let name = NSString::from_str(bundle_id);
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&name);
        // IgnoringOtherApps is a no-op on macOS 14+ (activation there is
        // cooperative, and the user just clicked in our app so the handoff is
        // granted); earlier systems need it to switch away from the Hub.
        #[allow(deprecated)]
        apps.firstObject()
            .map(|app| {
                app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps)
            })
            .unwrap_or(false)
    };
    if !requested {
        return false;
    }
    // Activation is asynchronous  -  poll until the target is actually
    // frontmost so the paste keystroke can't land mid-switch.
    for _ in 0..20 {
        if frontmost_bundle_id().as_deref() == Some(bundle_id) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

#[cfg(not(target_os = "macos"))]
pub fn activate_app(_bundle_id: &str) -> bool {
    false
}

/// How much selected text the Context Capsule will ever carry. Enough for a
/// paragraph of reference context without shipping whole documents into a
/// cleanup prompt.
#[cfg(target_os = "macos")]
const MAX_SELECTED_CHARS: usize = 800;

/// The text currently selected in the focused UI element, read via the
/// Accessibility API (system-wide focused element -> `AXSelectedText`), capped
/// at [`MAX_SELECTED_CHARS`]. `None` when nothing is selected, the focused
/// element has no selection attribute, or Accessibility is not granted.
///
/// Unlike [`frontmost_bundle_id`], this DOES require the Accessibility
/// permission  -  it reads content, not identity  -  which is why it is strictly
/// opt-in (Context Capsule `include_selection`).
#[cfg(target_os = "macos")]
pub fn ax_selected_text() -> Option<String> {
    if !crate::paste::is_trusted() {
        return None;
    }
    ax::selected_text(MAX_SELECTED_CHARS)
}

#[cfg(not(target_os = "macos"))]
pub fn ax_selected_text() -> Option<String> {
    None
}

/// Minimal CoreFoundation/Accessibility FFI for the selection read. Mirrors the
/// helpers in `autolearn` (kept separate  -  that module's FFI is private to its
/// correction observer and this read has a different attribute chain).
#[cfg(target_os = "macos")]
mod ax {
    use std::os::raw::{c_char, c_void};
    use std::ptr;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type AXUIElementRef = *const c_void;

    const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            cstr: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetLength(s: CFStringRef) -> isize;
        fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, encoding: u32)
            -> bool;
        fn CFStringGetMaximumSizeForEncoding(len: isize, encoding: u32) -> isize;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
    }

    /// Copy one AX attribute value (retained  -  caller CFReleases it), or null.
    unsafe fn copy_attr(element: AXUIElementRef, name: &str) -> CFTypeRef {
        let Ok(c) = std::ffi::CString::new(name) else {
            return ptr::null();
        };
        let attr = CFStringCreateWithCString(ptr::null(), c.as_ptr(), KCF_STRING_ENCODING_UTF8);
        if attr.is_null() {
            return ptr::null();
        }
        let mut value: CFTypeRef = ptr::null();
        let err = AXUIElementCopyAttributeValue(element, attr, &mut value);
        CFRelease(attr);
        if err != 0 {
            return ptr::null();
        }
        value
    }

    /// Convert a CFStringRef to a Rust String (None if it isn't actually a string).
    unsafe fn cfstring_to_string(s: CFStringRef) -> Option<String> {
        if s.is_null() || CFGetTypeID(s) != CFStringGetTypeID() {
            return None;
        }
        let len = CFStringGetLength(s);
        let max = CFStringGetMaximumSizeForEncoding(len, KCF_STRING_ENCODING_UTF8) + 1;
        if max <= 0 {
            return Some(String::new());
        }
        let mut buf = vec![0i8; max as usize];
        if CFStringGetCString(s, buf.as_mut_ptr(), max, KCF_STRING_ENCODING_UTF8) {
            std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_str()
                .ok()
                .map(|x| x.to_string())
        } else {
            None
        }
    }

    /// System-wide focused element -> `AXSelectedText`, capped at `cap` chars.
    pub fn selected_text(cap: usize) -> Option<String> {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }
            let focused = copy_attr(system, "AXFocusedUIElement");
            CFRelease(system);
            if focused.is_null() {
                return None;
            }
            let value = copy_attr(focused as AXUIElementRef, "AXSelectedText");
            CFRelease(focused);
            if value.is_null() {
                return None;
            }
            let text = cfstring_to_string(value as CFStringRef);
            CFRelease(value);
            text.map(|s| s.chars().take(cap).collect::<String>())
                .filter(|s| !s.trim().is_empty())
        }
    }
}
