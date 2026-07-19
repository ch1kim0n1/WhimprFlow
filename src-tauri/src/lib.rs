//! WhimprFlow Tauri shell.
//!
//! Runs as a macOS accessory (menu-bar) app: a tray item, a transparent
//! always-on-top Flow Bar overlay, and a hidden Hub window. This is the M0
//! skeleton  -  the sidecar supervisor, real state-machine bridge, and native
//! panel promotion arrive in later milestones. The overlay already listens for
//! `whimpr://flowbar/state`, so the tray demo items prove the event pipeline.

mod appctx;
mod autolearn;
mod hotkey;
mod local_llm;
mod paste;
#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "linux")]
mod linux;

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

const OVERLAY_LABEL: &str = "whimpr_bar";
const HUB_LABEL: &str = "main";

/// Anchor the overlay window bottom-center of its monitor.
fn position_overlay(w: &WebviewWindow) {
    // current_monitor() can be None before the window maps; fall back sensibly.
    let monitor = w
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| w.current_monitor().ok().flatten())
        .or_else(|| w.available_monitors().ok().and_then(|m| m.into_iter().next()));
    let Some(monitor) = monitor else {
        eprintln!("[whimpr] no monitor found  -  overlay stays at default position");
        return;
    };
    let scale = monitor.scale_factor();
    let msize = monitor.size();
    let mpos = monitor.position();
    let Ok(wsize) = w.outer_size() else { return };
    let inset = (40.0 * scale) as i32;
    let x = mpos.x + (msize.width as i32 - wsize.width as i32) / 2;
    let y = mpos.y + msize.height as i32 - wsize.height as i32 - inset;
    let _ = w.set_position(tauri::PhysicalPosition { x, y });
    eprintln!(
        "[whimpr] overlay placed: monitor {}x{} @({},{}) scale {:.1} -> window {}x{} @({},{})",
        msize.width, msize.height, mpos.x, mpos.y, scale, wsize.width, wsize.height, x, y
    );
}

fn build_overlay(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    let overlay = WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        WebviewUrl::App("overlay.html".into()),
    )
    .title("WhimprBar")
    // Tight window so it only catches clicks right around the pill, not a big
    // invisible box over the app behind it.
    .inner_size(300.0, 72.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .resizable(false)
    .visible(true)
    .build()?;
    position_overlay(&overlay);
    let _ = overlay.show();
    Ok(overlay)
}

fn build_hub(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, HUB_LABEL, WebviewUrl::App("index.html".into()))
        .title("WhimprFlow")
        .inner_size(920.0, 640.0)
        .min_inner_size(720.0, 480.0)
        .visible(true)
        .build()
}

/// Render a keybinding chord with macOS glyphs (for the tray shortcuts menu).
fn fmt_chord(c: &whimpr_core::Chord) -> String {
    let mut s = String::new();
    if c.ctrl {
        s.push('⌃');
    }
    if c.alt {
        s.push('⌥');
    }
    if c.shift {
        s.push('⇧');
    }
    if c.meta {
        s.push('⌘');
    }
    match c.key {
        whimpr_core::Key::Escape => s.push_str("Esc"),
        whimpr_core::Key::Char(ch) => s.push(ch.to_ascii_uppercase()),
    }
    s
}

#[tauri::command]
fn get_settings() -> whimpr_core::Settings {
    hotkey::current_settings()
}

#[tauri::command]
fn set_settings(settings: whimpr_core::Settings) {
    hotkey::update_settings(settings);
}

/// Aggregated dictation stats for the Hub dashboard. `tz_offset_minutes` is the
/// browser's `Date.getTimezoneOffset()` so "today"/streak match the user's clock.
#[tauri::command]
fn get_stats(tz_offset_minutes: i32) -> whimpr_core::StatsSummary {
    hotkey::stats_summary(tz_offset_minutes)
}

/// Recent dictations for the Hub Home history list (newest first).
#[tauri::command]
fn get_history() -> Vec<whimpr_core::HistoryItem> {
    hotkey::history(200)
}

/// Dictionary entries for the Hub Dictionary screen.
#[tauri::command]
fn get_dictionary() -> Vec<hotkey::DictEntryDto> {
    hotkey::dictionary_entries()
}

