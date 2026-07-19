//! Windows platform layer for WhimprFlow: a low-level keyboard hook for
//! push-to-talk, clipboard+SendInput text injection, and foreground-app detection,
//! plus the same dictation pipeline (audio → Whisper ASR → cleanup LLM → paste) and
//! the Hub-facing settings/stats/dictionary functions the Tauri commands call.
//!
//! ⚠️ UNVERIFIED: this module was written on macOS and has **never been compiled or
//! run on Windows**. The shared crates (audio, ASR, cleanup, core) are
//! cross-platform, but this Win32 glue will almost certainly need fixes before it
//! builds and runs. It is `cfg(target_os = "windows")` so it does not affect  -  and
//! is not checked by  -  the macOS build. Treat it as a starting point, not a
//! shipping port. Default push-to-talk key: Right Ctrl.
//!
//! **Hands-free double-tap-lock is a deliberate simplification here**, not a port of
//! `whimpr_core::StateMachine` (which macOS's `hotkey.rs` drives). This module has no
//! session-cap warning, no `AwaitingLock`-discards-the-tap semantics, and no cooldown
//! debounce  -  it just layers a minimal lock on top of the existing RECORDING-boolean
//! toggle: a key-down within `DOUBLE_TAP_MS` of the previous key-up sets `LOCKED` and
//! the *next* key-down (or a `confirm_dictation` UI stop click) finalizes instead of
//! toggling recording off on release. Bringing this module to full state-machine
//! parity with `hotkey.rs` is a larger rewrite left for a follow-up.
//!
//! One consequence of not replicating `AwaitingLock`: the first (short) tap of a
//! double-tap here still finalizes and pastes its own (likely near-empty) capture
//! before the second press locks  -  macOS instead discards that tap's audio. This
//! module accepts that minor extra paste/no-op as part of the simplification above.

#![cfg(target_os = "windows")]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RWIN, VK_SHIFT, VK_SPACE, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW,
    HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use whimpr_core::state::timing::DOUBLE_TAP_MS;
use whimpr_core::{AsrEngine, CleanupContext, CleanupMode, CleanupProvider, StatsSummary};

const OVERLAY_LABEL: &str = "whimpr_bar";
/// Push-to-talk key. Right Ctrl by default (Ctrl+Win chords land in a later pass).
const PTT_VK: u16 = VK_RCONTROL.0;
/// Command Mode hotkey: Ctrl+Alt+Space  -  mirrors macOS's Fn+Ctrl chord (see
/// `hotkey.rs`'s `tap_callback`). Detection-only scaffold on this platform; see
/// `command_mode_edit` below for why it does not attempt real selection
/// read/write here. Not user-rebindable this pass (see `whimpr_core::KeyBindings`'s
/// doc comment for why Command Mode isn't in that struct).
const COMMAND_MODE_VK: u16 = VK_SPACE.0;

/// Windows virtual-key code for a bindable [`whimpr_core::Key`]. For `Key::Char`,
/// the VK code for `'A'..='Z'`/`'0'..='9'` is literally the ASCII value, per the
/// Win32 `Virtual-Key Codes` reference  -  no lookup table needed there.
fn vk_for_key(key: whimpr_core::Key) -> u16 {
    use whimpr_core::Key;
    match key {
        Key::Escape => VK_ESCAPE.0,
        Key::Char(c) => c.to_ascii_uppercase() as u16,
    }
}

/// Whether the live modifier-key state matches a [`whimpr_core::Chord`] exactly
/// (all four flags, not "at least these"). Uses `GetAsyncKeyState` (not
/// `GetKeyState`) because the low-level keyboard hook thread has no normal
/// message queue for `GetKeyState`'s "last message processed" state to reflect.
fn mods_match_chord(chord: &whimpr_core::Chord) -> bool {
    const KEY_DOWN_BIT: i16 = i16::MIN; // high bit of GetAsyncKeyState's result
    let held = |vk: VIRTUAL_KEY| unsafe { (GetAsyncKeyState(vk.0 as i32) & KEY_DOWN_BIT) != 0 };
    let ctrl = held(VK_CONTROL);
    let alt = held(VK_MENU);
    let shift = held(VK_SHIFT);
    let meta = held(VK_LWIN) || held(VK_RWIN);
    ctrl == chord.ctrl && alt == chord.alt && shift == chord.shift && meta == chord.meta
}

