//! Linux platform layer for WhimprFlow: an X11 global-hotkey grab for push-to-talk,
//! clipboard+`xdotool` text injection, and best-effort foreground-app detection,
//! plus the same dictation pipeline (audio → Whisper ASR → cleanup LLM → paste) and
//! the Hub-facing settings/stats/dictionary/snippets/workflows/notes/voice-memory
//! functions the Tauri commands call.
//!
//! ⚠️ UNVERIFIED: this module was written on macOS, without a Linux machine to build
//! or run it against, mirroring `crate::win`'s structure (and its own precedent  -
//! see that file's doc comment). The shared crates (audio, ASR, cleanup, core) are
//! cross-platform, but this X11 glue has never been compiled. It is
//! `cfg(target_os = "linux")` so it does not affect  -  and is not checked by  -  the
//! macOS build. Treat it as a starting point, not a shipping port.
//!
//! Roadmap-15 parity scope on Linux (mirrors the macOS pipeline where the spec
//! says cross-platform): provenance + `record_full` (raw/confidence/low_words),
//! retention pruning, language threading via `transcribe_opts`, workflow trigger
//! routing with pending approve/reject, meeting-mode notes, voice memory
//! (encrypted log + export bundle), receipt events (`whimpr://receipt`,
//! `whimpr://pending`, same payloads as macOS), health, and delete-all-text.
//! Context Capsule, streaming preview, and screen capture stay macOS-only this
//! pass  -  see the `ponytail:` comments on `get_last_capsule`, the
//! streaming-preview note in `on_ptt_down`, and `capture_screen`.
//!
//! Scope and simplifications made in this pass (all documented inline below too):
//!
//! - **X11 only  -  no Wayland.** Hotkeys and window/paste APIs differ completely on
//!   Wayland (no global key grabs without a compositor-specific global-shortcuts
//!   portal, no synthetic input without `wlr-virtual-pointer`/`xdg-desktop-portal`
//!   remote-desktop permission). Wiring the Wayland portal path is explicitly out of
//!   scope for this pass  -  **follow-up work**, not attempted here. On a Wayland
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
//!   app sees it), and only that one physical key works  -  no chord/remap support,
//!   so the rebindable cancel/paste-last/copy-last/undo-last chords have no Linux
//!   hotkeys yet either. Good enough as a starting point; XRecord (or the Wayland
//!   portal) is the natural next step.
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
//!    -  one dependency, one failure mode, both readable at a glance. It does mean an
//!   `xdotool` binary must be present on the user's system (`apt install xdotool` /
//!   `dnf install xdotool` / `pacman -S xdotool`); a follow-up could vendor the XTest
//!   calls directly via `x11rb` to drop that runtime dependency.
//! - X11 auto-repeat: a held key normally generates repeated KeyRelease/KeyPress
//!   pairs, which would confuse both hold-to-talk and the double-tap lock below  -
//!   but the push-to-talk key is Right Ctrl, a *modifier*, and X11 keyboards do not
//!   auto-repeat modifier keys by default, so in practice one physical hold is one
//!   Press ... Release pair. A follow-up could enable XKB detectable auto-repeat
//!   (`XkbSetDetectableAutoRepeat`) to make this robust against exotic keymaps;
//!   not done here.
//!
//! Default push-to-talk key: Right Ctrl (same default as `crate::win`). A second
//! tap within the double-tap window locks a hands-free session; a third press (or
//! the pill's Stop button via `confirm_dictation`) finalizes it.

#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};
use x11rb::protocol::Event;

use whimpr_core::state::timing::DOUBLE_TAP_MS;
use whimpr_core::{
    CleanupContext, CleanupMode, CleanupProvider, StatsSummary, WorkflowDestination,
};

const OVERLAY_LABEL: &str = "whimpr_bar";
/// The Hub window's label  -  receipts and pending-approval events go to both
/// the overlay pill and the Hub (same routing as macOS).
const HUB_LABEL: &str = "main";
/// Truncation length for the pending-approval previews.
const PREVIEW_CHARS: usize = 200;

/// X11 keysym for Right Ctrl (`XK_Control_R`, see `<X11/keysymdef.h>`). Push-to-talk
/// key; chords land in a later pass (see the module doc comment).
const XK_CONTROL_R: u32 = 0xffe4;

static APP: OnceLock<AppHandle> = OnceLock::new();
static CLOCK: OnceLock<Instant> = OnceLock::new();
static RECORDING: AtomicBool = AtomicBool::new(false);
/// True while a double-tap locked (hands-free) session is running. Meeting mode
/// reads it at finalize to decide note-vs-paste and long-form transcription.
static LOCKED: AtomicBool = AtomicBool::new(false);
/// `now_ms()` of the last push-to-talk key-up, `u64::MAX` until the first one
/// this run. Compared against `DOUBLE_TAP_MS` on the next key-down for the
/// hands-free lock (mirrors `crate::win`).
static LAST_KEY_UP_MS: AtomicU64 = AtomicU64::new(u64::MAX);
static CAPTURE: OnceLock<Mutex<Option<whimpr_audio::CaptureHandle>>> = OnceLock::new();
/// The loaded whisper engine, hot-swappable when a language change needs a
/// different model file (see [`maybe_reload_asr`]). `Arc` so in-flight
/// transcriptions keep the old engine alive across a swap.
static ASR: OnceLock<Mutex<Option<Arc<whimpr_asr::WhisperEngine>>>> = OnceLock::new();
/// File name of the whisper model actually loaded (for provenance + health).
/// Swapped together with [`ASR`] on a hot reload  -  always set via
/// [`set_asr`] so the pair stays consistent.
static ASR_MODEL_NAME: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
static OPENAI: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>> = OnceLock::new();
static ANTHROPIC: OnceLock<Mutex<Option<whimpr_cleanup::AnthropicProvider>>> = OnceLock::new();
static SETTINGS: OnceLock<Mutex<whimpr_core::Settings>> = OnceLock::new();
static DICTIONARY: OnceLock<Mutex<whimpr_core::DictionaryStore>> = OnceLock::new();
static SNIPPETS: OnceLock<Mutex<whimpr_core::SnippetStore>> = OnceLock::new();
static STATS: OnceLock<Mutex<whimpr_core::StatsStore>> = OnceLock::new();
/// The user's voice workflows (trigger -> command-edit instruction).
static WORKFLOWS: OnceLock<Mutex<whimpr_core::WorkflowStore>> = OnceLock::new();
/// Voice Memory (encrypted at rest) and its keychain-held AES key. The key
/// slot stays empty when the Secret Service is unavailable  -  memory then
/// lives only for this run and saves are skipped.
static VOICE_MEMORY: OnceLock<Mutex<whimpr_core::VoiceMemory>> = OnceLock::new();
static VM_KEY: OnceLock<[u8; 32]> = OnceLock::new();
/// WM_CLASS of the app that was focused at record-start = the paste target.
/// Cleanup uses it to format for the medium (email vs. text vs. chat).
static TARGET_APP: OnceLock<Mutex<Option<String>>> = OnceLock::new();
/// (raw pre-cleanup text, final pasted text) from the most recent dictation.
/// No chord hotkey reads this on Linux yet (see the module doc comment); it is
/// stashed anyway so the undo-last-cleanup path only needs the key grab once
/// the XRecord/chord pass lands.
static LAST_TEXTS: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();
/// A workflow result held for user approval (`require_approval`), consumed
/// by `approve_pending` / `reject_pending`. At most one at a time; a new
/// pending result replaces an unanswered one.
static PENDING: OnceLock<Mutex<Option<PendingItem>>> = OnceLock::new();

