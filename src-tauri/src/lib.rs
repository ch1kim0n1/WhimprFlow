//! WhimprFlow Tauri shell.
//!
//! Runs as a macOS accessory (menu-bar) app: a tray item, a transparent
//! always-on-top Flow Bar overlay, and a hidden Hub window. This is the M0
//! skeleton  -  the sidecar supervisor, real state-machine bridge, and native
//! panel promotion arrive in later milestones. The overlay already listens for
//! `whimpr://flowbar/state`, so the tray demo items prove the event pipeline.

mod appctx;
mod asr_idle;
mod autolearn;
mod data_wipe;
mod feedback;
mod hotkey;
mod licensing;
#[cfg(target_os = "linux")]
mod linux;
mod local_llm;
mod logging;
mod mic_test;
mod models;
mod notes;
mod paste;
mod watchdog;
#[cfg(target_os = "windows")]
mod win;

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
        .or_else(|| {
            w.available_monitors()
                .ok()
                .and_then(|m| m.into_iter().next())
        });
    let Some(monitor) = monitor else {
        tracing::info!(target: "whimpr", "[whimpr] no monitor found  -  overlay stays at default position");
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
    tracing::info!(target: "whimpr",
        "[whimpr] overlay placed: monitor {}x{} @({},{}) scale {:.1} -> window {}x{} @({},{})",
        msize.width, msize.height, mpos.x, mpos.y, scale, wsize.width, wsize.height, x, y
    );
}

fn build_overlay(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    let overlay =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
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
        // Permit WebView downloads: the Studio "Export .md" button saves a blob,
        // which the WebView blocks unless a download handler accepts it.
        .on_download(|_, _| true)
        .build()
}