/// The current keybindings, read fresh so a rebind saved from the Shortcuts UI
/// takes effect immediately  -  no relaunch or hook reinstall needed.
fn current_keybindings() -> whimpr_core::KeyBindings {
    SETTINGS
        .get()
        .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).keybindings)
        .unwrap_or_default()
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static CLOCK: OnceLock<Instant> = OnceLock::new();
static RECORDING: AtomicBool = AtomicBool::new(false);
/// Set while a hands-free (double-tap-locked) session is open. While true, a
/// key-up does NOT stop capture  -  only the next key-down (the "third press") or
/// a `confirm_dictation` UI stop click does. See the module doc comment for how
/// this minimal lock differs from macOS's full state-machine-driven behavior.
static LOCKED: AtomicBool = AtomicBool::new(false);
/// `now_ms()` timestamp of the most recent push-to-talk key-up, or `u64::MAX` if
/// there hasn't been one yet this run. Compared against `DOUBLE_TAP_MS` on the next
/// key-down to detect a double-tap-to-lock.
static LAST_KEY_UP_MS: AtomicU64 = AtomicU64::new(u64::MAX);
/// Debounce state for the letter-key hotkeys above: latched true on the initial
/// keydown so held-key auto-repeat doesn't refire the action, cleared on keyup.
/// Named by action (not by physical key) since the key each is bound to can
/// now change at runtime via the Shortcuts UI.
static CANCEL_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static PASTE_LAST_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static COPY_LAST_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static UNDO_LAST_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static COMMAND_MODE_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static CAPTURE: OnceLock<Mutex<Option<whimpr_audio::CaptureHandle>>> = OnceLock::new();
static ASR: OnceLock<Arc<whimpr_asr::WhisperEngine>> = OnceLock::new();
static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
static OPENAI: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>> = OnceLock::new();
static SETTINGS: OnceLock<Mutex<whimpr_core::Settings>> = OnceLock::new();
static DICTIONARY: OnceLock<Mutex<whimpr_core::DictionaryStore>> = OnceLock::new();
static SNIPPETS: OnceLock<Mutex<whimpr_core::SnippetStore>> = OnceLock::new();
static STATS: OnceLock<Mutex<whimpr_core::StatsStore>> = OnceLock::new();
/// (raw pre-cleanup text, final pasted text) from the most recent dictation  -
/// feeds the "undo last cleanup edit" hotkey (Ctrl+Alt+Z). `None` until the first
/// dictation completes this run.
static LAST_TEXTS: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();

fn support_dir() -> std::path::PathBuf {
    // %APPDATA%\WhimprFlow
    let base = std::env::var("APPDATA").unwrap_or_default();
    std::path::PathBuf::from(base).join("WhimprFlow")
}
fn settings_path() -> std::path::PathBuf {
    support_dir().join("settings.json")
}
fn dict_path() -> std::path::PathBuf {
    support_dir().join("dictionary.json")
}
fn snippets_path() -> std::path::PathBuf {
    support_dir().join("snippets.json")
}
fn stats_path() -> std::path::PathBuf {
    support_dir().join("stats.json")
}

/// Copy settings/dictionary/snippets/stats into a timestamped folder under
/// `support_dir()/backups/`. Mirrors `hotkey.rs`'s `backup_data`.
pub fn backup_data() -> Result<String, String> {
    whimpr_core::backup::backup_files(
        &[
            ("settings.json", settings_path()),
            ("dictionary.json", dict_path()),
            ("snippets.json", snippets_path()),
            ("stats.json", stats_path()),
        ],
        &support_dir().join("backups"),
    )
    .map(|p| p.display().to_string())
    .map_err(|e| e.to_string())
}