/// Add a manual dictionary entry (word + optional known mishears).
#[tauri::command]
fn add_dictionary_entry(correct: String, mishears: Vec<String>) {
    hotkey::dictionary_add(correct, mishears);
}

/// Remove a dictionary entry by its spelling.
#[tauri::command]
fn remove_dictionary_entry(correct: String) {
    hotkey::dictionary_remove(&correct);
}

/// Snippet entries for the Hub Snippets screen.
#[tauri::command]
fn get_snippets() -> Vec<whimpr_core::SnippetEntry> {
    hotkey::snippet_entries()
}

/// Add (or replace, if the trigger already exists) a voice-triggered text snippet.
#[tauri::command]
fn add_snippet(trigger: String, expansion: String) {
    hotkey::snippet_add(trigger, expansion);
}

/// Remove a snippet by its trigger phrase.
#[tauri::command]
fn remove_snippet(trigger: String) {
    hotkey::snippet_remove(&trigger);
}

/// Permission + capability status shown in the Hub.
#[derive(Clone, Serialize)]
struct StatusReport {
    accessibility: bool,
    microphone: bool,
    input_monitoring: bool,
    has_openai_key: bool,
    has_anthropic_key: bool,
}

#[tauri::command]
fn get_status() -> StatusReport {
    StatusReport {
        accessibility: paste::is_trusted(),
        microphone: paste::microphone_granted(),
        input_monitoring: paste::input_monitoring_granted(),
        has_openai_key: has_key("openai_api_key"),
        has_anthropic_key: has_key("anthropic_api_key"),
    }
}

fn has_key(account: &str) -> bool {
    keyring::Entry::new("com.whimpr.whimprflow", account)
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

/// Request microphone access: trigger the native prompt (bundle has a usage string)
/// by briefly opening the input device, and open the Microphone settings pane.
#[tauri::command]
fn request_microphone() {
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(|| {
            if let Ok(h) = whimpr_audio::start(|_: &[f32]| {}) {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let _ = h.stop();
            }
        });
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
    }
}

/// Request Accessibility  -  the permission that makes the Fn key work in every app and
/// lets us type into other apps. Fire the native prompt, then open the pane.
#[tauri::command]
fn request_accessibility() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::prompt_accessibility();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
    }
}

/// Request Input Monitoring (needed for the Fn key to be seen in every app, not
/// just while WhimprFlow is frontmost): register + prompt, then open the pane.
#[tauri::command]
fn request_input_monitoring() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::request_input_monitoring();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent");
    }
}

/// Called by the overlay pill's Stop button to end a locked hands-free session  -
/// the UI equivalent of the re-press-to-finalize hotkey transition. A no-op
/// unless a session is actually locked.
#[tauri::command]
fn confirm_dictation() {
    hotkey::confirm_dictation();
}

/// Called by the overlay pill's Cancel button (mirrors the Escape key) to
/// discard whatever dictation is currently in flight. A no-op when idle.
#[tauri::command]
fn cancel_dictation() {
    hotkey::cancel_dictation();
}

/// Manual Command Mode test hook: runs the instruction-following rewrite prompt
/// against `selection`/`instruction` through whichever cleanup provider is
/// currently configured, without needing to actually hold the Fn+Ctrl hotkey,
/// grant Accessibility, or dictate audio. macOS-only for now (mirrors
/// `hotkey::test_command_edit`); a full diff-viewer UI is out of scope for this
/// pass, this just proves the prompt + provider layer end to end.
#[tauri::command]
fn test_command_edit(selection: String, instruction: String) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        hotkey::test_command_edit(selection, instruction)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (selection, instruction);
        Err("Command Mode test hook is only implemented on macOS in this pass".to_string())
    }
}

/// Run a **Transform**: rewrite `text` per a canned `instruction` (e.g. "rewrite
/// this as a polished email") through whichever cleanup provider is configured.
/// Reuses the Command Mode prompt + provider path  -  a Transform is just Command
/// Mode with a preset instruction instead of a spoken one. macOS-only in this
/// pass, same as the Command Mode test hook it shares plumbing with.
#[tauri::command]
fn run_transform(text: String, instruction: String) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        hotkey::test_command_edit(text, instruction)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (text, instruction);
        Err("Transforms are only implemented on macOS in this pass".to_string())
    }
}