/// A workflow result awaiting approval, with everything `record_full` and
/// the receipt need once the user decides.
struct PendingItem {
    name: String,
    text: String,
    destination: WorkflowDestination,
    raw: String,
    confidence: Option<f32>,
    low_words: Vec<String>,
    duration_secs: f32,
    /// WM_CLASS of the app the user dictated into (the TARGET_APP snapshot at
    /// creation time). This pass has no X11 re-activation helper, so a Paste
    /// approval downgrades to the clipboard (see `approve_pending`)  -  the
    /// snapshot is kept for the receipt message and the wmctrl upgrade path.
    target_app: Option<String>,
}

/// What `clean_transcript` produced: the normalized raw transcript (what
/// "undo cleanup" restores), the text to insert, and where it came from.
struct CleanOutcome {
    raw_out: String,
    final_text: String,
    provenance: whimpr_core::Provenance,
}

#[derive(Clone, Serialize)]
struct BarPayload {
    state: &'static str,
}

#[derive(Clone, Serialize)]
struct TranscriptPayload {
    text: String,
}

/// The insertion receipt emitted after every finalize (spec: whimpr://receipt).
#[derive(Clone, Serialize)]
struct ReceiptPayload {
    ok: bool,
    action: &'static str,
    app: Option<String>,
    words: u32,
    confidence: Option<f32>,
    low_words: Vec<String>,
    message: Option<String>,
}

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
    std::path::PathBuf::from(home)
        .join(".config")
        .join("WhimprFlow")
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
fn workflows_path() -> std::path::PathBuf {
    support_dir().join("workflows.json")
}
/// Where the Studio notes live. Mirrors `crate::notes`' private path helper
/// (same support dir, same file name)  -  used only for backups here.
fn notes_path() -> std::path::PathBuf {
    support_dir().join("notes.json")
}
fn voice_memory_path() -> std::path::PathBuf {
    support_dir().join("voice_memory.enc")
}
/// `.en`-suffixed models are English-only, so when a specific non-English
/// language is selected we only consider multilingual model files (no `.en`
/// suffix); otherwise `.en` models are preferred first for better English
/// accuracy, falling back to multilingual files if none are present.
fn whisper_model_path(language: Option<&str>) -> std::path::PathBuf {
    let dir = support_dir().join("models");
    let needs_multilingual = matches!(language, Some(lang) if lang != "en");
    const MULTILINGUAL: &[&str] = &["ggml-medium.bin", "ggml-small.bin", "ggml-base.bin"];
    const ENGLISH_FIRST: &[&str] = &[
        "ggml-medium.en.bin",
        "ggml-small.en.bin",
        "ggml-base.en.bin",
        "ggml-medium.bin",
        "ggml-small.bin",
        "ggml-base.bin",
    ];
    if needs_multilingual {
        for name in MULTILINGUAL {
            let p = dir.join(name);
            if p.exists() {
                return p;
            }
        }
        // No multilingual file installed: fall back to whatever exists
        // (likely an English-only .en model) rather than returning a
        // nonexistent path, which would leave ASR permanently unloaded.
        // Dictation stays alive  -  English-only  -  instead of bricked.
        eprintln!(
            "[whimpr:linux] no multilingual whisper model found for language {:?}  -  falling \
             back to an English-only model; add a non-.en ggml model to the models \
             folder to transcribe this language",
            language.unwrap_or_default()
        );
    }
    for name in ENGLISH_FIRST {
        let p = dir.join(name);
        if p.exists() {
            return p;
        }
    }
    dir.join(ENGLISH_FIRST.last().copied().unwrap_or("ggml-base.en.bin"))
}