/// `.en`-suffixed models are English-only, so when a specific non-English
/// language is selected we only consider multilingual model files (no `.en`
/// suffix); otherwise `.en` models are preferred first for better English
/// accuracy, falling back to multilingual files if none are present.
fn whisper_model_path(language: Option<&str>) -> std::path::PathBuf {
    let dir = support_dir().join("models");
    let needs_multilingual = matches!(language, Some(lang) if lang != "en");
    let candidates: &[&str] = if needs_multilingual {
        &["ggml-medium.bin", "ggml-small.bin", "ggml-base.bin"]
    } else {
        &[
            "ggml-medium.en.bin",
            "ggml-small.en.bin",
            "ggml-base.en.bin",
            "ggml-medium.bin",
            "ggml-small.bin",
            "ggml-base.bin",
        ]
    };
    for name in candidates {
        let p = dir.join(name);
        if p.exists() {
            return p;
        }
    }
    dir.join(candidates.last().copied().unwrap_or("ggml-base.en.bin"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    CLOCK.get().map(|c| c.elapsed().as_millis() as u64).unwrap_or(0)
}

fn emit_bar(state: &'static str) {
    if let Some(app) = APP.get() {
        #[derive(Clone, serde::Serialize)]
        struct P {
            state: &'static str,
        }
        let _ = app.emit_to(OVERLAY_LABEL, "whimpr://flowbar/state", P { state });
    }
}

/// The foreground process's executable name (e.g. "chrome.exe"), for per-app
/// cleanup formatting  -  the Windows analogue of the macOS bundle id.
fn foreground_app() -> Option<String> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle =
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
        let mut buf = [0u16; 260];
        let len = GetModuleBaseNameW(handle, None, &mut buf);
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

// ── Text injection: clipboard + Ctrl+V via SendInput ────────────────────────────

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

pub fn paste_text(text: &str) -> anyhow::Result<()> {
    use arboard::Clipboard;
    let mut cb = Clipboard::new()?;
    let saved = cb.get_text().ok();
    cb.set_text(text.to_string())?;
    std::thread::sleep(Duration::from_millis(60));
    let inputs = [
        key_event(VK_CONTROL.0, false),
        key_event(VK_V.0, false),
        key_event(VK_V.0, true),
        key_event(VK_CONTROL.0, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    std::thread::sleep(Duration::from_millis(150));
    if let Some(prev) = saved {
        let _ = cb.set_text(prev);
    }
    Ok(())
}

// ── Cleanup (shared, cross-platform building blocks) ────────────────────────────

fn current_settings_inner() -> whimpr_core::Settings {
    SETTINGS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default()
}

/// Returns `(raw_out, final_text)`: `raw_out` is the pre-cleanup transcript after
/// `pre_normalize_layout`/`post_process` (what would be pasted if cleanup were
/// skipped or rejected  -  used by the "undo last cleanup edit" hotkey to restore the
/// un-cleaned text), and `final_text` is what actually gets pasted.
fn clean_transcript(raw: &str) -> (String, String) {
    let settings = current_settings_inner();
    let level = settings.cleanup_level;
    if matches!(settings.cleanup_mode, CleanupMode::Raw) || level.bypasses_llm() {
        return (raw.to_string(), raw.to_string());
    }
    let raw_norm = whimpr_core::cleanup::pre_normalize_layout(raw);
    let raw_out = whimpr_core::cleanup::post_process(&raw_norm);
    let vocab = DICTIONARY
        .get()
        .map(|d| d.lock().unwrap_or_else(|e| e.into_inner()).prefilter(&raw_norm, 15))
        .unwrap_or_default();
    let ctx = CleanupContext {
        level,
        vocab,
        app_bundle_id: foreground_app(),
        style: settings.style.to_instructions(),
        ..Default::default()
    };
    let run_local = || -> Option<anyhow::Result<String>> {
        LOCAL.get().and_then(|m| {
            m.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|w| {
                let messages = whimpr_core::cleanup::build_messages(&raw_norm, &ctx);
                w.cleanup(&messages)
            })
        })
    };
    let result = match settings.cleanup_mode {
        CleanupMode::OpenAi => OPENAI
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.cleanup(&raw_norm, &ctx)))
            .or_else(run_local),
        CleanupMode::Local => run_local(),
        _ => run_local(),
    };
    let final_text = match result {
        Some(Ok(cleaned)) => {
            let cleaned = whimpr_core::cleanup::post_process(&cleaned);
            if whimpr_core::cleanup::evaluate_gates(&raw_out, &cleaned, level).passed() {
                cleaned
            } else {
                raw_out.clone()
            }
        }
        _ => raw_out.clone(),
    };
    (raw_out, final_text)
}

fn record_dictation(text: &str, duration_secs: f32, app: Option<String>) {
    let words = whimpr_core::stats::count_words(text);
    if words == 0 {
        return;
    }
    if let Some(m) = STATS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        let duration_ms = (duration_secs.max(0.0) * 1000.0) as u32;
        let chars = text.chars().count() as u32;
        store.record(words, duration_ms, chars, unix_now(), text.to_string(), app);
        let _ = store.save(&stats_path());
    }
}