/// Copy settings/dictionary/snippets/stats into a fresh timestamped folder
/// under the app's support directory. Returns the created folder's path on
/// success so the UI can show the user exactly where it went.
#[tauri::command]
fn backup_data() -> Result<String, String> {
    hotkey::backup_data()
}

/// Save (or clear, when empty) an API key in the OS keychain, then rebuild providers
/// so it takes effect immediately.
#[tauri::command]
fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let account = match provider.as_str() {
        "openai" => "openai_api_key",
        "anthropic" => "anthropic_api_key",
        _ => return Err(format!("unknown provider {provider}")),
    };
    let entry =
        keyring::Entry::new("com.whimpr.whimprflow", account).map_err(|e| e.to_string())?;
    let key = key.trim();
    // Delete any existing item first so the new one is created by (and readable to)
    // this app  -  a key added via the `security` CLI isn't readable by the app.
    let _ = entry.delete_credential();
    if !key.is_empty() {
        entry.set_password(key).map_err(|e| e.to_string())?;
    }
    hotkey::rebuild_providers();
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            get_stats,
            get_history,
            get_dictionary,
            add_dictionary_entry,
            remove_dictionary_entry,
            get_snippets,
            add_snippet,
            remove_snippet,
            get_status,
            request_microphone,
            request_accessibility,
            request_input_monitoring,
            set_api_key,
            confirm_dictation,
            cancel_dictation,
            test_command_edit,
            run_transform,
            backup_data
        ])
        .setup(|app| {
            // Regular app: shows in the Dock with a normal, focusable main window.
            // (Can switch to a menu-bar-only accessory app later for the Wispr look.)
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            build_overlay(app)?;
            let hub = build_hub(app)?;
            let _ = hub.show();
            let _ = hub.set_focus();

            // Wire the Fn key to the pill via the real state machine.
            hotkey::install(app.handle().clone());

            // Tray menu doubles as an at-a-glance popup of the active shortcuts,
            // built from the user's current keybindings.
            let kb = hotkey::current_settings().keybindings;
            let header = MenuItem::with_id(app, "hdr", "WhimprFlow Shortcuts", false, None::<&str>)?;
            let sep0 = PredefinedMenuItem::separator(app)?;
            let sc_ptt = MenuItem::with_id(app, "sc_ptt", "Push-to-talk:  Hold Fn", true, None::<&str>)?;
            let sc_hf = MenuItem::with_id(app, "sc_hf", "Hands-free lock:  Double-tap Fn", true, None::<&str>)?;
            let sc_cmd = MenuItem::with_id(app, "sc_cmd", "Command Mode:  Hold Fn+Ctrl", true, None::<&str>)?;
            let sc_cancel = MenuItem::with_id(app, "sc_cancel", &format!("Cancel:  {}", fmt_chord(&kb.cancel)), true, None::<&str>)?;
            let sc_paste = MenuItem::with_id(app, "sc_paste", &format!("Paste last:  {}", fmt_chord(&kb.paste_last)), true, None::<&str>)?;
            let sc_copy = MenuItem::with_id(app, "sc_copy", &format!("Copy last:  {}", fmt_chord(&kb.copy_last)), true, None::<&str>)?;
            let sc_undo = MenuItem::with_id(app, "sc_undo", &format!("Undo cleanup:  {}", fmt_chord(&kb.undo_last)), true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let open = MenuItem::with_id(app, "open", "Open WhimprFlow", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit WhimprFlow", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &header, &sep0, &sc_ptt, &sc_hf, &sc_cmd, &sc_cancel, &sc_paste, &sc_copy,
                    &sc_undo, &sep1, &open, &quit,
                ],
            )?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" | "sc_cancel" | "sc_paste" | "sc_copy" | "sc_undo" => {
                        if let Some(w) = app.get_webview_window(HUB_LABEL) {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
                Ok(img) => {
                    tray = tray.icon(img);
                }
                Err(_) => {
                    if let Some(icon) = app.default_window_icon().cloned() {
                        tray = tray.icon(icon);
                    }
                }
            }
            tray.build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running WhimprFlow");
}
