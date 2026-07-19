//! Linux platform layer for WhimprFlow: an X11 global-hotkey grab for push-to-talk,
//! clipboard+`xdotool` text injection, and best-effort foreground-app detection,
//! plus the same dictation pipeline (audio → Whisper ASR → cleanup LLM → paste) and
//! the Hub-facing settings/stats/dictionary functions the Tauri commands call.
//!
//! ⚠️ UNVERIFIED: this module was written on macOS, without a Linux machine to build
//! or run it against, mirroring `crate::win`'s structure (and its own precedent —
//! see that file's doc comment). The shared crates (audio, ASR, cleanup, core) are
//! cross-platform, but this X11 glue has never been compiled. It is
//! `cfg(target_os = "linux")` so it does not affect — and is not checked by — the
//! macOS build. Treat it as a starting point, not a shipping port.
//!
//! Scope and simplifications made in this pass (all documented inline below too):
//!
//! - **X11 only — no Wayland.** Hotkeys and window/paste APIs differ completely on
//!   Wayland (no global key grabs without a compositor-specific global-shortcuts
//!   portal, no synthetic input without `wlr-virtual-pointer`/`xdg-desktop-portal`
//!   remote-desktop permission). Wiring the Wayland portal path is explicitly out of
//!   scope for this pass — **follow-up work**, not attempted here. On a Wayland
//!   session this module will simply fail to connect to an X server (unless XWayland
//!   is active, in which case it will only see X11 clients) and log an error rather
//!   than silently doing nothing.
//! - **XGrabKey, not XRecordExtension.** A full XRecord tap (mirroring macOS's
//!   listen-only CGEventTap or Windows' low-level keyboard hook) would see the key
//!   globally without exclusively grabbing it from other apps. Wiring the XRecord
//!   extension's setup blind (uncompiled) is meaningfully more involved and riskier
//!   to get right than the core-protocol `XGrabKey`, so v1 uses `XGrabKey` on a
//!   single hardcoded key (Right Ctrl, `XK_Control_R`) with `AnyModifier`. The
//!   trade-off: this key is grabbed *exclusively* for WhimprFlow while held (no other
//!   app sees it), and only that one physical key works — no chord/remap support.
//!   Good enough as a starting point; XRecord (or the Wayland portal) is the natural
//!   next step.
//! - **`xdotool` for paste and foreground-window lookup, not raw XTest/atom queries.**
//!   The task allows either wiring the XTest extension (`XTestFakeKeyEvent`) directly
//!   via `x11rb`'s `xtest` feature, or shelling out to `xdotool` as a pragmatic
//!   fallback "if wiring the XTest extension directly proves too fiddly." Since this
//!   code cannot be compiled or tested here, getting the XTest extension's exact
//!   request wiring (feature flags, extension setup handshake, `fake_input`'s field
//!   order) subtly wrong would fail silently in a way that's hard to reason about
//!   from a doc comment. `xdotool key ctrl+v` is a single well-documented,
//!   easy-to-verify-by-inspection command, so it was chosen for both the paste step
//!   and (via `xdotool getactivewindow getwindowclassname`) foreground-app detection
//!   — one dependency, one failure mode, both readable at a glance. It does mean an
//!   `xdotool` binary must be present on the user's system (`apt install xdotool` /
//!   `dnf install xdotool` / `pacman -S xdotool`); a follow-up could vendor the XTest
//!   calls directly via `x11rb` to drop that runtime dependency.
//! - X11 auto-repeat while the push-to-talk key is held will generate repeated
//!   KeyPress/KeyRelease pairs; `on_ptt_down`'s `RECORDING` swap-check already makes
//!   repeat key-downs a no-op (shared with Windows/macOS), but rapid-fire
//!   KeyRelease-then-KeyPress from *detectable* auto-repeat could in principle cause
//!   brief flicker. A follow-up could enable XKB detectable auto-repeat
//!   (`XkbSetDetectableAutoRepeat`) to eliminate this; not done here.
//!
//! Default push-to-talk key: Right Ctrl (same default as `crate::win`).

#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};
use x11rb::protocol::Event;

use whimpr_core::{AsrEngine, CleanupContext, CleanupMode, CleanupProvider, StatsSummary};

const OVERLAY_LABEL: &str = "whimpr_bar";

/// X11 keysym for Right Ctrl (`XK_Control_R`, see `<X11/keysymdef.h>`). Push-to-talk
/// key; chords land in a later pass (see the module doc comment).
const XK_CONTROL_R: u32 = 0xffe4;