/// Render a keybinding chord for the tray shortcuts menu.
fn fmt_chord(c: &whimpr_core::Chord) -> String {
    let mut s = String::new();
    #[cfg(target_os = "macos")]
    {
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
    }
    #[cfg(not(target_os = "macos"))]
    {
        if c.ctrl {
            s.push_str("Ctrl+");
        }
        if c.alt {
            s.push_str("Alt+");
        }
        if c.shift {
            s.push_str("Shift+");
        }
        if c.meta {
            s.push_str("Win+");
        }
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
fn set_settings(mut settings: whimpr_core::Settings) -> Result<whimpr_core::Settings, String> {
    // Cloud cleanup requires an active license or trial.
    settings.cleanup_mode =
        licensing::gate_cleanup_mode(settings.cleanup_mode, licensing::cloud_cleanup_allowed());
    hotkey::update_settings(settings.clone());
    Ok(settings)
}

#[tauri::command]
fn get_entitlement() -> whimpr_core::Entitlement {
    licensing::current_entitlement()
}

#[tauri::command]
fn activate_license(key: String) -> Result<whimpr_core::Entitlement, String> {
    let ent = licensing::activate_license(&key)?;
    hotkey::rebuild_providers();
    Ok(ent)
}

#[tauri::command]
fn clear_license() -> Result<whimpr_core::Entitlement, String> {
    let ent = licensing::clear_license()?;
    hotkey::rebuild_providers();
    Ok(ent)
}

#[tauri::command]
fn start_trial() -> Result<whimpr_core::Entitlement, String> {
    let ent = licensing::start_trial()?;
    hotkey::rebuild_providers();
    Ok(ent)
}

/// Aggregated dictation stats for the Hub dashboard. `tz_offset_minutes` is the
/// browser's `Date.getTimezoneOffset()` so "today"/streak match the user's clock.
#[tauri::command]
fn get_stats(tz_offset_minutes: i32) -> whimpr_core::StatsSummary {
    hotkey::stats_summary(tz_offset_minutes)
}

/// Recent dictations for the Hub Home history list (newest first). `limit`
/// defaults to 200; the Studio Timeline search passes a higher cap so it
/// covers the full history, not just the newest page.
#[tauri::command]
fn get_history(limit: Option<usize>) -> Vec<whimpr_core::HistoryItem> {
    hotkey::history(limit.unwrap_or(200))
}

/// The Privacy pane's dictation ledger (newest first): every record, INCLUDING
/// textless ones (pruned or never stored), so provenance is auditable for
/// every dictation.
#[tauri::command]
fn get_ledger(limit: Option<usize>) -> Vec<whimpr_core::HistoryItem> {
    hotkey::ledger(limit.unwrap_or(200))
}

/// The workflow result currently held for approval, if any  -  lets the
/// Workflows pane seed itself on mount, since the `whimpr://pending` event is
/// fire-and-forget and may have fired before the pane existed.
#[tauri::command]
fn get_pending() -> Option<hotkey::PendingPayload> {
    hotkey::get_pending()
}

/// Dictionary entries for the Hub Dictionary screen.
#[tauri::command]
fn get_dictionary() -> Vec<hotkey::DictEntryDto> {
    hotkey::dictionary_entries()
}

/// Add a manual dictionary entry (word + optional known mishears). Each
/// mishear is also recorded in Voice Memory, so manual corrections land in
/// the same audit log as auto-learned ones.
#[tauri::command]
fn add_dictionary_entry(correct: String, mishears: Vec<String>) -> Result<(), String> {
    for mishear in &mishears {
        if !mishear.trim().is_empty() {
            hotkey::voice_memory_record(mishear.clone(), correct.clone(), "manual");
        }
    }
    hotkey::dictionary_add(correct, mishears)
}

/// Remove a dictionary entry by its spelling.
#[tauri::command]
fn remove_dictionary_entry(correct: String) -> Result<(), String> {
    hotkey::dictionary_remove(&correct)
}

/// Snippet entries for the Hub Snippets screen.
#[tauri::command]
fn get_snippets() -> Vec<whimpr_core::SnippetEntry> {
    hotkey::snippet_entries()
}

/// Add (or replace, if the trigger already exists) a voice-triggered text snippet.
#[tauri::command]
fn add_snippet(trigger: String, expansion: String) -> Result<(), String> {
    hotkey::snippet_add(trigger, expansion)
}

/// Remove a snippet by its trigger phrase.
#[tauri::command]
fn remove_snippet(trigger: String) -> Result<(), String> {
    hotkey::snippet_remove(&trigger)
}

/// Workflow entries for the Hub Workflows screen.
#[tauri::command]
fn get_workflows() -> Vec<whimpr_core::WorkflowEntry> {
    hotkey::workflow_entries()
}

#[tauri::command]
fn list_workflow_presets() -> Vec<whimpr_core::WorkflowPreset> {
    whimpr_core::workflow_presets()
}

#[tauri::command]
fn export_dictionary() -> Result<String, String> {
    let entries = hotkey::dictionary_entries();
    serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())
}

#[tauri::command]
fn import_dictionary(json: String, mode: String) -> Result<usize, String> {
    #[derive(serde::Deserialize)]
    struct EntryIn {
        correct: String,
        #[serde(default)]
        mishears: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Payload {
        Wrapped { entries: Vec<EntryIn> },
        Flat(Vec<EntryIn>),
    }
    let parsed: Payload = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let entries = match parsed {
        Payload::Wrapped { entries } => entries,
        Payload::Flat(entries) => entries,
    };
    if mode == "replace" {
        for e in hotkey::dictionary_entries() {
            let _ = hotkey::dictionary_remove(&e.correct);
        }
    }
    let mut n = 0usize;
    for e in entries {
        let correct = e.correct.trim().to_string();
        if correct.is_empty() {
            continue;
        }
        hotkey::dictionary_add(correct, e.mishears)?;
        n += 1;
    }
    Ok(n)
}

#[tauri::command]
fn validate_keybindings(kb: whimpr_core::KeyBindings) -> Vec<String> {
    whimpr_core::settings::validate_keybindings(&kb)
}

/// Add (or update, keyed by name) a voice workflow. An update bumps the version
/// and archives the prior revision.
#[tauri::command]
fn add_workflow(
    name: String,
    trigger: String,
    instruction: String,
    destination: whimpr_core::WorkflowDestination,
    require_approval: bool,
) -> Result<(), String> {
    hotkey::workflow_add(name, trigger, instruction, destination, require_approval)
}

/// Remove a workflow by its name.
#[tauri::command]
fn remove_workflow(name: String) -> Result<(), String> {
    hotkey::workflow_remove(&name)
}

/// Approve the workflow result currently held for approval (see the
/// `whimpr://pending` event): executes its destination now.
#[tauri::command]
fn approve_pending() {
    hotkey::approve_pending();
}

/// Discard the workflow result currently held for approval.
#[tauri::command]
fn reject_pending() {
    hotkey::reject_pending();
}

/// Pipeline health for the Hub's health chips (ASR/local LLM/permissions).
#[tauri::command]
fn get_health() -> hotkey::Health {
    hotkey::get_health()
}

/// Privacy: strip stored dictation text (final + raw) from every history
/// record, keeping numeric stats. Returns how many records were stripped.
#[tauri::command]
fn clear_history_text() -> usize {
    hotkey::clear_history_text()
}

/// Privacy: what the last Context Capsule contained  -  exactly what a cleanup
/// request would include. `None` until a capsule has been captured this run.
#[tauri::command]
fn get_last_capsule() -> Option<hotkey::CapsuleReport> {
    hotkey::get_last_capsule()
}

/// The Voice Memory correction audit list.
#[tauri::command]
fn get_voice_memory() -> Vec<whimpr_core::CorrectionEvent> {
    hotkey::get_voice_memory()
}

/// Export everything WhimprFlow has learned (corrections + dictionary +
/// snippets + style) as one plain-JSON bundle; returns the file's path.
#[tauri::command]
fn export_voice_memory() -> Result<String, String> {
    hotkey::export_voice_memory()
}

/// Wipe the Voice Memory correction log.
#[tauri::command]
fn clear_voice_memory() {
    hotkey::clear_voice_memory();
}

/// Screenshot into the app's captures folder; returns the image path
/// (macOS only in this pass).
#[tauri::command]
fn capture_screen() -> Result<String, String> {
    hotkey::capture_screen()
}

/// Notes (meeting transcripts, workflow notes, snap-notes), newest first.
#[tauri::command]
fn get_notes() -> Vec<notes::Note> {
    notes::entries()
}

/// Append a note. `image_path` links a captured screenshot (optional  -  the
/// Studio "Snap + note" flow passes the path `capture_screen` returned).
#[tauri::command]
fn add_note(title: String, text: String, image_path: Option<String>) {
    notes::add(title, text, image_path);
}

/// Remove a note by its timestamp.
#[tauri::command]
fn remove_note(ts_unix: u64) {
    notes::remove(ts_unix);
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

#[cfg(target_os = "windows")]
fn open_url(url: &str) {
    // `cmd /c start "" <uri>` launches ms-settings: / https: URIs via the shell.
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn();
}

/// Request microphone access: trigger the native prompt by briefly opening the
/// input device, and open the OS microphone privacy pane.
#[tauri::command]
fn request_microphone() {
    std::thread::spawn(|| {
        if let Ok(h) = whimpr_audio::start(|_: &[f32]| {}) {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let _ = h.stop();
        }
    });
    #[cfg(target_os = "macos")]
    open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
    #[cfg(target_os = "windows")]
    open_url("ms-settings:privacy-microphone");
}

/// Request Accessibility - on macOS this gates the Fn tap and paste into other
/// apps. On Windows there is no equivalent preflight grant for SendInput; open
/// the privacy landing page so users can review mic/speech permissions.
#[tauri::command]
fn request_accessibility() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::prompt_accessibility();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
    }
    #[cfg(target_os = "windows")]
    open_url("ms-settings:privacy-speech");
}

/// Request Input Monitoring (macOS Fn tap visibility). On Windows the LL hook
/// needs no separate grant - open keyboard settings as a helpful landing page.
#[tauri::command]
fn request_input_monitoring() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::request_input_monitoring();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent");
    }
    #[cfg(target_os = "windows")]
    open_url("ms-settings:easeofaccess-keyboard");
}