// ── The push-to-talk pipeline ───────────────────────────────────────────────────
//
// Hands-free double-tap-lock (minimal, see the module doc comment for why this
// isn't a `whimpr_core::StateMachine` port): a key-down within `DOUBLE_TAP_MS` of
// the previous key-up sets `LOCKED` instead of starting a normal push-to-talk
// hold; while `LOCKED`, key-up no longer stops capture, and the next key-down (or
// a `confirm_dictation` UI stop click) finalizes the session instead.

fn on_ptt_down() {
    if LOCKED.load(Ordering::SeqCst) {
        // Third press: finalize the locked hands-free session (same path a
        // Stop-button click reaches via `confirm_dictation`).
        finalize_locked_session();
        return;
    }
    if RECORDING.swap(true, Ordering::SeqCst) {
        return; // already recording
    }
    let now = now_ms();
    let last_up = LAST_KEY_UP_MS.load(Ordering::SeqCst);
    let is_double_tap = last_up != u64::MAX && now.saturating_sub(last_up) <= DOUBLE_TAP_MS;
    if is_double_tap {
        LOCKED.store(true, Ordering::SeqCst);
        emit_bar("locked");
    } else {
        emit_bar("recording");
    }
    std::thread::spawn(|| match whimpr_audio::start(|_: &[f32]| {}) {
        Ok(handle) => {
            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        }
        Err(e) => eprintln!("[whimpr:win] mic capture failed: {e}"),
    });
}

fn on_ptt_up() {
    if LOCKED.load(Ordering::SeqCst) {
        // Locked: releasing the key must not stop capture, but the release
        // timestamp isn't needed for anything further (the lock only clears via
        // `finalize_locked_session`)  -  nothing to record here.
        return;
    }
    if !RECORDING.swap(false, Ordering::SeqCst) {
        return; // wasn't recording
    }
    LAST_KEY_UP_MS.store(now_ms(), Ordering::SeqCst);
    emit_bar("idle");
    finish_capture_and_paste(foreground_app());
}

/// Ends a locked hands-free session: reached via a third key-down (see
/// `on_ptt_down` above) or a `confirm_dictation` UI stop click. Mirrors
/// `on_ptt_up`'s finalize path but is reachable without a matching key-up.
fn finalize_locked_session() {
    LOCKED.store(false, Ordering::SeqCst);
    RECORDING.store(false, Ordering::SeqCst);
    emit_bar("idle");
    finish_capture_and_paste(foreground_app());
}

/// Stop the current capture, transcribe, clean up, and paste. Shared by the
/// normal push-to-talk release path (`on_ptt_up`) and the locked-session
/// finalize path (`finalize_locked_session`).
fn finish_capture_and_paste(app: Option<String>) {
    let handle = CAPTURE.get().and_then(|slot| slot.lock().unwrap_or_else(|e| e.into_inner()).take());
    std::thread::spawn(move || {
        let Some(res) = handle.and_then(|h| h.stop()) else {
            return;
        };
        let Some(asr) = ASR.get().cloned() else {
            return;
        };
        let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
        let language = current_settings_inner().language;
        if let Ok(t) = asr.transcribe(&pcm, language.as_deref()) {
            let raw = t.text;
            // Static snippets are checked first, on the raw transcript, before
            // cleanup runs. A match pastes the expansion verbatim and skips the
            // whole cleanup pipeline (no LLM call, no gates).
            let snippet_expansion = SNIPPETS.get().and_then(|m| {
                m.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .find_match(&raw)
                    .map(|entry| entry.expansion.clone())
            });
            let (raw_out, text) = match snippet_expansion {
                Some(expansion) => (expansion.clone(), expansion),
                None => clean_transcript(&raw),
            };
            if !text.is_empty() {
                if let Err(e) = paste_text(&text) {
                    eprintln!("[whimpr:win] paste failed: {e}");
                }
                // Stash (raw, final) for the "undo last cleanup edit" hotkey
                // (Ctrl+Alt+Z), right after the paste.
                *LAST_TEXTS
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some((raw_out, text.clone()));
                record_dictation(&text, res.duration_secs(), app);
            }
        }
    });
}

/// Called by the overlay pill's Stop button (`confirm_dictation` Tauri command)
/// to end a locked hands-free session  -  the UI equivalent of the third
/// key-down in `on_ptt_down`. A no-op unless a session is actually locked.
pub fn confirm_dictation() {
    if LOCKED.load(Ordering::SeqCst) {
        finalize_locked_session();
    }
}