/// Seconds since the Unix epoch (UTC), or 0 if the clock is before the epoch.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    CLOCK
        .get()
        .map(|c| c.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

/// The currently loaded whisper engine, if any. Cheap: clones the `Arc`
/// out of the slot, so callers never hold the lock across a transcription
/// and a concurrent hot swap can't stall (or be stalled by) them.
fn current_asr() -> Option<Arc<whimpr_asr::WhisperEngine>> {
    ASR.get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

/// File name of the whisper model currently loaded, if any.
fn current_asr_model() -> Option<String> {
    ASR_MODEL_NAME
        .get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Store a freshly loaded engine + its model file name (initial load or a
/// hot swap). Holds both locks briefly so the pair can never be observed
/// mismatched; readers only ever take one lock at a time, so no deadlock.
fn set_asr(engine: Arc<whimpr_asr::WhisperEngine>, model_name: String) {
    let engine_slot = ASR.get_or_init(|| Mutex::new(None));
    let name_slot = ASR_MODEL_NAME.get_or_init(|| Mutex::new(None));
    let mut engine_guard = engine_slot.lock().unwrap_or_else(|e| e.into_inner());
    let mut name_guard = name_slot.lock().unwrap_or_else(|e| e.into_inner());
    *engine_guard = Some(engine);
    *name_guard = Some(model_name);
}

/// The language to hand whisper for the CURRENTLY LOADED model: an
/// English-only (`.en.bin`) model can only ever transcribe English, so
/// always pass "en" for it  -  auto-detect (or a non-English hint) on an
/// .en model wastes an encode pass on language detection and can
/// misbehave. Multilingual models get the configured language through
/// unchanged (`None` = auto-detect).
fn effective_language(configured: Option<&str>) -> Option<String> {
    let loaded_is_english_only = current_asr_model()
        .map(|n| n.ends_with(".en.bin"))
        .unwrap_or(false);
    if loaded_is_english_only {
        Some("en".to_string())
    } else {
        configured.map(|s| s.to_string())
    }
}

/// Provenance tag for the loaded ASR engine, e.g. "whisper.cpp:ggml-base.en.bin".
fn asr_engine_tag() -> String {
    current_asr_model()
        .map(|n| format!("whisper.cpp:{n}"))
        .unwrap_or_default()
}

/// First `n` chars of `s` (for event previews).
fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// WM_CLASS of the app that was focused at record start, if any.
fn target_app() -> Option<String> {
    TARGET_APP
        .get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

fn emit_bar(state: &'static str) {
    if let Some(app) = APP.get() {
        let _ = app.emit_to(
            OVERLAY_LABEL,
            "whimpr://flowbar/state",
            BarPayload { state },
        );
    }
}

/// Emit the insertion receipt to both the overlay pill and the Hub.
fn emit_receipt(app: &AppHandle, payload: ReceiptPayload) {
    eprintln!(
        "[whimpr:linux] receipt: ok={} action={} words={}",
        payload.ok, payload.action, payload.words
    );
    let _ = app.emit_to(OVERLAY_LABEL, "whimpr://receipt", payload.clone());
    let _ = app.emit_to(HUB_LABEL, "whimpr://receipt", payload);
}

/// Announce a workflow result awaiting approval, to both windows.
fn emit_pending(app: &AppHandle, name: &str, preview: &str) {
    let payload = crate::hotkey::PendingPayload {
        name: name.to_string(),
        preview: preview.to_string(),
    };
    let _ = app.emit_to(OVERLAY_LABEL, "whimpr://pending", payload.clone());
    let _ = app.emit_to(HUB_LABEL, "whimpr://pending", payload);
}

/// Local date/time note title for meeting-mode transcripts ("2026-07-19 14:03").
fn local_datetime_title() -> String {
    std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Meeting {}", unix_now()))
}

/// The focused window's WM_CLASS (e.g. "firefox"), for per-app cleanup formatting  -
/// the Linux analogue of the macOS bundle id / Windows executable name.
///
/// Pragmatic choice: shells out to `xdotool` (already required for `paste_text`
/// below) instead of hand-rolling `_NET_ACTIVE_WINDOW` + `WM_CLASS` X11
/// atom/property queries  -  see the module doc comment for why. Best-effort: returns
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
    // On failure the error propagates (so the receipt is honest) and our text is
    // deliberately LEFT on the clipboard  -  the user can still Ctrl+V manually.
    match std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => anyhow::bail!("xdotool exited with {status}"),
        Err(e) => anyhow::bail!(
            "failed to run xdotool ({e})  -  install it (apt install xdotool / \
             dnf install xdotool / pacman -S xdotool) for paste to work"
        ),
    }
    std::thread::sleep(Duration::from_millis(150));
    if let Some(prev) = saved {
        let _ = cb.set_text(prev);
    }
    Ok(())
}

// ── Keys + providers (mirrors hotkey.rs's read_key/rebuild_providers) ──────────

/// Read an API key from an env var or the OS keyring (never a plaintext file).
/// On Linux the keyring backend is the Secret Service (GNOME Keyring / KWallet);
/// when it is unavailable the env var is the only source.
fn read_key(account: &str, env_var: &str) -> Option<String> {
    if let Ok(k) = std::env::var(env_var) {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some(k);
        }
    }
    keyring::Entry::new("com.whimpr.whimprflow", account)
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}
fn read_openai_key() -> Option<String> {
    read_key("openai_api_key", "OPENAI_API_KEY")
}
fn read_anthropic_key() -> Option<String> {
    read_key("anthropic_api_key", "ANTHROPIC_API_KEY")
}

fn current_settings_inner() -> whimpr_core::Settings {
    SETTINGS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default()
}

// ── Cleanup (shared, cross-platform building blocks  -  mirrors hotkey.rs) ──────

/// Clean a raw transcript per the current settings (mode + level), feeding in the
/// dictionary vocabulary relevant to this utterance. Falls back to raw whenever
/// cleanup is off, the provider is unavailable, it errors, or the gates reject it.
///
/// Returns a [`CleanOutcome`]: `raw_out` is the pre-cleanup transcript after
/// `pre_normalize_layout`/`post_process`, `final_text` is what actually gets
/// pasted, and `provenance` says which route shaped it (Privacy ledger).
fn clean_transcript(raw: &str) -> CleanOutcome {
    let settings = current_settings_inner();
    let level = settings.cleanup_level;
    let mut provenance = whimpr_core::Provenance {
        asr_engine: asr_engine_tag(),
        cleanup: "raw".to_string(),
        sent_to_cloud: false,
        gate: "skipped".to_string(),
    };
    if matches!(settings.cleanup_mode, CleanupMode::Raw) || level.bypasses_llm() {
        let text = if settings.safe_mode {
            whimpr_core::redact_inappropriate_words(raw)
        } else {
            raw.to_string()
        };
        return CleanOutcome {
            raw_out: text.clone(),
            final_text: text,
            provenance,
        };
    }
    // Turn explicit spoken layout cues ("new line", "new paragraph") into break
    // markers up front  -  the model passes an opaque marker through reliably but
    // mangles the literal cue words. The model sees `raw` (with markers); the gate
    // and any raw fallback use `raw_out` (markers restored to real breaks).
    let raw_norm = whimpr_core::cleanup::pre_normalize_layout(raw);
    let raw = raw_norm.as_str();
    let raw_out = whimpr_core::cleanup::post_process(&raw_norm);
    let vocab = DICTIONARY
        .get()
        .map(|d| {
            d.lock()
                .unwrap_or_else(|e| e.into_inner())
                .prefilter(raw, 15)
        })
        .unwrap_or_default();
    let app_bundle_id = target_app();
    if let Some(app) = app_bundle_id.as_deref() {
        eprintln!("[whimpr:linux] cleanup target app: {app}");
    }
    // Code Mode: the code-dictation prompt variant, when the paste target is
    // an IDE/terminal and the user hasn't opted out. WM_CLASS values only
    // partially overlap `is_code_app`'s bundle-id substrings; unmatched code
    // apps just get the prose prompt.
    let code_mode = settings.code_mode_auto
        && app_bundle_id
            .as_deref()
            .map(whimpr_core::cleanup::prompts::is_code_app)
            .unwrap_or(false);
    if code_mode {
        eprintln!("[whimpr:linux] code mode active for this cleanup");
    }
    let ctx = CleanupContext {
        level,
        vocab,
        app_bundle_id,
        // Context Capsule is macOS-only this pass  -  no window context on Linux
        // (see `get_last_capsule` for the ceiling + upgrade path).
        window_context: None,
        style: settings.style.to_instructions(),
        code_mode,
    };
    // Run the on-device model with the same prompt + per-app formatting.
    let run_local = || -> Option<anyhow::Result<String>> {
        LOCAL.get().and_then(|m| {
            m.lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
                .map(|w| {
                    // System prompt + few-shot demonstration turns + the transcript,
                    // so the on-device model actually produces newlines/lists and
                    // resolves self-corrections instead of just being told to.
                    let messages = whimpr_core::cleanup::build_messages(raw, &ctx);
                    w.cleanup(&messages)
                })
        })
    };
    // Selected provider, falling back to local when a cloud key can't be read
    // (so cleanup still runs)  -  and Local mode uses the worker directly.
    // `route` names whichever provider actually ran, for the Privacy ledger.
    let local_route = |r: &Option<anyhow::Result<String>>| {
        if r.is_some() {
            "local".to_string()
        } else {
            String::new()
        }
    };
    let (result, route, sent_to_cloud): (Option<anyhow::Result<String>>, String, bool) =
        match settings.cleanup_mode {
            CleanupMode::OpenAi => {
                // Clone the provider out of the lock (providers are cheap
                // to clone) so rebuild_providers  -  settings / API-key
                // edits  -  never blocks behind this HTTP round-trip.
                let provider = OPENAI
                    .get()
                    .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
                let cloud = provider.map(|p| p.cleanup(raw, &ctx));
                match cloud {
                    Some(r) => (Some(r), format!("openai:{}", settings.openai_model), true),
                    None => {
                        let r = run_local();
                        let route = local_route(&r);
                        (r, route, false)
                    }
                }
            }
            CleanupMode::Anthropic => {
                let provider = ANTHROPIC
                    .get()
                    .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
                let cloud = provider.map(|p| p.cleanup(raw, &ctx));
                match cloud {
                    Some(r) => (
                        Some(r),
                        format!("anthropic:{}", settings.anthropic_model),
                        true,
                    ),
                    None => {
                        let r = run_local();
                        let route = local_route(&r);
                        (r, route, false)
                    }
                }
            }
            CleanupMode::Local => {
                let r = run_local();
                let route = local_route(&r);
                (r, route, false)
            }
            CleanupMode::Raw => (None, String::new(), false),
        };
    provenance.sent_to_cloud = sent_to_cloud;
    let final_text = match result {
        Some(Ok(cleaned)) => {
            // Deterministic safety net: convert any leftover spoken layout cue the
            // model missed into real line breaks, strip stray code fences, cap blank
            // lines.
            let cleaned = whimpr_core::cleanup::post_process(&cleaned);
            provenance.cleanup = route;
            if whimpr_core::cleanup::evaluate_gates(&raw_out, &cleaned, level).passed() {
                provenance.gate = "passed".to_string();
                cleaned
            } else {
                eprintln!("[whimpr:linux] cleanup gate rejected the edit  -  pasting raw");
                provenance.gate = "rejected".to_string();
                raw_out.clone()
            }
        }
        Some(Err(e)) => {
            // Provider errored: the final text is raw ("raw"/"skipped" stand),
            // but sent_to_cloud stays honest  -  the transcript may have left
            // the machine even though no edit came back.
            eprintln!("[whimpr:linux] cleanup failed ({e})  -  pasting raw");
            raw_out.clone()
        }
        None => {
            if matches!(settings.cleanup_mode, CleanupMode::Local) {
                eprintln!("[whimpr:linux] local cleanup model not wired yet  -  pasting raw");
            } else {
                eprintln!("[whimpr:linux] cleanup provider has no API key  -  pasting raw");
            }
            raw_out.clone()
        }
    };
    let raw_out = if settings.safe_mode {
        whimpr_core::redact_inappropriate_words(&raw_out)
    } else {
        raw_out
    };
    let final_text = if settings.safe_mode {
        whimpr_core::redact_inappropriate_words(&final_text)
    } else {
        final_text
    };
    CleanOutcome {
        raw_out,
        final_text,
        provenance,
    }
}

/// Run a workflow's instruction-following rewrite through whichever cleanup
/// provider is configured (cloud). Errors when only a local provider is
/// available, since the local worker's command-edit path isn't wired in this build.
fn run_command_edit(selection: &str, instruction: &str) -> anyhow::Result<String> {
    let settings = current_settings_inner();
    let run_local = |_selection: &str, _instruction: &str| -> anyhow::Result<String> {
        anyhow::bail!(
            "local Command Mode is unavailable in this build. Set Cleanup Engine to OpenAI \
             or Anthropic in Settings to use Workflows"
        )
    };
    // Clone the provider out of the lock before the blocking HTTP call so
    // rebuild_providers (settings / API-key edits) never waits on it.
    match settings.cleanup_mode {
        CleanupMode::OpenAi => OPENAI
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .map(|p| p.command_edit(selection, instruction))
            .unwrap_or_else(|| run_local(selection, instruction)),
        CleanupMode::Anthropic => ANTHROPIC
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .map(|p| p.command_edit(selection, instruction))
            .unwrap_or_else(|| run_local(selection, instruction)),
        CleanupMode::Local | CleanupMode::Raw => run_local(selection, instruction),
    }
}

/// Log one completed dictation to the stats store (words, speaking time, text,
/// raw transcript, provenance, confidence, target app) and persist it. Powers
/// the Hub stats, the history list, and the Privacy ledger. Honors retention:
/// `retention_days == Some(0)` never stores text, and any `Some(n)` prunes
/// text older than the window on every record.
fn record_dictation(
    text: &str,
    raw: &str,
    duration_secs: f32,
    provenance: whimpr_core::Provenance,
    confidence: Option<f32>,
    low_words: Vec<String>,
) {
    let words = whimpr_core::stats::count_words(text);
    if words == 0 {
        return;
    }
    let app = target_app();
    let retention_days = current_settings_inner().retention_days;
    // Some(0) = never store dictation text at all (numbers only).
    let store_text = retention_days != Some(0);
    if let Some(m) = STATS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        let duration_ms = (duration_secs.max(0.0) * 1000.0) as u32;
        let chars = text.chars().count() as u32;
        let now = unix_now();
        store.record_full(whimpr_core::SessionRecord {
            ts_unix: now,
            words,
            duration_ms,
            chars,
            text: if store_text {
                text.to_string()
            } else {
                String::new()
            },
            app,
            raw: if store_text {
                raw.to_string()
            } else {
                String::new()
            },
            provenance,
            confidence,
            // Low-confidence words are verbatim dictation content too  -
            // "never store text" keeps them out of the store as well.
            low_words: if store_text { low_words } else { Vec::new() },
        });
        if let Some(days) = retention_days {
            store.prune_texts(now, days);
        }
        let _ = store.save(&stats_path());
    }
}

// ── The finalize pipeline (mirrors hotkey.rs's finalize_transcript) ─────────────

/// The full post-ASR pipeline for one finalized dictation, in spec order:
/// workflow trigger -> snippet -> cleanup, then meeting-note or paste  -
/// with a receipt emitted and the session recorded on every path.
fn finalize_transcript(
    app: &AppHandle,
    raw: String,
    confidence: Option<f32>,
    low_words: Vec<String>,
    duration_secs: f32,
    was_locked: bool,
) {
    let settings = current_settings_inner();
    if raw.is_empty() {
        emit_receipt(
            app,
            ReceiptPayload {
                ok: false,
                action: "error",
                app: target_app(),
                words: 0,
                confidence,
                low_words,
                message: Some("no speech detected".to_string()),
            },
        );
        return;
    }

    // Workflow trigger first: a spoken prefix routes the remainder of the
    // utterance through the command-edit provider path.
    let matched = WORKFLOWS.get().and_then(|m| {
        let store = m.lock().unwrap_or_else(|e| e.into_inner());
        store
            .find_match(&raw)
            .map(|(e, payload)| (e.clone(), payload))
    });
    // When the matched workflow's provider call fails, the utterance falls
    // through to the normal snippet/cleanup pipeline below (paste + record
    // + receipt) instead of being swallowed; the receipt carries this note
    // so the user knows the workflow was bypassed.
    let mut workflow_note: Option<String> = None;
    if let Some((entry, payload)) = matched {
        eprintln!("[whimpr:linux] WORKFLOW \"{}\" matched", entry.name);
        // A trigger-only utterance runs the instruction over the whole
        // utterance rather than an empty payload.
        let input = if payload.is_empty() {
            raw.clone()
        } else {
            payload
        };
        match run_command_edit(&input, &entry.instruction) {
            Ok(text) => {
                let text = if settings.safe_mode {
                    whimpr_core::redact_inappropriate_words(&text)
                } else {
                    text
                };
                if entry.require_approval {
                    *PENDING
                        .get_or_init(|| Mutex::new(None))
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Some(PendingItem {
                        name: entry.name.clone(),
                        text: text.clone(),
                        destination: entry.destination,
                        raw,
                        confidence,
                        low_words: low_words.clone(),
                        duration_secs,
                        target_app: target_app(),
                    });
                    emit_pending(app, &entry.name, &truncate_chars(&text, PREVIEW_CHARS));
                    emit_receipt(
                        app,
                        ReceiptPayload {
                            ok: true,
                            action: "pending",
                            app: target_app(),
                            words: whimpr_core::stats::count_words(&text),
                            confidence,
                            low_words,
                            message: None,
                        },
                    );
                } else {
                    deliver_workflow(
                        app,
                        &entry.name,
                        text,
                        entry.destination,
                        raw,
                        confidence,
                        low_words,
                        duration_secs,
                        None,
                    );
                }
                return;
            }
            Err(e) => {
                eprintln!(
                    "[whimpr:linux] workflow \"{}\" failed ({e})  -  falling back to a normal \
                     dictation so the utterance isn't lost",
                    entry.name
                );
                workflow_note = Some(format!(
                    "workflow \"{}\" failed: {e}; inserted the plain transcript instead",
                    entry.name
                ));
                // fall through to the snippet/cleanup pipeline below
            }
        }
    }

    // Static snippets next, on the raw transcript, before cleanup runs. A
    // match pastes the expansion verbatim and skips the whole cleanup
    // pipeline (no LLM call, no gates).
    let snippet_expansion = SNIPPETS.get().and_then(|m| {
        m.lock()
            .unwrap_or_else(|e| e.into_inner())
            .find_match(&raw)
            .map(|entry| entry.expansion.clone())
    });
    let outcome = match snippet_expansion {
        Some(expansion) => {
            eprintln!("[whimpr:linux] SNIPPET matched  -  pasting expansion directly");
            let expansion = if settings.safe_mode {
                whimpr_core::redact_inappropriate_words(&expansion)
            } else {
                expansion
            };
            CleanOutcome {
                raw_out: expansion.clone(),
                final_text: expansion,
                provenance: whimpr_core::Provenance {
                    asr_engine: asr_engine_tag(),
                    cleanup: "snippet".to_string(),
                    sent_to_cloud: false,
                    gate: "skipped".to_string(),
                },
            }
        }
        None => {
            // Clean the transcript (cloud LLM if configured).
            let outcome = clean_transcript(&raw);
            if outcome.final_text != raw {
                eprintln!("[whimpr:linux] CLEANED:   \"{}\"", outcome.final_text);
            }
            outcome
        }
    };
    let CleanOutcome {
        raw_out,
        final_text: text,
        provenance,
    } = outcome;
    if text.is_empty() {
        emit_receipt(
            app,
            ReceiptPayload {
                ok: false,
                action: "error",
                app: target_app(),
                words: 0,
                confidence,
                low_words,
                message: Some("nothing to insert".to_string()),
            },
        );
        return;
    }
    let words = whimpr_core::stats::count_words(&text);

    // Meeting mode: a locked (hands-free) session's transcript becomes a
    // note instead of a paste.
    if settings.meeting_mode && was_locked {
        crate::notes::add(local_datetime_title(), text.clone(), None);
        record_dictation(
            &text,
            &raw_out,
            duration_secs,
            provenance,
            confidence,
            low_words.clone(),
        );
        emit_receipt(
            app,
            ReceiptPayload {
                ok: true,
                action: "noted",
                app: target_app(),
                words,
                confidence,
                low_words,
                message: workflow_note,
            },
        );
        let _ = app.emit_to(
            OVERLAY_LABEL,
            "whimpr://transcript",
            TranscriptPayload { text },
        );
        return;
    }

    // Paste into the target app; the receipt reports the outcome either way.
    let paste_result = paste_text(&text);
    if let Err(e) = &paste_result {
        eprintln!("[whimpr:linux] paste failed: {e}");
    }
    // Stash (raw, final) for the undo-last-cleanup path (no Linux hotkey for it
    // yet; see the LAST_TEXTS doc comment).
    *LAST_TEXTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some((raw_out.clone(), text.clone()));
    // Log words + speaking time for the Hub stats (WPM, streak, ...).
    record_dictation(
        &text,
        &raw_out,
        duration_secs,
        provenance,
        confidence,
        low_words.clone(),
    );
    match paste_result {
        Ok(()) => {
            // Autolearn correction watch, in the same pipeline slot as macOS. Its
            // observer is AX-based, so off macOS this is currently a no-op stub.
            crate::autolearn::watch_correction(&text);
            emit_receipt(
                app,
                ReceiptPayload {
                    ok: true,
                    action: "pasted",
                    app: target_app(),
                    words,
                    confidence,
                    low_words,
                    message: workflow_note,
                },
            );
        }
        Err(e) => emit_receipt(
            app,
            ReceiptPayload {
                ok: false,
                action: "error",
                app: target_app(),
                words,
                confidence,
                low_words,
                message: Some(format!("paste failed: {e}")),
            },
        ),
    }
    let _ = app.emit_to(
        OVERLAY_LABEL,
        "whimpr://transcript",
        TranscriptPayload { text },
    );
}

/// Send a workflow result to its destination, record it, and emit the
/// receipt. `note` is an optional caller-supplied context line for the
/// receipt (e.g. "copied to the clipboard instead"); a destination failure's
/// own error message takes precedence.
#[allow(clippy::too_many_arguments)]
fn deliver_workflow(
    app: &AppHandle,
    name: &str,
    text: String,
    destination: WorkflowDestination,
    raw: String,
    confidence: Option<f32>,
    low_words: Vec<String>,
    duration_secs: f32,
    note: Option<String>,
) {
    let provenance = whimpr_core::Provenance {
        asr_engine: asr_engine_tag(),
        cleanup: format!("workflow:{name}"),
        // command_edit only runs through the cloud providers in this build.
        sent_to_cloud: true,
        gate: "skipped".to_string(),
    };
    let words = whimpr_core::stats::count_words(&text);
    let (ok, action, message): (bool, &'static str, Option<String>) = match destination {
        WorkflowDestination::Paste => match paste_text(&text) {
            Ok(()) => {
                *LAST_TEXTS
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some((raw.clone(), text.clone()));
                (true, "pasted", None)
            }
            Err(e) => (false, "error", Some(format!("paste failed: {e}"))),
        },
        WorkflowDestination::Clipboard => {
            use arboard::Clipboard;
            match Clipboard::new().and_then(|mut cb| cb.set_text(text.clone())) {
                Ok(()) => (true, "clipboard", None),
                Err(e) => (false, "error", Some(format!("clipboard failed: {e}"))),
            }
        }
        WorkflowDestination::Note => {
            crate::notes::add(name.to_string(), text.clone(), None);
            (true, "noted", None)
        }
    };
    record_dictation(
        &text,
        &raw,
        duration_secs,
        provenance,
        confidence,
        low_words.clone(),
    );
    emit_receipt(
        app,
        ReceiptPayload {
            ok,
            action,
            app: target_app(),
            words,
            confidence,
            low_words,
            message: message.or(note),
        },
    );
}

// ── The push-to-talk pipeline (double-tap lock mirrors crate::win) ──────────────

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
    // Snapshot the paste target now, while the user's app is focused. Shelling
    // out to xdotool takes a few ms on the X event thread; acceptable since the
    // grab queue is ours alone.
    let target = foreground_app();
    *TARGET_APP
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = target;
    let now = now_ms();
    let last_up = LAST_KEY_UP_MS.load(Ordering::SeqCst);
    let is_double_tap = last_up != u64::MAX && now.saturating_sub(last_up) <= DOUBLE_TAP_MS;
    if is_double_tap {
        LOCKED.store(true, Ordering::SeqCst);
        emit_bar("locked");
    } else {
        emit_bar("recording");
    }
    // Streaming preview (whimpr://transcript/partial) is macOS-only this pass.
    // ponytail: no partial-transcription loop on Linux  -  the pill shows no live
    // text while recording. Upgrade path: mirror hotkey.rs's spawn_partial_loop
    // (CaptureHandle::snapshot + transcribe_opts every ~1.2 s) once this module
    // can be compiled and profiled on Linux hardware.
    std::thread::spawn(|| match whimpr_audio::start(|_: &[f32]| {}) {
        Ok(handle) => {
            *CAPTURE
                .get_or_init(|| Mutex::new(None))
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(handle);
        }
        Err(e) => eprintln!("[whimpr:linux] mic capture failed: {e}"),
    });
}

