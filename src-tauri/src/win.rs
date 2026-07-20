//! Windows platform layer for WhimprFlow: a low-level keyboard hook for
//! push-to-talk, clipboard+SendInput text injection, and foreground-app detection,
//! plus the same dictation pipeline (audio → Whisper ASR → cleanup LLM → paste) and
//! the Hub-facing settings/stats/dictionary functions the Tauri commands call.
//!
//! Roadmap-15 parity with `hotkey.rs` (the macOS layer): CleanOutcome provenance,
//! `record_full` history records (raw + provenance + confidence + low_words),
//! retention pruning, language threading via `transcribe_opts`, workflow trigger
//! routing with pending approve/reject, meeting-mode notes, Voice Memory
//! (encrypted log + export), receipt events, health, and the workflows CRUD.
//! Context Capsule and streaming preview stay macOS-only this pass (see the
//! `ponytail:` notes at `get_last_capsule` and `on_ptt_down`); `capture_screen`
//! errors here.
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
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RWIN, VK_SHIFT, VK_SPACE, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW,
    HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use whimpr_core::state::timing::DOUBLE_TAP_MS;
use whimpr_core::{
    CleanupContext, CleanupMode, CleanupProvider, StatsSummary, WorkflowDestination,
};

const OVERLAY_LABEL: &str = "whimpr_bar";
/// The Hub window's label  -  receipts and pending-approval events go to both
/// the overlay pill and the Hub (same as `hotkey.rs`).
const HUB_LABEL: &str = "main";
/// Truncation length for the pending-approval previews.
const PREVIEW_CHARS: usize = 200;
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
/// Executable name of the app that was foreground at record start = the paste
/// target (the Windows analogue of macOS's bundle-id TARGET_APP). Cleanup uses
/// it to format for the medium; receipts and history records report it.
static TARGET_APP: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static CAPTURE: OnceLock<Mutex<Option<whimpr_audio::CaptureHandle>>> = OnceLock::new();
/// The loaded whisper engine, hot-swappable when a language change needs a
/// different model file (see [`maybe_reload_asr`]). `Arc` so in-flight
/// transcriptions keep the old engine alive across a swap.
static ASR: OnceLock<Mutex<Option<Arc<whimpr_asr::WhisperEngine>>>> = OnceLock::new();
static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
static OPENAI: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>> = OnceLock::new();
static ANTHROPIC: OnceLock<Mutex<Option<whimpr_cleanup::AnthropicProvider>>> = OnceLock::new();
static SETTINGS: OnceLock<Mutex<whimpr_core::Settings>> = OnceLock::new();
static DICTIONARY: OnceLock<Mutex<whimpr_core::DictionaryStore>> = OnceLock::new();
static SNIPPETS: OnceLock<Mutex<whimpr_core::SnippetStore>> = OnceLock::new();
static STATS: OnceLock<Mutex<whimpr_core::StatsStore>> = OnceLock::new();
/// (raw pre-cleanup text, final pasted text) from the most recent dictation  -
/// feeds the "undo last cleanup edit" hotkey (Ctrl+Alt+Z). `None` until the first
/// dictation completes this run.
static LAST_TEXTS: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();
/// The user's voice workflows (trigger -> command-edit instruction).
static WORKFLOWS: OnceLock<Mutex<whimpr_core::WorkflowStore>> = OnceLock::new();
/// Voice Memory (encrypted at rest) and its credential-manager-held AES key.
/// The key slot stays empty when the credential store is unavailable  -  memory
/// then lives only for this run and saves are skipped.
static VOICE_MEMORY: OnceLock<Mutex<whimpr_core::VoiceMemory>> = OnceLock::new();
static VM_KEY: OnceLock<[u8; 32]> = OnceLock::new();
/// File name of the whisper model actually loaded (for provenance + health).
/// Swapped together with [`ASR`] on a hot reload  -  always set via
/// [`set_asr`] so the pair stays consistent.
static ASR_MODEL_NAME: OnceLock<Mutex<Option<String>>> = OnceLock::new();
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
    /// Executable name of the app the user dictated into (the TARGET_APP
    /// snapshot at creation time)  -  clicking Approve makes the Hub
    /// foreground, so `approve_pending` names it when a Paste approval has
    /// to downgrade to the clipboard.
    target_app: Option<String>,
}