/// Called by the overlay pill's Cancel button (`cancel_dictation` Tauri
/// command) *or* the user's configured Cancel hotkey (`hook_proc`'s dynamic
/// binding check, default bare Escape) to discard whatever dictation is in
/// flight  -  locked or a normal push-to-talk hold  -  without transcribing it.
/// A no-op when idle.
pub fn cancel_dictation() {
    let was_locked = LOCKED.swap(false, Ordering::SeqCst);
    let was_recording = RECORDING.swap(false, Ordering::SeqCst);
    if !was_locked && !was_recording {
        return; // nothing to cancel
    }
    emit_bar("cancelled");
    if let Some(slot) = CAPTURE.get() {
        if let Some(handle) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = handle.stop(); // discard  -  do not transcribe
        }
    }
    // Let the "cancelled" tick linger briefly before returning to idle, mirroring
    // the macOS pill (see `hotkey.rs`'s `apply_action` for `BarState::Done`).
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(500));
        emit_bar("idle");
    });
}

// ── "Paste/copy last transcript" (Ctrl+Alt+V / Ctrl+Alt+C) and "undo last
//    cleanup edit" (Ctrl+Alt+Z) hotkeys ──────────────────────────────────────

/// The most recent dictation's text, if any.
fn latest_transcript() -> Option<String> {
    STATS
        .get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).latest().map(|r| r.text.clone()))
}

/// True when both Ctrl and Alt are currently held  -  Command Mode's chord isn't
/// user-rebindable this pass, so it keeps its own simple check rather than
/// going through `mods_match_chord`/`whimpr_core::Chord`.
fn ctrl_alt_held() -> bool {
    const KEY_DOWN_BIT: i16 = i16::MIN; // high bit of GetAsyncKeyState's result
    unsafe {
        (GetAsyncKeyState(VK_CONTROL.0 as i32) & KEY_DOWN_BIT) != 0
            && (GetAsyncKeyState(VK_MENU.0 as i32) & KEY_DOWN_BIT) != 0
    }
}

/// Re-paste the most recently dictated transcript into the foreground app.
fn paste_last_transcript() {
    match latest_transcript() {
        Some(text) if !text.is_empty() => {
            eprintln!("[whimpr:win] hotkey: paste last transcript");
            if let Err(e) = paste_text(&text) {
                eprintln!("[whimpr:win] paste-last-transcript failed: {e}");
            }
        }
        _ => eprintln!("[whimpr:win] paste-last-transcript: no transcript yet"),
    }
}

/// Copy the most recently dictated transcript to the clipboard without pasting it.
fn copy_last_transcript() {
    match latest_transcript() {
        Some(text) if !text.is_empty() => {
            eprintln!("[whimpr:win] hotkey: copy last transcript");
            use arboard::Clipboard;
            if let Err(e) = Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                eprintln!("[whimpr:win] copy-last-transcript failed: {e}");
            }
        }
        _ => eprintln!("[whimpr:win] copy-last-transcript: no transcript yet"),
    }
}

/// Re-paste the raw (pre-cleanup) transcript from the most recent dictation,
/// undoing the LLM cleanup edit. No-ops if cleanup made no change (nothing to
/// undo) or nothing has been dictated yet this run.
///
/// v1 simplification: this pastes the raw text as a NEW insertion at the current
/// cursor position  -  it does not attempt to find-and-replace the previously
/// pasted cleaned text in place (see perfect-todo.md item 3).
fn undo_last_cleanup() {
    let pair = LAST_TEXTS.get().and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
    match pair {
        Some((raw, final_pasted)) if raw != final_pasted => {
            eprintln!("[whimpr:win] hotkey: undo last cleanup edit");
            if let Err(e) = paste_text(&raw) {
                eprintln!("[whimpr:win] undo-last-cleanup failed: {e}");
            }
        }
        Some(_) => {
            eprintln!("[whimpr:win] undo-last-cleanup: cleanup made no changes, nothing to undo")
        }
        None => eprintln!("[whimpr:win] undo-last-cleanup: no transcript yet"),
    }
}