fn on_ptt_up() {
    if LOCKED.load(Ordering::SeqCst) {
        // Locked: releasing the key must not stop capture; the lock only clears
        // via `finalize_locked_session`.
        return;
    }
    if !RECORDING.swap(false, Ordering::SeqCst) {
        return; // wasn't recording
    }
    LAST_KEY_UP_MS.store(now_ms(), Ordering::SeqCst);
    emit_bar("idle");
    finish_capture_and_finalize(false);
}

/// Ends a locked hands-free session: reached via a third key-down (see
/// `on_ptt_down` above) or a `confirm_dictation` UI stop click.
fn finalize_locked_session() {
    LOCKED.store(false, Ordering::SeqCst);
    RECORDING.store(false, Ordering::SeqCst);
    emit_bar("idle");
    finish_capture_and_finalize(true);
}

/// Stop the current capture, transcribe (with the configured language, long-form
/// for a locked meeting session), and run the finalize pipeline. Shared by the
/// normal push-to-talk release path and the locked-session finalize path.
fn finish_capture_and_finalize(was_locked: bool) {
    let handle = CAPTURE
        .get()
        .and_then(|slot| slot.lock().unwrap_or_else(|e| e.into_inner()).take());
    std::thread::spawn(move || {
        let Some(app) = APP.get() else {
            return;
        };
        let Some(res) = handle.and_then(|h| h.stop()) else {
            eprintln!("[whimpr:linux] no audio captured");
            emit_receipt(
                app,
                ReceiptPayload {
                    ok: false,
                    action: "error",
                    app: target_app(),
                    words: 0,
                    confidence: None,
                    low_words: Vec::new(),
                    message: Some("no audio was captured - check microphone access".to_string()),
                },
            );
            return;
        };
        let Some(asr) = current_asr() else {
            eprintln!("[whimpr:linux] ASR not ready (model still loading or missing)");
            emit_receipt(
                app,
                ReceiptPayload {
                    ok: false,
                    action: "error",
                    app: target_app(),
                    words: 0,
                    confidence: None,
                    low_words: Vec::new(),
                    message: Some("speech model is not loaded yet".to_string()),
                },
            );
            return;
        };
        let settings = current_settings_inner();
        // Long-form transcription only for a hands-free meeting session  -
        // push-to-talk clips stay single-segment.
        let long_form = was_locked && settings.meeting_mode;
        let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
        // An English-only (.en) model always gets "en", never auto-detect.
        let lang = effective_language(settings.language.as_deref());
        match asr.transcribe_opts(&pcm, lang.as_deref(), long_form) {
            Ok(t) => {
                eprintln!("[whimpr:linux] TRANSCRIPT: \"{}\"", t.text);
                finalize_transcript(
                    app,
                    t.text,
                    t.confidence,
                    t.low_words,
                    res.duration_secs(),
                    was_locked,
                );
            }
            Err(e) => {
                eprintln!("[whimpr:linux] ASR error: {e}");
                emit_receipt(
                    app,
                    ReceiptPayload {
                        ok: false,
                        action: "error",
                        app: target_app(),
                        words: 0,
                        confidence: None,
                        low_words: Vec::new(),
                        message: Some(format!("transcription failed: {e}")),
                    },
                );
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

/// Called by the overlay pill's Cancel button (`cancel_dictation` Tauri command)
/// to discard whatever dictation is in flight  -  locked or a normal push-to-talk
/// hold  -  without transcribing it. A no-op when idle.
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

// ── X11 global hotkey grab (XGrabKey  -  see the module doc comment) ─────────────

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
    let count = (max_kc as u16)
        .saturating_sub(min_kc as u16)
        .saturating_add(1) as u8;
    let mapping = conn
        .get_keyboard_mapping(min_kc, count)
        .ok()?
        .reply()
        .ok()?;
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

    let keycode = keycode_for_keysym(&conn, XK_CONTROL_R).ok_or_else(|| {
        anyhow::anyhow!("no keycode maps to XK_Control_R (Right Ctrl) on this keyboard layout")
    })?;

    // NOTE: unverified against the exact x11rb version pinned here  -  if `modifiers`
    // or `pointer_mode`/`keyboard_mode` don't accept `ModMask::ANY` / `GrabMode::ASYNC`
    // directly, adjust to whatever this crate version's grab_key signature expects.
    conn.grab_key(
        true,
        root,
        ModMask::ANY,
        keycode,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?
    .check()?;
    conn.flush()?;
    eprintln!("[whimpr:linux] X11 key grab installed (push-to-talk: Right Ctrl, X11 only  -  see linux.rs doc comment for Wayland)");

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
                "[whimpr:linux] X11 hotkey grab failed: {e}  -  is a display server reachable? \
                 This module only supports X11 (or XWayland); Wayland compositors' native \
                 protocol is not supported yet (see the module doc comment)."
            );
        }
    });
}

// ── Voice Memory key + persistence (mirrors hotkey.rs) ──────────────────────────

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Get (or create, on first run) the voice-memory AES key: 32 random bytes,
/// hex-encoded in the OS keyring (Secret Service on Linux) so the encrypted
/// file on disk is useless without the user's keyring.
fn load_or_create_vm_key() -> Option<[u8; 32]> {
    let entry = keyring::Entry::new("com.whimpr.whimprflow", "voice_memory_key").ok()?;
    if let Ok(hex) = entry.get_password() {
        if let Some(bytes) = hex_decode(hex.trim()) {
            if let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice()) {
                return Some(key);
            }
        }
    }
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key).ok()?;
    entry.set_password(&hex_encode(&key)).ok()?;
    Some(key)
}