/// Called by the overlay pill's Stop button to end a locked hands-free session  -
/// the UI equivalent of the re-press-to-finalize hotkey transition. A no-op
/// unless a session is actually locked.
#[tauri::command]
fn confirm_dictation() -> Result<(), String> {
    hotkey::confirm_dictation();
    Ok(())
}

/// Called by the overlay pill's Cancel button (mirrors the Escape key) to
/// discard whatever dictation is currently in flight. A no-op when idle.
#[tauri::command]
fn cancel_dictation() -> Result<(), String> {
    hotkey::cancel_dictation();
    Ok(())
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

#[tauri::command]
fn list_backups() -> Result<Vec<String>, String> {
    hotkey::list_backups()
}

#[tauri::command]
fn restore_backup(backup_dir: String) -> Result<usize, String> {
    hotkey::restore_backup(backup_dir)
}

#[tauri::command]
fn list_model_offers() -> Vec<models::ModelOffer> {
    models::list_offers()
}

#[tauri::command]
fn asr_model_installed() -> bool {
    models::recommended_installed()
}

/// Download the recommended Whisper model with SHA-256 verification, then
/// reload ASR. Emits `whimpr://model/progress` while streaming.
#[tauri::command]
fn download_asr_model(app: tauri::AppHandle, model_id: Option<String>) -> Result<String, String> {
    models::download_recommended(app, model_id)
}

#[tauri::command]
fn reload_asr() -> Result<(), String> {
    hotkey::reload_asr();
    Ok(())
}

#[tauri::command]
fn dismiss_safe_mode() -> Result<(), String> {
    crate::watchdog::clear_launch_sentinel()
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
    let entry = keyring::Entry::new("com.whimpr.whimprflow", account).map_err(|e| e.to_string())?;
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

#[tauri::command]
fn export_diagnostics() -> Result<String, String> {
    logging::export_diagnostics()
}

#[tauri::command]
fn list_crash_reports() -> Vec<String> {
    logging::list_crash_reports()
}

#[tauri::command]
fn get_last_cold_start_ms() -> Option<u64> {
    logging::last_cold_start_ms()
}

#[tauri::command]
fn hub_ready() {
    logging::mark_hub_ready();
    let _ = crate::watchdog::clear_launch_sentinel();
}

#[derive(serde::Serialize)]
struct BuildInfo {
    version: &'static str,
    git_hash: &'static str,
}

#[tauri::command]
fn get_build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        git_hash: env!("WHIMPR_GIT_HASH"),
    }
}