// ── Command Mode (Ctrl+Alt+Space): select text, hold, speak an edit instruction,
//    release to have the selection rewritten in place ──────────────────────────
//
// ⚠️ STUB, NOT IMPLEMENTED on Windows. The macOS path (`hotkey.rs`) reads and
// writes the focused element's text selection via the Accessibility API
// (`AXUIElementCopyAttributeValue`/`AXUIElementSetAttributeValue` on
// `kAXSelectedTextAttribute`). The Win32 analogue is UI Automation  -
// `IUIAutomation::GetFocusedElement`, `ITextPattern`/`ITextPattern2` /
// `ITextRangeProvider` to read the current selection range and `SetValue`/
// `RemoveFromSelection`+insert to replace it  -  a materially different API
// surface (COM-based, not a small C FFI) with its own per-app quirks. Wiring
// that blind, on a machine that has never run Windows to verify against, risks
// shipping code that silently corrupts a user's selected text in an arbitrary
// third-party app  -  worse than doing nothing. This function only recognizes the
// hotkey chord and reports the gap; a real UIA implementation needs to be built
// and verified ON Windows hardware in a follow-up pass.
fn command_mode_edit() -> anyhow::Result<()> {
    eprintln!(
        "[whimpr:win] Command Mode not yet implemented on Windows (needs UI Automation \
         selection read/write, unverified without Windows hardware)  -  no-op"
    );
    anyhow::bail!(
        "Command Mode not yet implemented on Windows (needs UI Automation selection \
         read/write, unverified without Windows hardware)"
    )
}

// ── Low-level keyboard hook ─────────────────────────────────────────────────────

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode as u16;
        if vk == PTT_VK {
            match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => on_ptt_down(),
                WM_KEYUP | WM_SYSKEYUP => on_ptt_up(),
                _ => {}
            }
        } else {
            // Read the user's current bindings fresh on every keypress (cheap  -
            // a Mutex lock over a small Copy struct) so a rebind saved from the
            // Shortcuts UI takes effect immediately, no relaunch or hook
            // reinstall needed.
            let bindings = current_keybindings();
            // (latch, vk-for-this-binding, mods-required, the action to run).
            // Cancel has no OS-level hook on Windows until now  -  this is a new
            // capability, not just a port of the existing three.
            let candidates: [(&AtomicBool, u16, whimpr_core::Chord, fn()); 4] = [
                (&CANCEL_KEY_DOWN, vk_for_key(bindings.cancel.key), bindings.cancel, cancel_dictation),
                (&PASTE_LAST_KEY_DOWN, vk_for_key(bindings.paste_last.key), bindings.paste_last, paste_last_transcript),
                (&COPY_LAST_KEY_DOWN, vk_for_key(bindings.copy_last.key), bindings.copy_last, copy_last_transcript),
                (&UNDO_LAST_KEY_DOWN, vk_for_key(bindings.undo_last.key), bindings.undo_last, undo_last_cleanup),
            ];
            for (latch, bound_vk, chord, action) in candidates {
                if vk != bound_vk {
                    continue;
                }
                match wparam.0 as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        // swap(true) returns the prior state  -  only act on the
                        // leading edge (not-down -> down) so held-key
                        // auto-repeat fires once.
                        if !latch.swap(true, Ordering::SeqCst) && mods_match_chord(&chord) {
                            action();
                        }
                    }
                    WM_KEYUP | WM_SYSKEYUP => latch.store(false, Ordering::SeqCst),
                    _ => {}
                }
            }
        }
        if vk == COMMAND_MODE_VK {
            match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    // swap(true) returns the prior state  -  only act on the leading
                    // edge so held-key auto-repeat doesn't refire the (stub) action.
                    if !COMMAND_MODE_KEY_DOWN.swap(true, Ordering::SeqCst) && ctrl_alt_held() {
                        let _ = command_mode_edit();
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => COMMAND_MODE_KEY_DOWN.store(false, Ordering::SeqCst),
                _ => {}
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Install the hook on a dedicated thread with its own message pump (required for
/// WH_KEYBOARD_LL to deliver events).
fn spawn_hook_thread() {
    std::thread::spawn(|| unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hinst, 0);
        if hook.is_err() {
            eprintln!("[whimpr:win] failed to install keyboard hook");
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {}
    });
}

// ── Public surface (mirrors the macOS `hotkey::` functions the commands call) ────

pub fn install(app: AppHandle) {
    let _ = APP.set(app);
    let _ = CLOCK.set(Instant::now());
    let settings = whimpr_core::Settings::load(&settings_path());
    let language_for_model = settings.language.clone();
    let _ = SETTINGS.set(Mutex::new(settings));
    let _ = DICTIONARY.set(Mutex::new(whimpr_core::DictionaryStore::load(&dict_path())));
    let _ = SNIPPETS.set(Mutex::new(whimpr_core::SnippetStore::load(&snippets_path())));
    let _ = STATS.set(Mutex::new(whimpr_core::StatsStore::load(&stats_path())));
    let _ = OPENAI.set(Mutex::new(None));
    let _ = LOCAL.set(Mutex::new(None));
    rebuild_providers();

    // Load Whisper.
    std::thread::spawn(move || {
        match whimpr_asr::WhisperEngine::load(&whisper_model_path(language_for_model.as_deref())) {
            Ok(engine) => {
                let _ = ASR.set(Arc::new(engine));
                eprintln!("[whimpr:win] ASR ready");
            }
            Err(e) => eprintln!("[whimpr:win] ASR load failed: {e}"),
        }
    });
    // Start the local cleanup worker.
    std::thread::spawn(|| {
        if let Some(w) = crate::local_llm::spawn_default() {
            if let Some(slot) = LOCAL.get() {
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
            }
        }
    });

    spawn_hook_thread();
    eprintln!(
        "[whimpr:win] keyboard hook installed (push-to-talk: Right Ctrl, fixed; cancel/paste-last/\
         copy-last/undo-last: see Settings → Shortcuts for current bindings, defaults Ctrl+Alt+V/C/Z; \
         Command Mode: Ctrl+Alt+Space [stub, not implemented  -  see command_mode_edit], fixed)"
    );
}

pub fn current_settings() -> whimpr_core::Settings {
    current_settings_inner()
}

pub fn update_settings(new: whimpr_core::Settings) {
    if let Some(m) = SETTINGS.get() {
        *m.lock().unwrap_or_else(|e| e.into_inner()) = new.clone();
    }
    let _ = new.save(&settings_path());
    rebuild_providers();
}

pub fn rebuild_providers() {
    let settings = current_settings_inner();
    let model = settings.openai_model;
    let base_url = settings.openai_base_url;
    let key = keyring::Entry::new("com.whimpr.whimprflow", "openai_api_key")
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|k| !k.trim().is_empty());
    if let Some(slot) = OPENAI.get() {
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = key.map(|k| {
            whimpr_cleanup::OpenAiProvider::with_base_url(k, model, Some(base_url))
        });
    }
}

pub fn stats_summary(tz_offset_minutes: i32) -> StatsSummary {
    STATS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).summary(tz_offset_minutes, unix_now()))
        .unwrap_or_else(|| whimpr_core::StatsStore::default().summary(tz_offset_minutes, unix_now()))
}