static APP: OnceLock<AppHandle> = OnceLock::new();
static CLOCK: OnceLock<Instant> = OnceLock::new();
static RECORDING: AtomicBool = AtomicBool::new(false);
static CAPTURE: OnceLock<Mutex<Option<whimpr_audio::CaptureHandle>>> = OnceLock::new();
static ASR: OnceLock<Arc<whimpr_asr::WhisperEngine>> = OnceLock::new();
static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
static OPENAI: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>> = OnceLock::new();
static SETTINGS: OnceLock<Mutex<whimpr_core::Settings>> = OnceLock::new();
static DICTIONARY: OnceLock<Mutex<whimpr_core::DictionaryStore>> = OnceLock::new();
static STATS: OnceLock<Mutex<whimpr_core::StatsStore>> = OnceLock::new();

fn support_dir() -> std::path::PathBuf {
    // $XDG_CONFIG_HOME/WhimprFlow, falling back to ~/.config/WhimprFlow per the XDG
    // Base Directory spec (the Linux analogue of %APPDATA% / ~/Library/Application
    // Support).
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return std::path::PathBuf::from(xdg).join("WhimprFlow");
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".config").join("WhimprFlow")
}
fn settings_path() -> std::path::PathBuf {
    support_dir().join("settings.json")
}
fn dict_path() -> std::path::PathBuf {
    support_dir().join("dictionary.json")
}
fn stats_path() -> std::path::PathBuf {
    support_dir().join("stats.json")
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

/// The focused window's WM_CLASS (e.g. "firefox"), for per-app cleanup formatting —
/// the Linux analogue of the macOS bundle id / Windows executable name.
///
/// Pragmatic choice: shells out to `xdotool` (already required for `paste_text`
/// below) instead of hand-rolling `_NET_ACTIVE_WINDOW` + `WM_CLASS` X11
/// atom/property queries — see the module doc comment for why. Best-effort: returns
/// `None` on any failure (no `xdotool`, no active window, non-EWMH window manager,
/// Wayland/XWayland oddities, ...) rather than erroring the pipeline.
fn foreground_app() -> Option<String> {
    let out = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowclassname"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

// ── Text injection: clipboard + Ctrl+V via `xdotool` ────────────────────────────

pub fn paste_text(text: &str) -> anyhow::Result<()> {
    use arboard::Clipboard;
    let mut cb = Clipboard::new()?;
    let saved = cb.get_text().ok();
    cb.set_text(text.to_string())?;
    std::thread::sleep(Duration::from_millis(60));
    // See the module doc comment: `xdotool` chosen over wiring XTest directly.
    match std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("[whimpr:linux] xdotool exited with {status}"),
        Err(e) => eprintln!(
            "[whimpr:linux] failed to run xdotool ({e}) — install it (apt install xdotool / \
             dnf install xdotool / pacman -S xdotool) for paste to work"
        ),
    }
    std::thread::sleep(Duration::from_millis(150));
    if let Some(prev) = saved {
        let _ = cb.set_text(prev);
    }
    Ok(())
}

// ── Cleanup (shared, cross-platform building blocks — copied from `crate::win`) ─

fn current_settings_inner() -> whimpr_core::Settings {
    SETTINGS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default()
}

fn clean_transcript(raw: &str) -> String {
    let settings = current_settings_inner();
    let level = settings.cleanup_level;
    if matches!(settings.cleanup_mode, CleanupMode::Raw) || level.bypasses_llm() {
        return raw.to_string();
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
    match result {
        Some(Ok(cleaned)) => {
            let cleaned = whimpr_core::cleanup::post_process(&cleaned);
            if whimpr_core::cleanup::evaluate_gates(&raw_out, &cleaned, level).passed() {
                cleaned
            } else {
                raw_out
            }
        }
        _ => raw_out,
    }
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

// ── The push-to-talk pipeline (copied from `crate::win`) ────────────────────────

fn on_ptt_down() {
    if RECORDING.swap(true, Ordering::SeqCst) {
        return; // already recording
    }
    let _ = now_ms();
    emit_bar("recording");
    std::thread::spawn(|| match whimpr_audio::start(|_: &[f32]| {}) {
        Ok(handle) => {
            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        }
        Err(e) => eprintln!("[whimpr:linux] mic capture failed: {e}"),
    });
}

fn on_ptt_up() {
    if !RECORDING.swap(false, Ordering::SeqCst) {
        return; // wasn't recording
    }
    emit_bar("idle");
    let app = foreground_app();
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
            let text = clean_transcript(&t.text);
            if !text.is_empty() {
                if let Err(e) = paste_text(&text) {
                    eprintln!("[whimpr:linux] paste failed: {e}");
                }
                record_dictation(&text, res.duration_secs(), app);
            }
        }
    });
}