#[tauri::command]
fn wipe_all_data(delete_models: bool) -> Result<(), String> {
    crate::data_wipe::wipe_all(delete_models)
}

#[tauri::command]
fn mic_self_test() -> Result<f32, String> {
    crate::mic_test::peak_rms_2s()
}

pub fn run() {
    logging::mark_process_start();
    logging::init();
    let _ = crate::watchdog::note_launch();
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            get_stats,
            get_history,
            get_ledger,
            get_pending,
            get_dictionary,
            add_dictionary_entry,
            remove_dictionary_entry,
            get_snippets,
            add_snippet,
            remove_snippet,
            get_workflows,
            list_workflow_presets,
            add_workflow,
            remove_workflow,
            export_dictionary,
            import_dictionary,
            validate_keybindings,
            approve_pending,
            reject_pending,
            get_health,
            clear_history_text,
            get_last_capsule,
            get_voice_memory,
            export_voice_memory,
            clear_voice_memory,
            capture_screen,
            get_notes,
            add_note,
            remove_note,
            get_status,
            request_microphone,
            request_accessibility,
            request_input_monitoring,
            set_api_key,
            confirm_dictation,
            cancel_dictation,
            test_command_edit,
            run_transform,
            backup_data,
            list_backups,
            restore_backup,
            list_model_offers,
            asr_model_installed,
            download_asr_model,
            reload_asr,
            get_entitlement,
            activate_license,
            clear_license,
            start_trial,
            export_diagnostics,
            get_last_cold_start_ms,
            hub_ready,
            get_build_info,
            wipe_all_data,
            mic_self_test,
            dismiss_safe_mode,
            list_crash_reports
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

            if crate::watchdog::in_safe_mode() {
                use tauri::Emitter;
                let _ = app.emit("whimpr://safe-mode", ());
            }

            // Wire the Fn key to the pill via the real state machine.
            hotkey::install(app.handle().clone());

            // Tray menu doubles as an at-a-glance popup of the active shortcuts,
            // built from the user's current keybindings.
            let kb = hotkey::current_settings().keybindings;
            let header =
                MenuItem::with_id(app, "hdr", "WhimprFlow Shortcuts", false, None::<&str>)?;
            let sep0 = PredefinedMenuItem::separator(app)?;
            #[cfg(target_os = "macos")]
            let (ptt_label, hf_label, cmd_label) = (
                "Push-to-talk:  Hold Fn",
                "Hands-free lock:  Double-tap Fn",
                "Command Mode:  Hold Fn+Ctrl",
            );
            #[cfg(target_os = "windows")]
            let (ptt_label, hf_label, cmd_label) = (
                "Push-to-talk:  Hold Right Ctrl",
                "Hands-free lock:  Double-tap Right Ctrl",
                "Command Mode:  Ctrl+Alt+Space (coming soon)",
            );
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let (ptt_label, hf_label, cmd_label) = (
                "Push-to-talk:  Hold trigger key",
                "Hands-free lock:  Double-tap trigger",
                "Command Mode:  see Settings",
            );
            let sc_ptt = MenuItem::with_id(app, "sc_ptt", ptt_label, true, None::<&str>)?;
            let sc_hf = MenuItem::with_id(app, "sc_hf", hf_label, true, None::<&str>)?;
            let sc_cmd = MenuItem::with_id(app, "sc_cmd", cmd_label, true, None::<&str>)?;
            let sc_cancel = MenuItem::with_id(
                app,
                "sc_cancel",
                format!("Cancel:  {}", fmt_chord(&kb.cancel)),
                true,
                None::<&str>,
            )?;
            let sc_paste = MenuItem::with_id(
                app,
                "sc_paste",
                format!("Paste last:  {}", fmt_chord(&kb.paste_last)),
                true,
                None::<&str>,
            )?;
            let sc_copy = MenuItem::with_id(
                app,
                "sc_copy",
                format!("Copy last:  {}", fmt_chord(&kb.copy_last)),
                true,
                None::<&str>,
            )?;
            let sc_undo = MenuItem::with_id(
                app,
                "sc_undo",
                format!("Undo cleanup:  {}", fmt_chord(&kb.undo_last)),
                true,
                None::<&str>,
            )?;
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
                    // Template image: macOS renders it monochrome and adapts it to
                    // the menu bar (white on dark). Not meaningful on Windows/Linux.
                    #[cfg(target_os = "macos")]
                    {
                        tray = tray.icon_as_template(true);
                    }
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