pub fn history(limit: usize) -> Vec<whimpr_core::HistoryItem> {
    STATS.get().map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).history(limit)).unwrap_or_default()
}

pub fn dictionary_entries() -> Vec<crate::hotkey::DictEntryDto> {
    DICTIONARY
        .get()
        .map(|m| {
            m.lock()
                .unwrap_or_else(|e| e.into_inner())
                .entries
                .iter()
                .map(|e| crate::hotkey::DictEntryDto {
                    correct: e.correct.clone(),
                    mishears: e.mishears.clone(),
                    auto: matches!(e.source, whimpr_core::DictSource::Auto),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn dictionary_add(correct: String, mishears: Vec<String>) {
    if let Some(m) = DICTIONARY.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        store.add(correct, mishears, whimpr_core::DictSource::Manual);
        let _ = store.save(&dict_path());
    }
}

pub fn dictionary_remove(correct: &str) {
    if let Some(m) = DICTIONARY.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        if store.remove(correct) {
            let _ = store.save(&dict_path());
        }
    }
}

pub fn dictionary_learn(correct: String, mishears: Vec<String>) {
    if let Some(m) = DICTIONARY.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        store.add(correct, mishears, whimpr_core::DictSource::Auto);
        let _ = store.save(&dict_path());
    }
}

pub fn snippet_entries() -> Vec<whimpr_core::SnippetEntry> {
    SNIPPETS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).entries.clone())
        .unwrap_or_default()
}

pub fn snippet_add(trigger: String, expansion: String) {
    if let Some(m) = SNIPPETS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        store.add(trigger, expansion);
        let _ = store.save(&snippets_path());
    }
}

pub fn snippet_remove(trigger: &str) {
    if let Some(m) = SNIPPETS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        if store.remove(trigger) {
            let _ = store.save(&snippets_path());
        }
    }
}