/// What `clean_transcript` produced: the normalized raw transcript (what
/// "undo cleanup" restores), the text to insert, and where it came from.
struct CleanOutcome {
    raw_out: String,
    final_text: String,
    provenance: whimpr_core::Provenance,
}

#[derive(Clone, serde::Serialize)]
struct TranscriptPayload {
    text: String,
}

/// The insertion receipt emitted after every finalize (spec: whimpr://receipt).
/// Same field names and shapes as `hotkey.rs`'s ReceiptPayload.
#[derive(Clone, serde::Serialize)]
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

/// Copy every user data store into a timestamped backup folder under
/// `support_dir()/backups/`. Note voice_memory.enc is only decryptable on
/// the same machine  -  its AES key lives in the user's credential store,
/// not in the backup. Mirrors `hotkey.rs`'s `backup_data`.
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

/// `.en`-suffixed models are English-only, so when a specific non-English
/// language is selected we only consider multilingual model files (no `.en`
/// suffix); otherwise `.en` models are preferred first for better English
/// accuracy, falling back to multilingual files if none are present.
fn whisper_model_path(language: Option<&str>) -> std::path::PathBuf {
    let dir = support_dir().join("models");
    let needs_multilingual = matches!(language, Some(lang) if lang != "en");
    const MULTILINGUAL: &[&str] = &[
        "ggml-medium.bin",
        "ggml-medium-q8_0.bin",
        "ggml-small.bin",
        "ggml-small-q8_0.bin",
        "ggml-base.bin",
        "ggml-base-q8_0.bin",
    ];
    // "ggml-distil-large-v3.5.bin": rename after downloading from
    // distil-whisper/distil-large-v3.5-ggml (ships as generic
    // "ggml-model.bin"). Same weight class as medium.en but distilled from
    // large-v3: meaningfully more accurate, ~4x faster than large-v2;
    // English-only. "ggml-medium-32-2.en.bin" / "ggml-distil-small.en.bin"
    // are distil-whisper's medium.en/small.en distillations - same accuracy
    // class, faster. "-q8_0" files are 8-bit quantized ggml weights:
    // near-lossless WER, roughly half the file size of the full model.
    const ENGLISH_FIRST: &[&str] = &[
        "ggml-distil-large-v3.5.bin",
        "ggml-medium-32-2.en.bin",
        "ggml-medium.en.bin",
        "ggml-medium.en-q8_0.bin",
        "ggml-distil-small.en.bin",
        "ggml-small.en.bin",
        "ggml-small.en-q8_0.bin",
        "ggml-base.en.bin",
        "ggml-base.en-q8_0.bin",
        "ggml-medium.bin",
        "ggml-medium-q8_0.bin",
        "ggml-small.bin",
        "ggml-small-q8_0.bin",
        "ggml-base.bin",
        "ggml-base-q8_0.bin",
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
            "[whimpr:win] no multilingual whisper model found for language {:?}  -  falling \
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
    dir.join("ggml-base.en.bin")
}

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

fn emit_bar(state: &'static str) {
    if let Some(app) = APP.get() {
        #[derive(Clone, serde::Serialize)]
        struct P {
            state: &'static str,
        }
        let _ = app.emit_to(OVERLAY_LABEL, "whimpr://flowbar/state", P { state });
    }
}

/// Emit the insertion receipt to both the overlay pill and the Hub
/// (same event name + payload as `hotkey.rs`).
fn emit_receipt(payload: ReceiptPayload) {
    eprintln!(
        "[whimpr:win] receipt: ok={} action={} words={}",
        payload.ok, payload.action, payload.words
    );
    let Some(app) = APP.get() else { return };
    let _ = app.emit_to(OVERLAY_LABEL, "whimpr://receipt", payload.clone());
    let _ = app.emit_to(HUB_LABEL, "whimpr://receipt", payload);
}

/// Announce a workflow result awaiting approval, to both windows. Uses the
/// shared `hotkey::PendingPayload` shape, same as `get_pending` returns.
fn emit_pending(name: &str, preview: &str) {
    let Some(app) = APP.get() else { return };
    let payload = crate::hotkey::PendingPayload {
        name: name.to_string(),
        preview: preview.to_string(),
    };
    let _ = app.emit_to(OVERLAY_LABEL, "whimpr://pending", payload.clone());
    let _ = app.emit_to(HUB_LABEL, "whimpr://pending", payload);
}

/// Show the final inserted text on the overlay pill (same event as macOS).
fn emit_transcript(text: String) {
    let Some(app) = APP.get() else { return };
    let _ = app.emit_to(
        OVERLAY_LABEL,
        "whimpr://transcript",
        TranscriptPayload { text },
    );
}

/// Executable name of the app that was foreground at record start, if any.
fn target_app() -> Option<String> {
    TARGET_APP
        .get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

/// First `n` chars of `s` (for event previews).
fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
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
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
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

/// Read an API key from an env var or the OS credential store (never a
/// plaintext file). Mirrors `hotkey.rs`'s `read_key`.
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

/// Clean a raw transcript per the current settings (mode + level), feeding in the
/// dictionary vocabulary relevant to this utterance. Falls back to raw whenever
/// cleanup is off, the provider is unavailable, it errors, or the gates reject it.
///
/// Returns a [`CleanOutcome`]: `raw_out` is the pre-cleanup transcript after
/// `pre_normalize_layout`/`post_process` (what would be pasted if cleanup were
/// skipped or rejected  -  used by the "undo last cleanup edit" hotkey to restore
/// the un-cleaned text), `final_text` is what actually gets pasted, and
/// `provenance` says which route shaped it (Privacy ledger).
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
    // and any raw fallback use `raw_out` (markers restored to real breaks) so we
    // never paste a "[[NL]]" token or lose an explicit break.
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
        eprintln!("[whimpr:win] cleanup target app: {app}");
    }
    // Code Mode: the code-dictation prompt variant, when the paste target is
    // an IDE/terminal and the user hasn't opted out.
    let code_mode = settings.code_mode_auto
        && app_bundle_id
            .as_deref()
            .map(whimpr_core::cleanup::prompts::is_code_app)
            .unwrap_or(false);
    if code_mode {
        eprintln!("[whimpr:win] code mode active for this cleanup");
    }
    // ponytail: Context Capsule is macOS-only this pass, so cleanup never gets
    // window context here. Upgrade path: read the focused element's selection
    // via UI Automation (ITextPattern) at record start, same as the AX read
    // `hotkey.rs` feeds into `CleanupContext.window_context`.
    let ctx = CleanupContext {
        level,
        vocab,
        app_bundle_id,
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
            // lines. Guarantees no "new line"/"new paragraph" word reaches the cursor.
            let cleaned = whimpr_core::cleanup::post_process(&cleaned);
            provenance.cleanup = route;
            if whimpr_core::cleanup::evaluate_gates(&raw_out, &cleaned, level).passed() {
                provenance.gate = "passed".to_string();
                cleaned
            } else {
                eprintln!("[whimpr:win] cleanup gate rejected the edit  -  pasting raw");
                provenance.gate = "rejected".to_string();
                raw_out.clone()
            }
        }
        Some(Err(e)) => {
            // Provider errored: the final text is raw ("raw"/"skipped" stand),
            // but sent_to_cloud stays honest  -  the transcript may have left
            // the machine even though no edit came back.
            eprintln!("[whimpr:win] cleanup failed ({e})  -  pasting raw");
            raw_out.clone()
        }
        None => {
            if matches!(settings.cleanup_mode, CleanupMode::Local) {
                eprintln!("[whimpr:win] local cleanup model not wired yet  -  pasting raw");
            } else {
                eprintln!("[whimpr:win] cleanup provider has no API key  -  pasting raw");
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

/// Note title for meeting-mode transcripts. macOS shells out to `date` for a
/// local "YYYY-MM-DD HH:MM" title; there is no equivalent command to lean on
/// here and std has no local-time API.
///
/// ponytail: v1 ceiling is a Unix-timestamp title ("Meeting 1752900000").
/// Upgrade path: `GetLocalTime` via the `Win32_System_SystemInformation`
/// feature (not currently enabled in Cargo.toml), or a time crate.
fn local_datetime_title() -> String {
    format!("Meeting {}", unix_now())
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
    // Snapshot the paste target now, while the user's app is still foreground
    // (mirrors macOS's Fn-down TARGET_APP snapshot).
    *TARGET_APP
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = foreground_app();
    let now = now_ms();
    let last_up = LAST_KEY_UP_MS.load(Ordering::SeqCst);
    let is_double_tap = last_up != u64::MAX && now.saturating_sub(last_up) <= DOUBLE_TAP_MS;
    if is_double_tap {
        LOCKED.store(true, Ordering::SeqCst);
        emit_bar("locked");
    } else {
        emit_bar("recording");
    }
    // ponytail: streaming preview (whimpr://transcript/partial) is macOS-only
    // this pass  -  no partial-transcription loop is spawned here. Upgrade
    // path: the same `CaptureHandle::snapshot()` loop `hotkey.rs` runs
    // (`spawn_partial_loop`, ~1.2 s cadence); nothing about it is
    // macOS-specific beyond having only been verified there.
    std::thread::spawn(|| match whimpr_audio::start(|_: &[f32]| {}) {
        Ok(handle) => {
            *CAPTURE
                .get_or_init(|| Mutex::new(None))
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(handle);
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
    finish_capture_and_paste(false);
}

/// Ends a locked hands-free session: reached via a third key-down (see
/// `on_ptt_down` above) or a `confirm_dictation` UI stop click. Mirrors
/// `on_ptt_up`'s finalize path but is reachable without a matching key-up.
fn finalize_locked_session() {
    LOCKED.store(false, Ordering::SeqCst);
    RECORDING.store(false, Ordering::SeqCst);
    emit_bar("idle");
    finish_capture_and_paste(true);
}

/// Stop the current capture, transcribe (with the configured language;
/// long-form for a locked meeting session), and run the finalize pipeline.
/// Shared by the normal push-to-talk release path (`on_ptt_up`) and the
/// locked-session finalize path (`finalize_locked_session`).
fn finish_capture_and_paste(was_locked: bool) {
    let handle = CAPTURE
        .get()
        .and_then(|slot| slot.lock().unwrap_or_else(|e| e.into_inner()).take());
    std::thread::spawn(move || {
        let Some(res) = handle.and_then(|h| h.stop()) else {
            eprintln!("[whimpr:win] no audio captured");
            emit_receipt(ReceiptPayload {
                ok: false,
                action: "error",
                app: target_app(),
                words: 0,
                confidence: None,
                low_words: Vec::new(),
                message: Some("no audio was captured - check microphone access".to_string()),
            });
            return;
        };
        let Some(asr) = current_asr() else {
            eprintln!("[whimpr:win] ASR not ready (model still loading or missing)");
            emit_receipt(ReceiptPayload {
                ok: false,
                action: "error",
                app: target_app(),
                words: 0,
                confidence: None,
                low_words: Vec::new(),
                message: Some("speech model is not loaded yet".to_string()),
            });
            return;
        };
        let settings = current_settings_inner();
        // Long-form transcription only for a hands-free meeting session  -
        // push-to-talk clips stay single-segment.
        let long_form = was_locked && settings.meeting_mode;
        let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
        let lang = effective_language(settings.language.as_deref());
        match asr.transcribe_opts(&pcm, lang.as_deref(), long_form) {
            Ok(t) => {
                eprintln!("[whimpr:win] TRANSCRIPT: \"{}\"", t.text);
                finalize_transcript(
                    t.text,
                    t.confidence,
                    t.low_words,
                    res.duration_secs(),
                    was_locked,
                );
            }
            Err(e) => {
                eprintln!("[whimpr:win] ASR error: {e}");
                emit_receipt(ReceiptPayload {
                    ok: false,
                    action: "error",
                    app: target_app(),
                    words: 0,
                    confidence: None,
                    low_words: Vec::new(),
                    message: Some(format!("transcription failed: {e}")),
                });
            }
        }
    });
}

/// The full post-ASR pipeline for one finalized dictation, in spec order:
/// workflow trigger -> snippet -> cleanup, then meeting-note or paste  -
/// with a receipt emitted and the session recorded on every path. Mirrors
/// `hotkey.rs`'s `finalize_transcript`.
fn finalize_transcript(
    raw: String,
    confidence: Option<f32>,
    low_words: Vec<String>,
    duration_secs: f32,
    was_locked: bool,
) {
    let settings = current_settings_inner();
    if raw.is_empty() {
        emit_receipt(ReceiptPayload {
            ok: false,
            action: "error",
            app: target_app(),
            words: 0,
            confidence,
            low_words,
            message: Some("no speech detected".to_string()),
        });
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
        eprintln!("[whimpr:win] WORKFLOW \"{}\" matched", entry.name);
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
                    emit_pending(&entry.name, &truncate_chars(&text, PREVIEW_CHARS));
                    emit_receipt(ReceiptPayload {
                        ok: true,
                        action: "pending",
                        app: target_app(),
                        words: whimpr_core::stats::count_words(&text),
                        confidence,
                        low_words,
                        message: None,
                    });
                } else {
                    deliver_workflow(
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
                    "[whimpr:win] workflow \"{}\" failed ({e})  -  falling back to a normal \
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
            eprintln!("[whimpr:win] SNIPPET matched  -  pasting expansion directly");
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
                eprintln!("[whimpr:win] CLEANED:   \"{}\"", outcome.final_text);
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
        emit_receipt(ReceiptPayload {
            ok: false,
            action: "error",
            app: target_app(),
            words: 0,
            confidence,
            low_words,
            message: Some("nothing to insert".to_string()),
        });
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
        emit_receipt(ReceiptPayload {
            ok: true,
            action: "noted",
            app: target_app(),
            words,
            confidence,
            low_words,
            message: workflow_note,
        });
        emit_transcript(text);
        return;
    }

    // Paste into the target app; the receipt reports the outcome either way.
    let paste_result = paste_text(&text);
    if let Err(e) = &paste_result {
        eprintln!("[whimpr:win] paste failed: {e}");
    }
    // Stash (raw, final) for the "undo last cleanup edit" hotkey
    // (Ctrl+Alt+Z), right after the paste.
    *LAST_TEXTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some((raw_out.clone(), text.clone()));
    // Log words + speaking time for the Hub stats (WPM, streak…).
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
            // Watch the field for a post-paste correction to learn (a no-op
            // stub off-macOS today; kept for pipeline-shape parity).
            crate::autolearn::watch_correction(&text);
            emit_receipt(ReceiptPayload {
                ok: true,
                action: "pasted",
                app: target_app(),
                words,
                confidence,
                low_words,
                message: workflow_note,
            });
        }
        Err(e) => emit_receipt(ReceiptPayload {
            ok: false,
            action: "error",
            app: target_app(),
            words,
            confidence,
            low_words,
            message: Some(format!("paste failed: {e}")),
        }),
    }
    emit_transcript(text);
}

/// Send a workflow result to its destination, record it, and emit the
/// receipt. `note` is an optional caller-supplied context line for the
/// receipt (e.g. "copied to the clipboard instead"); a destination
/// failure's own error message takes precedence.
#[allow(clippy::too_many_arguments)]
fn deliver_workflow(
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
    emit_receipt(ReceiptPayload {
        ok,
        action,
        app: target_app(),
        words,
        confidence,
        low_words,
        message: message.or(note),
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
        .and_then(|m| {
            m.lock()
                .unwrap_or_else(|e| e.into_inner())
                .history(1)
                .into_iter()
                .next()
                .map(|r| r.text)
        })
        .filter(|t| !t.is_empty())
        // "Never store" retention (Some(0)) keeps text out of the stats
        // store, but the most recent dictation of this run is still in
        // memory for the undo pair  -  use its final text so
        // paste-last/copy-last keep working under max privacy.
        .or_else(|| {
            LAST_TEXTS
                .get()
                .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
                .map(|(_raw, final_text)| final_text)
        })
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
    let pair = LAST_TEXTS
        .get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
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

/// Run a workflow's instruction-following edit through whichever cleanup
/// provider is configured (cloud). Errors when only a local provider is
/// available, since the local worker's command-edit path isn't wired in this
/// build  -  mirrors `hotkey.rs`'s `run_command_edit`.
fn run_command_edit(selection: &str, instruction: &str) -> anyhow::Result<String> {
    let settings = current_settings_inner();
    let run_local = |_selection: &str, _instruction: &str| -> anyhow::Result<String> {
        anyhow::bail!(
            "local Command Mode is unavailable in this build. Set Cleanup Engine to OpenAI \
             or Anthropic in Settings to use Workflows or Command Mode"
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
                (
                    &CANCEL_KEY_DOWN,
                    vk_for_key(bindings.cancel.key),
                    bindings.cancel,
                    cancel_dictation,
                ),
                (
                    &PASTE_LAST_KEY_DOWN,
                    vk_for_key(bindings.paste_last.key),
                    bindings.paste_last,
                    paste_last_transcript,
                ),
                (
                    &COPY_LAST_KEY_DOWN,
                    vk_for_key(bindings.copy_last.key),
                    bindings.copy_last,
                    copy_last_transcript,
                ),
                (
                    &UNDO_LAST_KEY_DOWN,
                    vk_for_key(bindings.undo_last.key),
                    bindings.undo_last,
                    undo_last_cleanup,
                ),
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

    // Voice Memory: key from the credential store (created on first run), then
    // the encrypted log from disk. Never a reason the app fails to start.
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
                "[whimpr:win] voice memory key unavailable (credential manager?)  -  memory \
                 is in-memory only this run"
            );
            let _ = VOICE_MEMORY.set(Mutex::new(whimpr_core::VoiceMemory::default()));
        }
    }

    // Load Whisper.
    std::thread::spawn(move || {
        let path = whisper_model_path(language_for_model.as_deref());
        if !path.exists() {
            eprintln!("[whimpr:win] ASR model not found at {}", path.display());
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
                eprintln!("[whimpr:win] ASR ready");
            }
            Err(e) => eprintln!("[whimpr:win] ASR load failed: {e}"),
        }
    });
    // Start the local cleanup worker.
    std::thread::spawn(|| {
        let worker = crate::local_llm::spawn_default();
        let _ = LOCAL.set(Mutex::new(worker));
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

/// Apply new settings and rebuild the cloud providers (picks up model
/// changes). Also applies retention immediately (prunes stored text older
/// than the new window) and hot-reloads the whisper engine when a language
/// change needs a different model file. Mirrors `hotkey.rs`'s `update_settings`.
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
        "[whimpr:win] ASR model change: {}  ->  {target_name} (loading in the background)",
        loaded.as_deref().unwrap_or("<none>")
    );
    std::thread::spawn(move || match whimpr_asr::WhisperEngine::load(&target) {
        Ok(engine) => {
            set_asr(Arc::new(engine), target_name.clone());
            eprintln!("[whimpr:win] ASR model swapped in: {target_name}");
        }
        Err(e) => {
            eprintln!("[whimpr:win] ASR hot-reload failed ({e})  -  keeping the current model")
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
        "[whimpr:win] cleanup providers: openai={}, anthropic={}",
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

/// Pipeline health for the Hub's health chips. Windows has no TCC-style
/// microphone/Accessibility gates for this pipeline, so `crate::paste`'s
/// non-macOS stubs report both as granted.
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

/// ponytail: Context Capsule is macOS-only this pass (the capture reads the
/// focused element's AX-selected text), so no capsule is ever captured here and
/// this always returns `None`. Upgrade path: capture the foreground app +
/// UI Automation (ITextPattern) selection at record start and fill the same
/// `CapsuleReport` shape `hotkey.rs` builds.
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
/// Clicking Approve made the Hub foreground, so a Paste destination would
/// land in the Hub window itself, not the app the user dictated into.
///
/// ponytail: Windows has no re-activation helper yet (macOS's
/// `appctx::activate_app`), so a Paste approval always downgrades to the
/// clipboard, with the receipt saying so honestly. Upgrade path: remember
/// the target HWND at record start and `SetForegroundWindow` it here
/// before pasting (the stored `target_app` names which app to refocus).
pub fn approve_pending() {
    let item = PENDING
        .get()
        .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).take());
    let Some(item) = item else {
        return;
    };
    let mut destination = item.destination;
    let mut note = None;
    if matches!(destination, WorkflowDestination::Paste) {
        eprintln!(
            "[whimpr:win] approve: cannot refocus {}  -  delivering to the clipboard",
            item.target_app.as_deref().unwrap_or("the target app")
        );
        destination = WorkflowDestination::Clipboard;
        note = Some(
            "cannot refocus the target app on Windows yet - copied to the clipboard instead"
                .to_string(),
        );
    }
    deliver_workflow(
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
/// hex-encoded in the OS credential store so the encrypted file on disk is
/// useless without the user's account.
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

/// Persist the memory encrypted; a no-op when the credential-store key is
/// unavailable (memory then lives only for this run).
fn save_voice_memory() {
    let (Some(m), Some(key)) = (VOICE_MEMORY.get(), VM_KEY.get()) else {
        return;
    };
    let vm = m.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = vm.save_encrypted(&voice_memory_path(), key) {
        eprintln!("[whimpr:win] voice memory save failed: {e}");
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

/// ponytail: screen capture is macOS-only this pass (`screencapture -x`).
/// Upgrade path: GDI BitBlt of the virtual screen (or the Windows.Graphics.
/// Capture API), written as a PNG into `support_dir()/captures/`.
pub fn capture_screen() -> Result<String, String> {
    Err("not implemented on this platform yet".to_string())
}