/// Persist the memory encrypted; a no-op when the keyring key is unavailable
/// (memory then lives only for this run).
fn save_voice_memory() {
    let (Some(m), Some(key)) = (VOICE_MEMORY.get(), VM_KEY.get()) else {
        return;
    };
    let vm = m.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = vm.save_encrypted(&voice_memory_path(), key) {
        eprintln!("[whimpr:linux] voice memory save failed: {e}");
    }
}

// ── Public surface (everything hotkey.rs's Linux pub use block re-exports) ─────

pub fn install(app: AppHandle) {
    let _ = APP.set(app);
    let _ = CLOCK.set(Instant::now());

    // Load settings + stores, and build cloud providers from stored keys.
    // Loaded synchronously (cheap file read) before the ASR thread below so
    // `whisper_model_path()` can pick an English-only vs. multilingual model
    // file based on the configured language.
    let settings = whimpr_core::Settings::load(&settings_path());
    eprintln!(
        "[whimpr:linux] cleanup mode: {:?}, level: {:?}, language: {:?}",
        settings.cleanup_mode, settings.cleanup_level, settings.language
    );
    let language_for_model = settings.language.clone();
    let retention_days = settings.retention_days;
    let _ = SETTINGS.set(Mutex::new(settings));
    let _ = DICTIONARY.set(Mutex::new(whimpr_core::DictionaryStore::load(&dict_path())));
    let _ = SNIPPETS.set(Mutex::new(
        whimpr_core::SnippetStore::load(&snippets_path()),
    ));
    let _ = STATS.set(Mutex::new(whimpr_core::StatsStore::load(&stats_path())));
    let _ = WORKFLOWS.set(Mutex::new(whimpr_core::WorkflowStore::load(
        &workflows_path(),
    )));
    rebuild_providers();

    // Retention pruning: strip stored text past the window once at startup
    // (it also runs after every new record).
    if let Some(days) = retention_days {
        if let Some(m) = STATS.get() {
            let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
            if store.prune_texts(unix_now(), days) > 0 {
                let _ = store.save(&stats_path());
            }
        }
    }

    // Voice Memory: key from the keyring (created on first run), then the
    // encrypted log from disk. Never a reason the app fails to start.
    match load_or_create_vm_key() {
        Some(key) => {
            let _ = VOICE_MEMORY.set(Mutex::new(whimpr_core::VoiceMemory::load_encrypted(
                &voice_memory_path(),
                &key,
            )));
            let _ = VM_KEY.set(key);
        }
        None => {
            eprintln!(
                "[whimpr:linux] voice memory key unavailable (Secret Service?)  -  memory is \
                 in-memory only this run"
            );
            let _ = VOICE_MEMORY.set(Mutex::new(whimpr_core::VoiceMemory::default()));
        }
    }

    // Load Whisper off the main thread (it takes ~1s).
    std::thread::spawn(move || {
        let path = whisper_model_path(language_for_model.as_deref());
        if !path.exists() {
            eprintln!("[whimpr:linux] ASR model not found at {}", path.display());
            return;
        }
        match whimpr_asr::WhisperEngine::load(&path) {
            Ok(engine) => {
                // Remember which model file loaded (provenance + health chips).
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                set_asr(Arc::new(engine), name);
                eprintln!("[whimpr:linux] ASR ready");
            }
            Err(e) => eprintln!("[whimpr:linux] ASR load failed: {e}"),
        }
    });
    // Start the local cleanup worker in the background.
    std::thread::spawn(|| {
        let worker = crate::local_llm::spawn_default();
        let _ = LOCAL.set(Mutex::new(worker));
    });

    spawn_hotkey_thread();
    eprintln!("[whimpr:linux] installing X11 push-to-talk grab (Right Ctrl)");
}

