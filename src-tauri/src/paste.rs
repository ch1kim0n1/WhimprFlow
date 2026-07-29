//! Text insertion: deliver transcribed/cleaned text to the frontmost app.
//!
//! First rung of the insertion ladder - clipboard paste: save the current
//! clipboard, write our text, synthesize the platform paste shortcut, then
//! restore the clipboard. This is the universal path that works in almost
//! every app. (AX/UIA direct-insert and terminal/secure-input handling layer
//! on later.)

/// Restore (or clear) the clipboard after a paste, honoring
/// `clear_clipboard_after_paste`.
///
/// On Windows, when clearing, also clears Windows Clipboard History (Win+V).
/// macOS Universal Clipboard ages out on its own; Linux has no standard history API.
pub(crate) fn restore_clipboard_after_paste(cb: &mut arboard::Clipboard, saved: Option<String>) {
    let clear_empty = crate::hotkey::current_settings().clear_clipboard_after_paste;
    if clear_empty {
        match saved {
            Some(prev) if !prev.is_empty() => {
                let _ = cb.set_text(prev);
            }
            _ => {
                let _ = cb.clear();
                clear_os_clipboard_history();
            }
        }
    } else if let Some(prev) = saved {
        let _ = cb.set_text(prev);
    }
}

#[cfg(target_os = "windows")]
fn clear_os_clipboard_history() {
    // Win10 1809+: ClearClipboardHistory in user32. Resolve at runtime so older
    // SDKs / linkers without the import still build.
    use windows::core::PCSTR;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    type ClearFn = unsafe extern "system" fn() -> i32;
    unsafe {
        let Ok(lib) = LoadLibraryA(PCSTR::from_raw(c"user32.dll".as_ptr().cast())) else {
            return;
        };
        let Some(proc) = GetProcAddress(
            HMODULE(lib.0),
            PCSTR::from_raw(c"ClearClipboardHistory".as_ptr().cast()),
        ) else {
            return;
        };
        let f: ClearFn = std::mem::transmute(proc);
        let _ = f();
    }
}

#[cfg(not(target_os = "windows"))]
fn clear_os_clipboard_history() {}

#[cfg(target_os = "macos")]
mod imp {
    use std::os::raw::c_void;
    use std::ptr::null;
    use std::time::Duration;

    type CGEventRef = *mut c_void;
    type CGEventSourceRef = *const c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            keycode: u16,
            keydown: bool,
        ) -> CGEventRef;
        fn CGEventSetFlags(event: CGEventRef, flags: u64);
        fn CGEventPost(tap: u32, event: CGEventRef);
        /// Whether the app has Input Monitoring (listen-event) access - required for
        /// the Fn key tap to see keystrokes globally, not just while we're frontmost.
        fn CGPreflightListenEventAccess() -> bool;
        /// Request Input Monitoring access: registers the app in the list and prompts.
        fn CGRequestListenEventAccess() -> bool;
    }

    /// True when Input Monitoring is granted (the Fn tap works in every app).
    pub fn input_monitoring_granted() -> bool {
        unsafe { CGPreflightListenEventAccess() }
    }

    /// Prompt for Input Monitoring and register the app in the settings list.
    pub fn request_input_monitoring() -> bool {
        unsafe { CGRequestListenEventAccess() }
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    const KCG_HID_EVENT_TAP: u32 = 0;
    const KCG_FLAG_MASK_COMMAND: u64 = 0x0010_0000;
    const KEYCODE_V: u16 = 9;

    /// Whether the app has Accessibility permission. This one grant governs BOTH the
    /// global Fn CGEventTap (untrusted taps are silently limited to frontmost-only)
    /// and posting the Cmd+V paste into other apps.
    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    /// Check Accessibility trust and, if missing, show the native prompt that offers
    /// to open System Settings → Privacy & Security → Accessibility.
    pub fn prompt_accessibility() -> bool {
        macos_accessibility_client::accessibility::application_is_trusted_with_prompt()
    }

    /// Whether microphone access is authorized (so the Hub can show it accurately).
    pub fn microphone_granted() -> bool {
        use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
        unsafe {
            let Some(audio) = AVMediaTypeAudio else {
                return false;
            };
            let status = AVCaptureDevice::authorizationStatusForMediaType(audio);
            status == AVAuthorizationStatus::Authorized
        }
    }

    fn post_cmd_v() {
        unsafe {
            let down = CGEventCreateKeyboardEvent(null(), KEYCODE_V, true);
            CGEventSetFlags(down, KCG_FLAG_MASK_COMMAND);
            CGEventPost(KCG_HID_EVENT_TAP, down);
            CFRelease(down as *const c_void);

            let up = CGEventCreateKeyboardEvent(null(), KEYCODE_V, false);
            CGEventSetFlags(up, KCG_FLAG_MASK_COMMAND);
            CGEventPost(KCG_HID_EVENT_TAP, up);
            CFRelease(up as *const c_void);
        }
    }

    pub fn paste_text(text: &str) -> anyhow::Result<()> {
        use arboard::Clipboard;
        if !is_trusted() {
            return Err(anyhow::anyhow!(
                "no Accessibility permission - cannot paste (grant it in System Settings → \
                 Privacy & Security → Accessibility, then relaunch)"
            ));
        }
        let mut cb = Clipboard::new()?;
        let saved = cb.get_text().ok();
        cb.set_text(text.to_string())?;
        // Give the pasteboard a moment to settle before the paste keystroke.
        std::thread::sleep(Duration::from_millis(60));
        post_cmd_v();
        // Let the target consume the paste before we restore the old clipboard.
        std::thread::sleep(Duration::from_millis(150));
        crate::paste::restore_clipboard_after_paste(&mut cb, saved);
        crate::feedback::play_complete();
        Ok(())
    }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)] // prompt_* helpers kept for API parity with macOS