// ── X11 global hotkey grab (XGrabKey — see the module doc comment) ─────────────

/// Find a keycode that maps to the given keysym by walking the server's keyboard
/// mapping table. There is no `XKeysymToKeycode` in the async/xcb-style protocol
/// `x11rb` speaks, so this replicates it via `GetKeyboardMapping`.
///
/// UNVERIFIED against the exact `x11rb` version this project pins: double-check
/// `GetKeyboardMappingReply`'s field names/shape if this doesn't compile as-is.
fn keycode_for_keysym<C: Connection>(conn: &C, target: u32) -> Option<u8> {
    let setup = conn.setup();
    let min_kc = setup.min_keycode;
    let max_kc = setup.max_keycode;
    let count = (max_kc as u16).saturating_sub(min_kc as u16).saturating_add(1) as u8;
    let mapping = conn.get_keyboard_mapping(min_kc, count).ok()?.reply().ok()?;
    let per = mapping.keysyms_per_keycode as usize;
    if per == 0 {
        return None;
    }
    mapping
        .keysyms
        .chunks(per)
        .position(|chunk| chunk.iter().any(|&ks| ks == target))
        .map(|i| min_kc.wrapping_add(i as u8))
}

/// Connect to the X server, grab Right Ctrl globally (`AnyModifier`, so it fires
/// regardless of what other modifiers happen to be held), and block delivering
/// KeyPress/KeyRelease for it into the push-to-talk pipeline. Runs on its own thread
/// for the lifetime of the process, mirroring `crate::win::spawn_hook_thread`'s
/// dedicated message-pump thread.
fn run_hotkey_loop() -> anyhow::Result<()> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;

    let keycode = keycode_for_keysym(&conn, XK_CONTROL_R)
        .ok_or_else(|| anyhow::anyhow!("no keycode maps to XK_Control_R (Right Ctrl) on this keyboard layout"))?;

    // NOTE: unverified against the exact x11rb version pinned here — if `modifiers`
    // or `pointer_mode`/`keyboard_mode` don't accept `ModMask::ANY` / `GrabMode::ASYNC`
    // directly, adjust to whatever this crate version's grab_key signature expects.
    conn.grab_key(true, root, ModMask::ANY, keycode, GrabMode::ASYNC, GrabMode::ASYNC)?
        .check()?;
    conn.flush()?;
    eprintln!("[whimpr:linux] X11 key grab installed (push-to-talk: Right Ctrl, X11 only — see linux.rs doc comment for Wayland)");

    loop {
        match conn.wait_for_event()? {
            Event::KeyPress(ev) if ev.detail == keycode => on_ptt_down(),
            Event::KeyRelease(ev) if ev.detail == keycode => on_ptt_up(),
            _ => {}
        }
    }
}

fn spawn_hotkey_thread() {
    std::thread::spawn(|| {
        if let Err(e) = run_hotkey_loop() {
            eprintln!(
                "[whimpr:linux] X11 hotkey grab failed: {e} — is a display server reachable? \
                 This module only supports X11 (or XWayland); Wayland compositors' native \
                 protocol is not supported yet (see the module doc comment)."
            );
        }
    });
}

// ── Public surface (mirrors crate::win's, which the Tauri commands call) ───────

pub fn install(app: AppHandle) {
    let _ = APP.set(app);
    let _ = CLOCK.set(Instant::now());
    let settings = whimpr_core::Settings::load(&settings_path());
    let language_for_model = settings.language.clone();
    let _ = SETTINGS.set(Mutex::new(settings));
    let _ = DICTIONARY.set(Mutex::new(whimpr_core::DictionaryStore::load(&dict_path())));
    let _ = STATS.set(Mutex::new(whimpr_core::StatsStore::load(&stats_path())));
    let _ = OPENAI.set(Mutex::new(None));
    let _ = LOCAL.set(Mutex::new(None));
    rebuild_providers();

    // Load Whisper.
    std::thread::spawn(move || {
        match whimpr_asr::WhisperEngine::load(&whisper_model_path(language_for_model.as_deref())) {
            Ok(engine) => {
                let _ = ASR.set(Arc::new(engine));
                eprintln!("[whimpr:linux] ASR ready");
            }
            Err(e) => eprintln!("[whimpr:linux] ASR load failed: {e}"),
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

    spawn_hotkey_thread();
    eprintln!("[whimpr:linux] installing X11 push-to-talk grab (Right Ctrl)");
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