/// A snapshot of the current settings.
pub fn current_settings() -> whimpr_core::Settings {
    current_settings_inner()
}

/// Apply new settings and rebuild the cloud providers (picks up model
/// changes). Also applies retention immediately (prunes stored text older
/// than the new window) and hot-reloads the whisper engine when a language
/// change needs a different model file.
pub fn update_settings(new: whimpr_core::Settings) {
    let language_changed = current_settings_inner().language != new.language;
    if let Some(m) = SETTINGS.get() {
        *m.lock().unwrap_or_else(|e| e.into_inner()) = new.clone();
    }
    let _ = new.save(&settings_path());
    rebuild_providers();
    // Tightening retention takes effect now, not at the next dictation
    // (mirrors the install()-time prune).
    if let Some(days) = new.retention_days {
        if let Some(m) = STATS.get() {
            let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
            if store.prune_texts(unix_now(), days) > 0 {
                let _ = store.save(&stats_path());
            }
        }
    }
    if language_changed {
        maybe_reload_asr(new.language.as_deref());
    }
}

/// Hot-reload the whisper engine when the configured language and the
/// loaded model file no longer fit: a non-English language on an
/// English-only `.en.bin` model (when a multilingual file exists), or back
/// to English when a better-fitting `.en` model exists. Loads the new
/// model on a background thread and swaps it in  -  the old engine keeps
/// serving until the swap, so dictation never goes dark mid-reload. Also
/// recovers the case where no engine loaded at startup at all.
fn maybe_reload_asr(language: Option<&str>) {
    let target = whisper_model_path(language);
    if !target.exists() {
        return; // nothing to load; keep whatever is serving now
    }
    let target_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let loaded = current_asr_model();
    if loaded.as_deref() == Some(target_name.as_str()) {
        return; // the loaded model is already the best fit
    }
    eprintln!(
        "[whimpr:linux] ASR model change: {}  ->  {target_name} (loading in the background)",
        loaded.as_deref().unwrap_or("<none>")
    );
    std::thread::spawn(move || match whimpr_asr::WhisperEngine::load(&target) {
        Ok(engine) => {
            set_asr(Arc::new(engine), target_name.clone());
            eprintln!("[whimpr:linux] ASR model swapped in: {target_name}");
        }
        Err(e) => {
            eprintln!("[whimpr:linux] ASR hot-reload failed ({e})  -  keeping the current model")
        }
    });
}