mod imp {
    use std::time::{Duration, Instant};

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_MENU, VK_SHIFT, VK_V,
    };

    /// Windows has no TCC-style Accessibility gate for SendInput from a normal
    /// integrity-level process. Elevated (admin) targets can still block UIPI;
    /// that surfaces as a failed paste, not a preflight flag.
    pub fn is_trusted() -> bool {
        true
    }

    pub fn prompt_accessibility() -> bool {
        true
    }

    /// Microphone privacy is enforced by the OS when cpal opens the device;
    /// there is no cheap synchronous grant check equivalent to macOS AVFoundation.
    pub fn microphone_granted() -> bool {
        true
    }

    /// Low-level keyboard hooks do not need a separate Input Monitoring grant.
    pub fn input_monitoring_granted() -> bool {
        true
    }

    pub fn request_input_monitoring() -> bool {
        true
    }

    fn key_event(vk: u16, up: bool) -> INPUT {
        let mut ki = KEYBDINPUT {
            wVk: VIRTUAL_KEY(vk),
            ..Default::default()
        };
        if up {
            ki.dwFlags = KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 { ki },
        }
    }

    fn mod_held(vk: VIRTUAL_KEY) -> bool {
        const KEY_DOWN_BIT: i16 = i16::MIN;
        unsafe { (GetAsyncKeyState(vk.0 as i32) & KEY_DOWN_BIT) != 0 }
    }

    /// Wait until modifier keys from the triggering hotkey are released so our
    /// synthesized Ctrl+V is not combined with leftover Ctrl/Alt/Shift.
    fn wait_modifiers_up(timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if !mod_held(VK_CONTROL) && !mod_held(VK_MENU) && !mod_held(VK_SHIFT) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn send_keys(inputs: &[INPUT]) -> bool {
        let n = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
        n == inputs.len() as u32
    }

    /// Clipboard + Ctrl+V, with Shift+Insert fallback for apps that ignore Ctrl+V.
    pub fn paste_text(text: &str) -> anyhow::Result<()> {
        use arboard::Clipboard;
        let mut cb = Clipboard::new()?;
        let saved = cb.get_text().ok();
        cb.set_text(text.to_string())?;
        wait_modifiers_up(Duration::from_millis(400));
        std::thread::sleep(Duration::from_millis(40));

        let ctrl_v = [
            key_event(VK_CONTROL.0, false),
            key_event(VK_V.0, false),
            key_event(VK_V.0, true),
            key_event(VK_CONTROL.0, true),
        ];
        let ok = send_keys(&ctrl_v);
        if !ok {
            let shift_insert = [
                key_event(VK_SHIFT.0, false),
                key_event(VK_INSERT.0, false),
                key_event(VK_INSERT.0, true),
                key_event(VK_SHIFT.0, true),
            ];
            if !send_keys(&shift_insert) {
                crate::paste::restore_clipboard_after_paste(&mut cb, saved.clone());
                return Err(anyhow::anyhow!(
                    "SendInput paste failed (target may be elevated or blocking injection)"
                ));
            }
        }

        std::thread::sleep(Duration::from_millis(150));
        crate::paste::restore_clipboard_after_paste(&mut cb, saved);
        crate::feedback::play_complete();
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use imp::{
    input_monitoring_granted, is_trusted, microphone_granted, paste_text, prompt_accessibility,
    request_input_monitoring,
};

#[cfg(target_os = "windows")]
#[allow(unused_imports)] // prompt_* / request_* used from lib.rs only on macOS today
pub use imp::{
    input_monitoring_granted, is_trusted, microphone_granted, paste_text, prompt_accessibility,
    request_input_monitoring,
};

// On Linux, text injection lives in `crate::linux::paste_text` (xdotool). These
// stubs keep the Hub permission surface compiling; hotkey paths call linux::paste.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn paste_text(text: &str) -> anyhow::Result<()> {
    crate::linux::paste_text(text)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn paste_text(_text: &str) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn is_trusted() -> bool {
    true
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[allow(dead_code)]
pub fn prompt_accessibility() -> bool {
    true
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn microphone_granted() -> bool {
    true
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn input_monitoring_granted() -> bool {
    true
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[allow(dead_code)]
pub fn request_input_monitoring() -> bool {
    true
}