/// (Re)build the cloud cleanup providers from the current keys + settings. Called
/// at startup and whenever a key or model changes, so edits take effect live.
pub fn rebuild_providers() {
    let settings = current_settings_inner();
    let openai = read_openai_key().map(|k| {
        whimpr_cleanup::OpenAiProvider::with_base_url(
            k,
            settings.openai_model.clone(),
            Some(settings.openai_base_url.clone()),
        )
    });
    let anthropic = read_anthropic_key()
        .map(|k| whimpr_cleanup::AnthropicProvider::new(k, settings.anthropic_model.clone()));
    eprintln!(
        "[whimpr:linux] cleanup providers: openai={}, anthropic={}",
        openai.is_some(),
        anthropic.is_some()
    );
    match OPENAI.get() {
        Some(m) => *m.lock().unwrap_or_else(|e| e.into_inner()) = openai,
        None => {
            let _ = OPENAI.set(Mutex::new(openai));
        }
    }
    match ANTHROPIC.get() {
        Some(m) => *m.lock().unwrap_or_else(|e| e.into_inner()) = anthropic,
        None => {
            let _ = ANTHROPIC.set(Mutex::new(anthropic));
        }
    }
}

/// Aggregated stats for the Hub. `tz_offset_minutes` is the UI's
/// `Date.getTimezoneOffset()` so day math matches the user's local clock.
pub fn stats_summary(tz_offset_minutes: i32) -> StatsSummary {
    STATS
        .get()
        .map(|m| {
            m.lock()
                .unwrap_or_else(|e| e.into_inner())
                .summary(tz_offset_minutes, unix_now())
        })
        .unwrap_or_else(|| {
            whimpr_core::StatsStore::default().summary(tz_offset_minutes, unix_now())
        })
}

/// The most recent dictations for the Hub Home history list.
pub fn history(limit: usize) -> Vec<whimpr_core::HistoryItem> {
    STATS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).history(limit))
        .unwrap_or_default()
}

/// The most recent records for the Privacy dictation ledger, INCLUDING
/// textless ones (pruned or never stored)  -  the ledger audits provenance
/// for every dictation, not just the ones whose text survives retention.
// dead_code: the Tauri command wrapper in lib.rs lands with the Privacy
// pane's ledger wiring; drop the allow when it does.
#[allow(dead_code)]
pub fn ledger(limit: usize) -> Vec<whimpr_core::HistoryItem> {
    STATS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).ledger(limit))
        .unwrap_or_default()
}

/// The dictionary entries for the Hub Dictionary screen (auto-learned flagged).
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

/// Add a manual dictionary entry and persist.
pub fn dictionary_add(correct: String, mishears: Vec<String>) {
    if let Some(m) = DICTIONARY.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        store.add(correct, mishears, whimpr_core::DictSource::Manual);
        let _ = store.save(&dict_path());
    }
}

/// Remove a dictionary entry by spelling and persist.
pub fn dictionary_remove(correct: &str) {
    if let Some(m) = DICTIONARY.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        if store.remove(correct) {
            let _ = store.save(&dict_path());
        }
    }
}

/// Add an AUTO-learned entry and persist. Marked ✨ auto in the UI.
pub fn dictionary_learn(correct: String, mishears: Vec<String>) {
    if let Some(m) = DICTIONARY.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        store.add(correct, mishears, whimpr_core::DictSource::Auto);
        let _ = store.save(&dict_path());
    }
}

/// The snippet entries for the Hub Snippets screen.
pub fn snippet_entries() -> Vec<whimpr_core::SnippetEntry> {
    SNIPPETS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).entries.clone())
        .unwrap_or_default()
}

/// Add (or, if the trigger already exists, replace) a snippet and persist.
pub fn snippet_add(trigger: String, expansion: String) {
    if let Some(m) = SNIPPETS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        store.add(trigger, expansion);
        let _ = store.save(&snippets_path());
    }
}

/// Remove a snippet by its trigger and persist.
pub fn snippet_remove(trigger: &str) {
    if let Some(m) = SNIPPETS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        if store.remove(trigger) {
            let _ = store.save(&snippets_path());
        }
    }
}

/// Copy every user data store into a timestamped backup folder. Note
/// voice_memory.enc is only decryptable on the same machine  -  its AES
/// key lives in the user's keyring, not in the backup.
pub fn backup_data() -> Result<String, String> {
    whimpr_core::backup::backup_files(
        &[
            ("settings.json", settings_path()),
            ("dictionary.json", dict_path()),
            ("snippets.json", snippets_path()),
            ("stats.json", stats_path()),
            ("workflows.json", workflows_path()),
            ("notes.json", notes_path()),
            ("voice_memory.enc", voice_memory_path()),
        ],
        &support_dir().join("backups"),
    )
    .map(|p| p.display().to_string())
    .map_err(|e| e.to_string())
}

/// Pipeline health for the Hub's health chips.
///
/// ponytail: no Linux permission probes are wired  -  `crate::paste`'s non-macOS
/// stubs report the microphone and accessibility as granted. Upgrade path: a
/// PipeWire/PulseAudio device query for the mic and an X11/portal capability
/// check for input injection.
pub fn get_health() -> crate::hotkey::Health {
    crate::hotkey::Health {
        asr_ready: current_asr().is_some(),
        asr_model: current_asr_model(),
        local_llm_ready: LOCAL
            .get()
            .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).is_some())
            .unwrap_or(false),
        microphone: crate::paste::microphone_granted(),
        accessibility: crate::paste::is_trusted(),
    }
}

/// Delete-all-text: strip stored dictation text (final + raw) from every
/// history record, keeping the numeric stats. Returns how many were stripped.
pub fn clear_history_text() -> usize {
    STATS
        .get()
        .map(|m| {
            let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
            let n = store.clear_texts();
            let _ = store.save(&stats_path());
            n
        })
        .unwrap_or(0)
}

/// Context Capsule is macOS-only this pass, so there is never a last capsule
/// to report on Linux.
///
/// ponytail: v1 ceiling is no capsule capture at all on Linux (no selected-text
/// read, no glossary, no per-app allow-list check). Upgrade path: read the
/// focused selection over AT-SPI2 (the accessibility bus), key the allow-list
/// on WM_CLASS, and mirror hotkey.rs's `capture_capsule` at record start.
pub fn get_last_capsule() -> Option<crate::hotkey::CapsuleReport> {
    None
}

/// The workflow entries for the Hub Workflows screen.
pub fn workflow_entries() -> Vec<whimpr_core::WorkflowEntry> {
    WORKFLOWS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).entries.clone())
        .unwrap_or_default()
}

/// Add (or update, keyed by name) a workflow and persist. An update bumps
/// the version and archives the prior revision.
pub fn workflow_add(
    name: String,
    trigger: String,
    instruction: String,
    destination: WorkflowDestination,
    require_approval: bool,
) {
    if let Some(m) = WORKFLOWS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        store.add(
            name,
            trigger,
            instruction,
            destination,
            require_approval,
            unix_now(),
        );
        let _ = store.save(&workflows_path());
    }
}

/// Remove a workflow by its name and persist.
pub fn workflow_remove(name: &str) {
    if let Some(m) = WORKFLOWS.get() {
        let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
        if store.remove(name) {
            let _ = store.save(&workflows_path());
        }
    }
}

/// The workflow result currently held for approval, if any. Lets the
/// Workflows pane seed itself on mount  -  the `whimpr://pending` event is
/// fire-and-forget and may have fired before the pane existed.
// dead_code: the Tauri command wrapper in lib.rs lands with the Workflows
// pane's seeding wiring; drop the allow when it does.
#[allow(dead_code)]
pub fn get_pending() -> Option<crate::hotkey::PendingPayload> {
    let slot = PENDING.get()?;
    let guard = slot.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|item| crate::hotkey::PendingPayload {
        name: item.name.clone(),
        preview: truncate_chars(&item.text, PREVIEW_CHARS),
    })
}

/// Approve the held workflow result: execute its destination now.
///
/// Clicking Approve focused the Hub, so a Paste destination would land in
/// the Hub window itself  -  and this pass has no X11 helper to hand focus
/// back to the app the user dictated into. Deliver to the clipboard instead
/// and say so in the receipt.
///
/// ponytail: no window re-activation on X11 in this pass, so approve-paste
/// ALWAYS downgrades to the clipboard. Upgrade path: `wmctrl -x -a` (or a
/// raw `_NET_ACTIVE_WINDOW` client message via x11rb) against the stored
/// `target_app` WM_CLASS, then poll `foreground_app()` until it matches
/// before pasting  -  mirroring macOS's `appctx::activate_app`.
pub fn approve_pending() {
    let item = PENDING
        .get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).take());
    let (Some(item), Some(app)) = (item, APP.get()) else {
        return;
    };
    let mut destination = item.destination;
    let mut note = None;
    if matches!(destination, WorkflowDestination::Paste) {
        destination = WorkflowDestination::Clipboard;
        note = Some(match item.target_app.as_deref() {
            Some(target) => {
                format!("cannot refocus {target} on X11 yet - copied to the clipboard instead")
            }
            None => "cannot refocus the target app - copied to the clipboard instead".to_string(),
        });
    }
    deliver_workflow(
        app,
        &item.name,
        item.text,
        destination,
        item.raw,
        item.confidence,
        item.low_words,
        item.duration_secs,
        note,
    );
}

/// Discard the held workflow result without executing it.
pub fn reject_pending() {
    if let Some(m) = PENDING.get() {
        *m.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Append one learned correction to Voice Memory and persist (encrypted).
pub fn voice_memory_record(from: String, to: String, source: &str) {
    let Some(m) = VOICE_MEMORY.get() else {
        return;
    };
    m.lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(from, to, source.to_string(), unix_now());
    save_voice_memory();
}

/// The correction audit list for the Voice Memory pane.
pub fn get_voice_memory() -> Vec<whimpr_core::CorrectionEvent> {
    VOICE_MEMORY
        .get()
        .map(|m| {
            m.lock()
                .unwrap_or_else(|e| e.into_inner())
                .corrections
                .clone()
        })
        .unwrap_or_default()
}

/// Write the plain-JSON export bundle (corrections + dictionary + snippets +
/// style) into the app's exports folder and return its path.
pub fn export_voice_memory() -> Result<String, String> {
    let vm = VOICE_MEMORY
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default();
    let dict = DICTIONARY
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default();
    let snippets = SNIPPETS
        .get()
        .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default();
    let style = current_settings_inner().style;
    let bundle = vm.export_bundle(&dict, &snippets, &style);
    let dir = support_dir().join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("voice-memory-{}.json", unix_now()));
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Wipe the correction log (the dictionary itself is managed separately).
pub fn clear_voice_memory() {
    if let Some(m) = VOICE_MEMORY.get() {
        m.lock()
            .unwrap_or_else(|e| e.into_inner())
            .corrections
            .clear();
    }
    save_voice_memory();
}

/// Screen capture is macOS-only this pass.
///
/// ponytail: v1 ceiling is no capture at all on Linux. Upgrade path: shell out
/// to a screenshot tool when present (`gnome-screenshot`/`scrot` on X11, `grim`
/// on wlroots) or call the xdg-desktop-portal Screenshot API, writing into the
/// same `captures/` folder macOS uses.
pub fn capture_screen() -> Result<String, String> {
    Err("not implemented on this platform yet".to_string())
}
