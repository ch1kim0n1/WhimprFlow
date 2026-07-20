//! Hold-Fn → pill wiring for the demo shell.
//!
//! This installs an in-process CoreGraphics event tap that feeds Fn key-down /
//! key-up into the real [`whimpr_core`] dictation state machine, and turns the
//! machine's actions into `whimpr://flowbar/state` events the overlay pill
//! renders. There is no audio or ASR yet, so a finalized session is simulated as
//! completing shortly after key release  -  enough to see the full
//! recording → transcribing → done → idle loop driven by the actual state machine.
//!
//! In the shipping product this hook lives in a separate sidecar process (so heavy
//! inference can't stall it); running it in-process is an acceptable macOS-only
//! path for this demo and the early milestones.

/// Dictionary entry shape sent to the Hub UI (auto-learned entries flagged).
#[derive(Clone, serde::Serialize)]
pub struct DictEntryDto {
    pub correct: String,
    pub mishears: Vec<String>,
    pub auto: bool,
}

/// Pipeline health for the Hub's health chips: is speech-to-text loaded (and
/// which model file), is the local cleanup model up, and are the two macOS
/// permissions granted.
#[derive(Clone, serde::Serialize)]
pub struct Health {
    pub asr_ready: bool,
    pub asr_model: Option<String>,
    pub local_llm_ready: bool,
    pub microphone: bool,
    pub accessibility: bool,
}

/// What the last Context Capsule contained, for the Privacy pane  -  so the user
/// sees exactly what a cleanup request would include. `selection_preview` is
/// truncated for display; `enabled` reflects the CURRENT setting.
#[derive(Clone, serde::Serialize)]
pub struct CapsuleReport {
    pub app: Option<String>,
    pub selection_preview: Option<String>,
    pub glossary: Vec<String>,
    pub style: bool,
    pub enabled: bool,
}

/// A workflow result awaiting approval (spec: whimpr://pending). Emitted as
/// the fire-and-forget `whimpr://pending` event and returned by `get_pending`
/// so the Workflows pane can seed itself on mount.
#[derive(Clone, serde::Serialize)]
pub struct PendingPayload {
    pub name: String,
    pub preview: String,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{CapsuleReport, DictEntryDto, Health, PendingPayload};
    use std::os::raw::c_void;
    use std::path::PathBuf;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter};
    use whimpr_core::state::{Action, BarState};
    use whimpr_core::{
        CleanupContext, CleanupMode, CleanupProvider, Input, PipelineEvent, StateMachine,
        TriggerToken, WorkflowDestination,
    };
    use whimpr_ipc::BindingId;

    const OVERLAY_LABEL: &str = "whimpr_bar";
    /// The Hub window's label  -  receipts and pending-approval events go to both
    /// the overlay pill and the Hub.
    const HUB_LABEL: &str = "main";
    /// Cadence of the streaming-preview partial transcriptions.
    const PARTIAL_INTERVAL: Duration = Duration::from_millis(1200);
    /// Truncation length for the pending-approval / capsule previews.
    const PREVIEW_CHARS: usize = 200;

    // --- CoreGraphics / CoreFoundation FFI (listen-only Fn tap) -----------
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CGEventRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;
    type CGEventTapCallBack =
        extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetFlags(event: CGEventRef) -> u64;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        static kCFRunLoopDefaultMode: CFStringRef;
    }

    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT: u32 = 0;
    const K_CG_TAP_OPTION_LISTEN_ONLY: u32 = 1;
    const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
    // kCGEventKeyDown  -  needed (in addition to FlagsChanged) so we can detect the
    // Cmd+Shift+V / Cmd+Shift+C "paste/copy last transcript", Cmd+Shift+Z "undo
    // last cleanup edit", and plain Escape "cancel dictation" keys below; the Fn
    // push-to-talk key alone only ever needs FlagsChanged since it has no keycode.
    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const EVENTS_OF_INTEREST: u64 = (1 << K_CG_EVENT_FLAGS_CHANGED) | (1 << K_CG_EVENT_KEY_DOWN);
    const FLAG_SECONDARY_FN: u64 = 0x0080_0000;
    // CGEventFlags modifier bits (CGEventTypes.h). COMMAND is also used by
    // `paste::post_cmd_v`; the other three are new here for the hotkey combos.
    const KCG_FLAG_MASK_COMMAND: u64 = 0x0010_0000;
    const KCG_FLAG_MASK_SHIFT: u64 = 0x0002_0000;
    const KCG_FLAG_MASK_CONTROL: u64 = 0x0004_0000;
    const KCG_FLAG_MASK_ALTERNATE: u64 = 0x0008_0000;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
    // kCGKeyboardEventAutorepeat  -  nonzero for the synthetic repeat keydowns the OS
    // sends while a key is held. We only want to fire once per physical press.
    const K_CG_KEYBOARD_EVENT_AUTOREPEAT: u32 = 8;
    const KEYCODE_FN: i64 = 63;
    // Standard macOS virtual keycodes for the letters used in the hotkey combos.
    // "Undo last cleanup edit" hotkey: Cmd+Shift+Z. Re-pastes the raw (pre-cleanup)
    // transcript. Deliberately not plain Cmd+Z (universal undo, would surprise the
    // user in the target app) and distinct from the Cmd+Shift+V/C combos above.
    const K_CG_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    const K_CG_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static MACHINE: OnceLock<Mutex<StateMachine>> = OnceLock::new();
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    static FN_IS_DOWN: AtomicBool = AtomicBool::new(false);
    static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
    /// Bundle id of the app that was frontmost at record-start = the paste target.
    /// Cleanup uses it to format for the medium (email vs. text vs. chat).
    static TARGET_APP: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    static CAPTURE: OnceLock<Mutex<Option<whimpr_audio::CaptureHandle>>> = OnceLock::new();
    /// The loaded whisper engine, hot-swappable when a language change needs a
    /// different model file (see [`maybe_reload_asr`]). `Arc` so in-flight
    /// transcriptions keep the old engine alive across a swap.
    static ASR: OnceLock<Mutex<Option<Arc<whimpr_asr::WhisperEngine>>>> = OnceLock::new();
    static OPENAI: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>> = OnceLock::new();
    static ANTHROPIC: OnceLock<Mutex<Option<whimpr_cleanup::AnthropicProvider>>> = OnceLock::new();
    static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
    static SETTINGS: OnceLock<Mutex<whimpr_core::Settings>> = OnceLock::new();
    static DICTIONARY: OnceLock<Mutex<whimpr_core::DictionaryStore>> = OnceLock::new();
    static SNIPPETS: OnceLock<Mutex<whimpr_core::SnippetStore>> = OnceLock::new();
    static STATS: OnceLock<Mutex<whimpr_core::StatsStore>> = OnceLock::new();
    /// (raw pre-cleanup text, final pasted text) from the most recent dictation  -
    /// feeds the "undo last cleanup edit" hotkey (Cmd+Shift+Z). `None` until the
    /// first dictation completes this run.
    static LAST_TEXTS: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();
    /// The user's voice workflows (trigger -> command-edit instruction).
    static WORKFLOWS: OnceLock<Mutex<whimpr_core::WorkflowStore>> = OnceLock::new();
    /// Voice Memory (encrypted at rest) and its keychain-held AES key. The key
    /// slot stays empty when the keychain is unavailable  -  memory then lives
    /// only for this run and saves are skipped.
    static VOICE_MEMORY: OnceLock<Mutex<whimpr_core::VoiceMemory>> = OnceLock::new();
    static VM_KEY: OnceLock<[u8; 32]> = OnceLock::new();
    /// File name of the whisper model actually loaded (for provenance + health).
    /// Swapped together with [`ASR`] on a hot reload  -  always set via
    /// [`set_asr`] so the pair stays consistent.
    static ASR_MODEL_NAME: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    /// True while the current session is a locked (hands-free) one  -  set on
    /// `ShowBar(Locked)`, cleared on idle/record-start. Meeting mode reads it at
    /// finalize to decide note-vs-paste and long-form transcription.
    static SESSION_LOCKED: AtomicBool = AtomicBool::new(false);
    /// Generation counter for the streaming-preview loop: bumped on every
    /// capture start/stop so a stale loop exits at its next tick.
    static PARTIAL_GEN: AtomicU64 = AtomicU64::new(0);
    /// Guards that only one partial transcription runs at a time.
    static PARTIAL_BUSY: AtomicBool = AtomicBool::new(false);
    /// The Context Capsule captured at the most recent record-start (Fn down),
    /// or `None` when the capsule was disabled / not applicable for that app.
    static LAST_CAPSULE: OnceLock<Mutex<Option<Capsule>>> = OnceLock::new();
    /// Generation counter for capsule captures: bumped (and the slot cleared)
    /// synchronously at every Fn down, so a slow AX read from an EARLIER
    /// dictation can never overwrite the current one's capsule  -  the stale
    /// `capture_capsule` thread sees a newer generation and drops its result.
    static CAPSULE_GEN: AtomicU64 = AtomicU64::new(0);
    /// A workflow result held for user approval (`require_approval`), consumed
    /// by `approve_pending` / `reject_pending`. At most one at a time; a new
    /// pending result replaces an unanswered one.
    static PENDING: OnceLock<Mutex<Option<PendingItem>>> = OnceLock::new();

    /// The per-dictation context bundle (Context Capsule, opt-in).
    #[derive(Clone)]
    struct Capsule {
        app: Option<String>,
        selection: Option<String>,
        glossary: Vec<String>,
        style: bool,
    }

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
        /// Bundle id of the app the user dictated into (the TARGET_APP snapshot
        /// at creation time)  -  a Paste approval re-activates it first, since
        /// clicking Approve made the Hub frontmost.
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
    struct WavePayload {
        bars: Vec<f32>,
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

    /// The whisper ASR model to load: prefer the most accurate one present, in
    /// descending quality order, falling back to the small base model. Bigger
    /// English models mis-hear names/technical terms far less (and better ASR means
    /// less for cleanup and the dictionary to fix downstream).
    ///
    /// `language` is the user's selected ASR language (`None`/`Some("en")` = auto or
    /// English). `.en`-suffixed models are English-only  -  they cannot transcribe
    /// other languages at all  -  so when a specific *non-English* language is
    /// selected we only consider multilingual model files (no `.en` suffix).
    /// Otherwise we keep preferring `.en` models first (better English accuracy),
    /// falling back to multilingual files if no `.en` model is present.
    fn model_path(language: Option<&str>) -> PathBuf {
        let dir = support_dir().join("models");
        let needs_multilingual = matches!(language, Some(lang) if lang != "en");
        const MULTILINGUAL: &[&str] = &[
            "ggml-large-v3-turbo.bin",
            "ggml-large-v3-turbo-q8_0.bin",
            "ggml-medium.bin",
            "ggml-medium-q8_0.bin",
            "ggml-small.bin",
            "ggml-small-q8_0.bin",
            "ggml-base.bin",
            "ggml-base-q8_0.bin",
        ];
        // "ggml-distil-large-v3.5.bin": rename after downloading from
        // distil-whisper/distil-large-v3.5-ggml (ships as generic
        // "ggml-model.bin"). ~1.5x faster than large-v3-turbo with better
        // short-form WER (slightly worse long-form); English-only.
        // "ggml-medium-32-2.en.bin" / "ggml-distil-small.en.bin" are
        // distil-whisper's medium.en/small.en distillations - same accuracy
        // class, faster. "-q8_0" files are 8-bit quantized ggml weights:
        // near-lossless WER, roughly half the file size of the full model.
        const ENGLISH_FIRST: &[&str] = &[
            "ggml-distil-large-v3.5.bin",
            "ggml-large-v3-turbo.bin",
            "ggml-large-v3-turbo-q8_0.bin",
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
                "[whimpr] no multilingual whisper model found for language {:?}  -  falling \
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

    fn support_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/WhimprFlow")
    }
    fn settings_path() -> PathBuf {
        support_dir().join("settings.json")
    }
    fn dict_path() -> PathBuf {
        support_dir().join("dictionary.json")
    }
    fn snippets_path() -> PathBuf {
        support_dir().join("snippets.json")
    }
    fn stats_path() -> PathBuf {
        support_dir().join("stats.json")
    }
    fn workflows_path() -> PathBuf {
        support_dir().join("workflows.json")
    }
    /// Where the Studio notes live. Mirrors `crate::notes`' private path helper
    /// (same support dir, same file name)  -  used only for backups here.
    fn notes_path() -> PathBuf {
        support_dir().join("notes.json")
    }
    fn voice_memory_path() -> PathBuf {
        support_dir().join("voice_memory.enc")
    }

    /// Seconds since the Unix epoch (UTC), or 0 if the clock is before the epoch.
    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
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
        let app = TARGET_APP
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
        let retention_days = current_settings().retention_days;
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
    /// unchanged (`None` = auto-detect). Used by both the finalize path and
    /// the streaming-preview loop.
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

    /// The most recent dictation's text, for the "paste/copy last transcript"
    /// hotkeys. `None` if nothing has been dictated yet this run.
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
            // memory for the Cmd+Shift+Z undo pair  -  use its final text so
            // paste-last/copy-last keep working under max privacy.
            .or_else(|| {
                LAST_TEXTS
                    .get()
                    .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
                    .map(|(_raw, final_text)| final_text)
            })
    }

    /// Re-paste the most recently dictated transcript into the frontmost app
    /// (Cmd+Shift+V). Uses the same clipboard-paste path as a normal dictation.
    fn paste_last_transcript() {
        match latest_transcript() {
            Some(text) if !text.is_empty() => {
                eprintln!("[whimpr] hotkey: paste last transcript (Cmd+Shift+V)");
                if let Err(e) = crate::paste::paste_text(&text) {
                    eprintln!("[whimpr] paste-last-transcript failed: {e}");
                }
            }
            _ => eprintln!("[whimpr] paste-last-transcript: no transcript yet"),
        }
    }

    /// Copy the most recently dictated transcript to the clipboard, without
    /// pasting it anywhere (Cmd+Shift+C).
    fn copy_last_transcript() {
        match latest_transcript() {
            Some(text) if !text.is_empty() => {
                eprintln!("[whimpr] hotkey: copy last transcript (Cmd+Shift+C)");
                use arboard::Clipboard;
                if let Err(e) = Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                    eprintln!("[whimpr] copy-last-transcript failed: {e}");
                }
            }
            _ => eprintln!("[whimpr] copy-last-transcript: no transcript yet"),
        }
    }

    /// Re-paste the raw (pre-cleanup) transcript from the most recent dictation,
    /// undoing the LLM cleanup edit (Cmd+Shift+Z). No-ops if cleanup made no change
    /// (nothing to undo) or nothing has been dictated yet this run.
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
                eprintln!("[whimpr] hotkey: undo last cleanup edit (Cmd+Shift+Z)");
                if let Err(e) = crate::paste::paste_text(&raw) {
                    eprintln!("[whimpr] undo-last-cleanup failed: {e}");
                }
            }
            Some(_) => {
                eprintln!("[whimpr] undo-last-cleanup: cleanup made no changes, nothing to undo")
            }
            None => eprintln!("[whimpr] undo-last-cleanup: no transcript yet"),
        }
    }

    /// The dictionary entries for the Hub Dictionary screen (auto-learned flagged).
    pub fn dictionary_entries() -> Vec<DictEntryDto> {
        DICTIONARY
            .get()
            .map(|m| {
                m.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .entries
                    .iter()
                    .map(|e| DictEntryDto {
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

    /// Add an AUTO-learned entry (from the post-paste correction observer) and persist.
    /// Marked ✨ auto in the UI. No-op if it would duplicate an existing entry's data.
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

    /// Aggregated stats for the Hub. `tz_offset_minutes` is the UI's
    /// `Date.getTimezoneOffset()` so day math matches the user's local clock.
    pub fn stats_summary(tz_offset_minutes: i32) -> whimpr_core::StatsSummary {
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

    /// Read an API key from an env var or the OS keychain (never a plaintext file).
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

    /// A snapshot of the current settings.
    pub fn current_settings() -> whimpr_core::Settings {
        SETTINGS
            .get()
            .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .unwrap_or_default()
    }
    /// Apply new settings and rebuild the cloud providers (picks up model
    /// changes). Also applies retention immediately (prunes stored text older
    /// than the new window) and hot-reloads the whisper engine when a language
    /// change needs a different model file.
    pub fn update_settings(new: whimpr_core::Settings) {
        let language_changed = current_settings().language != new.language;
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
        let target = model_path(language);
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
            "[whimpr] ASR model change: {}  ->  {target_name} (loading in the background)",
            loaded.as_deref().unwrap_or("<none>")
        );
        std::thread::spawn(move || match whimpr_asr::WhisperEngine::load(&target) {
            Ok(engine) => {
                set_asr(Arc::new(engine), target_name.clone());
                eprintln!("[whimpr] ASR model swapped in: {target_name}");
            }
            Err(e) => {
                eprintln!("[whimpr] ASR hot-reload failed ({e})  -  keeping the current model")
            }
        });
    }

    /// (Re)build the cloud cleanup providers from the current keys + settings. Called
    /// at startup and whenever a key or model changes, so edits take effect live.
    pub fn rebuild_providers() {
        let settings = current_settings();
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
            "[whimpr] cleanup providers: openai={}, anthropic={}",
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
        let settings = current_settings();
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
        let app_bundle_id = TARGET_APP
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
        if let Some(app) = app_bundle_id.as_deref() {
            eprintln!("[whimpr] cleanup target app: {app}");
        }
        // Code Mode: the code-dictation prompt variant, when the paste target is
        // an IDE/terminal and the user hasn't opted out.
        let code_mode = settings.code_mode_auto
            && app_bundle_id
                .as_deref()
                .map(whimpr_core::cleanup::prompts::is_code_app)
                .unwrap_or(false);
        if code_mode {
            eprintln!("[whimpr] code mode active for this cleanup");
        }
        // Context Capsule (opt-in): the AX selection captured at record start
        // becomes reference-only window context for the model.
        let window_context = LAST_CAPSULE
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .and_then(|c| c.selection);
        let ctx = CleanupContext {
            level,
            vocab,
            app_bundle_id,
            window_context,
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
                    eprintln!("[whimpr] cleanup gate rejected the edit  -  pasting raw");
                    provenance.gate = "rejected".to_string();
                    raw_out.clone()
                }
            }
            Some(Err(e)) => {
                // Provider errored: the final text is raw ("raw"/"skipped" stand),
                // but sent_to_cloud stays honest  -  the transcript may have left
                // the machine even though no edit came back.
                eprintln!("[whimpr] cleanup failed ({e})  -  pasting raw");
                raw_out.clone()
            }
            None => {
                if matches!(settings.cleanup_mode, CleanupMode::Local) {
                    eprintln!("[whimpr] local cleanup model not wired yet  -  pasting raw");
                } else {
                    eprintln!("[whimpr] cleanup provider has no API key  -  pasting raw");
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

    fn now_ms() -> u64 {
        CLOCK
            .get()
            .map(|c| c.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    fn bar_name(b: BarState) -> &'static str {
        match b {
            BarState::Idle => "idle",
            BarState::Recording => "recording",
            BarState::Locked => "locked",
            BarState::Transcribing => "transcribing",
            BarState::Done => "done",
            BarState::Cancelled => "cancelled",
            BarState::Error => "error",
        }
    }

    fn emit_bar(app: &AppHandle, state: &'static str) {
        eprintln!("[whimpr] pill -> {state}");
        let _ = app.emit_to(
            OVERLAY_LABEL,
            "whimpr://flowbar/state",
            BarPayload { state },
        );
    }

    /// Bundle id of the app that was frontmost at record start, if any.
    fn target_app() -> Option<String> {
        TARGET_APP
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    /// Emit the insertion receipt to both the overlay pill and the Hub.
    fn emit_receipt(app: &AppHandle, payload: ReceiptPayload) {
        eprintln!(
            "[whimpr] receipt: ok={} action={} words={}",
            payload.ok, payload.action, payload.words
        );
        let _ = app.emit_to(OVERLAY_LABEL, "whimpr://receipt", payload.clone());
        let _ = app.emit_to(HUB_LABEL, "whimpr://receipt", payload);
    }

    /// Announce a workflow result awaiting approval, to both windows.
    fn emit_pending(app: &AppHandle, name: &str, preview: &str) {
        let payload = PendingPayload {
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
        let settings = current_settings();
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
            eprintln!("[whimpr] WORKFLOW \"{}\" matched", entry.name);
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
                        "[whimpr] workflow \"{}\" failed ({e})  -  falling back to a normal \
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
                eprintln!("[whimpr] SNIPPET matched  -  pasting expansion directly");
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
                    eprintln!("[whimpr] CLEANED:   \"{}\"", outcome.final_text);
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
        let paste_result = crate::paste::paste_text(&text);
        if let Err(e) = &paste_result {
            eprintln!("[whimpr] paste failed: {e}");
        }
        // Stash (raw, final) for the "undo last cleanup edit" hotkey
        // (Cmd+Shift+Z), right after the paste.
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
                // Watch the field for a post-paste correction to learn (✨).
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
    /// receipt (e.g. "delivered to the clipboard because the target app is
    /// gone"); a destination failure's own error message takes precedence.
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
            WorkflowDestination::Paste => match crate::paste::paste_text(&text) {
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

    /// While recording (and streaming preview is on), transcribe a snapshot of
    /// the capture every ~1.2 s and emit provisional text to the pill. Partials
    /// are never pasted  -  commit stays the verified finalize path.
    ///
    /// ponytail: each tick re-runs full whisper inference over the whole buffer
    /// so far; with a large model (or a long hands-free session) one pass can
    /// exceed the tick interval  -  the busy flag then skips ticks, so the
    /// preview just updates more slowly. Upgrade path: incremental decoding
    /// over only the new audio, or a dedicated small preview model.
    fn spawn_partial_loop(app: AppHandle) {
        let settings = current_settings();
        if !settings.streaming_preview {
            return;
        }
        let Some(asr) = current_asr() else {
            return; // no preview until the model loads; finalize still works
        };
        let language = settings.language;
        let my_gen = PARTIAL_GEN.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::spawn(move || loop {
            std::thread::sleep(PARTIAL_INTERVAL);
            if PARTIAL_GEN.load(Ordering::SeqCst) != my_gen {
                return; // capture stopped or a newer session started
            }
            // Only one partial transcription at a time  -  skip the tick if
            // the previous one is still running. Claimed BEFORE the snapshot
            // so a busy tick never touches the shared sample buffer at all.
            if PARTIAL_BUSY.swap(true, Ordering::SeqCst) {
                continue;
            }
            // Snapshot under the slot lock so a concurrent stop can't race us.
            let mut capture_present = false;
            let snap = CAPTURE.get().and_then(|slot| {
                let guard = slot.lock().unwrap_or_else(|e| e.into_inner());
                guard.as_ref().and_then(|h| {
                    capture_present = true;
                    h.snapshot()
                })
            });
            if !capture_present {
                PARTIAL_BUSY.store(false, Ordering::SeqCst);
                return;
            }
            let Some(res) = snap else {
                PARTIAL_BUSY.store(false, Ordering::SeqCst);
                continue;
            };
            // Under 1 s of audio: too short for whisper to say anything useful.
            if res.duration_secs() < 1.0 {
                PARTIAL_BUSY.store(false, Ordering::SeqCst);
                continue;
            }
            let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
            let lang = effective_language(language.as_deref());
            let out = asr.transcribe_opts(&pcm, lang.as_deref(), false);
            PARTIAL_BUSY.store(false, Ordering::SeqCst);
            if PARTIAL_GEN.load(Ordering::SeqCst) != my_gen {
                return; // finalized while transcribing  -  drop the stale partial
            }
            if let Ok(t) = out {
                if !t.text.is_empty() {
                    let _ = app.emit_to(
                        OVERLAY_LABEL,
                        "whimpr://transcript/partial",
                        TranscriptPayload { text: t.text },
                    );
                }
            }
        });
    }

    /// Capture the Context Capsule at record start (Fn down): frontmost app,
    /// AX-selected text (opt-in), the dictionary glossary relevant to that
    /// selection, and whether a style profile applies. Stores `None` when the
    /// capsule is off or the app isn't in the allow-list, so cleanup gets no
    /// window context and the Privacy pane shows nothing was captured.
    ///
    /// `gen` is the CAPSULE_GEN value sampled at the Fn down that spawned this
    /// capture: the result is only stored while it is still current, so a
    /// capture stalled on a slow AX read can never overwrite a NEWER
    /// dictation's capsule with the previous app's selection.
    fn capture_capsule(target: Option<String>, gen: u64) {
        let slot = LAST_CAPSULE.get_or_init(|| Mutex::new(None));
        // Re-checked under the lock so a bump can't slip between check and store.
        let store = |value: Option<Capsule>| {
            let mut guard = slot.lock().unwrap_or_else(|e| e.into_inner());
            if CAPSULE_GEN.load(Ordering::SeqCst) == gen {
                *guard = value;
            }
        };
        let settings = current_settings();
        let allowed = settings.capsule.enabled
            && (settings.capsule.apps.is_empty()
                || target
                    .as_deref()
                    .map(|a| settings.capsule.apps.iter().any(|x| x == a))
                    .unwrap_or(false));
        if !allowed {
            store(None);
            return;
        }
        let selection = if settings.capsule.include_selection {
            crate::appctx::ax_selected_text()
        } else {
            None
        };
        let glossary: Vec<String> = selection
            .as_deref()
            .map(|sel| {
                DICTIONARY
                    .get()
                    .map(|d| {
                        d.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .prefilter(sel, 15)
                            .into_iter()
                            .map(|v| v.correct)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let style = settings.style.to_instructions().is_some();
        store(Some(Capsule {
            app: target,
            selection,
            glossary,
            style,
        }));
    }

    /// Feed one input into the shared state machine and enact its actions.
    fn handle_input(input: Input) {
        let (Some(app), Some(machine)) = (APP.get(), MACHINE.get()) else {
            return;
        };
        let actions = {
            let mut m = machine.lock().unwrap_or_else(|e| e.into_inner());
            m.step(input)
        };
        for action in actions {
            apply_action(app, action);
        }
    }

    /// Invoked by the pill's Stop button (Tauri `confirm_dictation` command) to
    /// end a locked hands-free session  -  synthesizes the same re-press-to-finalize
    /// input a second `PushToTalk` chord would produce while `Locked`. A no-op in
    /// every other state (Idle, mid-hold, AwaitingLock, Finalizing), matching the
    /// state machine's own handling of a stray `Down` there.
    pub fn confirm_dictation() {
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::PushToTalk,
            at_ms: now_ms(),
        }));
    }

    /// Invoked by the pill's Cancel button (Tauri `cancel_dictation` command)  -
    /// synthesizes the same `Cancel` trigger the Escape key produces. A no-op
    /// from Idle.
    pub fn cancel_dictation() {
        handle_input(Input::Trigger(TriggerToken::Cancel { at_ms: now_ms() }));
    }

    fn apply_action(app: &AppHandle, action: Action) {
        match action {
            Action::ShowBar(bar) => {
                // Track whether the current session is a locked (hands-free) one  -
                // meeting mode reads this at finalize.
                match bar {
                    BarState::Locked => SESSION_LOCKED.store(true, Ordering::SeqCst),
                    BarState::Idle => SESSION_LOCKED.store(false, Ordering::SeqCst),
                    _ => {}
                }
                emit_bar(app, bar_name(bar));
                // Let the "done" tick linger briefly before returning to idle.
                if bar == BarState::Done {
                    let app2 = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(500));
                        emit_bar(&app2, "idle");
                    });
                }
            }
            // Start the microphone; stream real RMS bars to the pill waveform.
            // Runs off the tap thread so the mic-permission prompt can't stall keys.
            Action::StartCapture { .. } => {
                SESSION_LOCKED.store(false, Ordering::SeqCst);
                // Sample the partial generation NOW, on the tap thread: both
                // stop paths (finalize/discard) bump it before take()ing the
                // slot, so a quick tap or cancel that outraces the (possibly
                // slow  -  first-run permission prompt) mic start below is
                // detectable in the Ok arm. Without this, the handle would be
                // stored after the stop already emptied the slot and the mic
                // would stay hot forever.
                let start_gen = PARTIAL_GEN.load(Ordering::SeqCst);
                let app_thread = app.clone();
                std::thread::spawn(move || {
                    let app_cb = app_thread.clone();
                    match whimpr_audio::start(move |bars| {
                        let _ = app_cb.emit_to(
                            OVERLAY_LABEL,
                            "whimpr://audio/waveform",
                            WavePayload {
                                bars: bars.to_vec(),
                            },
                        );
                    }) {
                        Ok(handle) => {
                            let slot = CAPTURE.get_or_init(|| Mutex::new(None));
                            let mut guard = slot.lock().unwrap_or_else(|e| e.into_inner());
                            if PARTIAL_GEN.load(Ordering::SeqCst) != start_gen {
                                // The session already ended (quick tap, cancel,
                                // or finalize) while the mic was still starting
                                // -  its stop path found an empty slot, so
                                // nobody else will ever stop this handle. Stop
                                // it here and skip the preview loop.
                                drop(guard);
                                eprintln!(
                                    "[whimpr] capture outlived its session  -  stopping the mic"
                                );
                                let _ = handle.stop();
                                return;
                            }
                            *guard = Some(handle);
                            drop(guard);
                            // Live provisional text while recording (opt-out).
                            spawn_partial_loop(app_thread);
                        }
                        Err(e) => eprintln!("[whimpr] mic capture failed to start: {e}"),
                    }
                });
            }
            // Stop the mic, transcribe the buffered audio, and advance the machine.
            Action::StopCaptureAndFinalize { session } => {
                let app2 = app.clone();
                let was_locked = SESSION_LOCKED.load(Ordering::SeqCst);
                // Stop the streaming-preview loop for this capture.
                PARTIAL_GEN.fetch_add(1, Ordering::SeqCst);
                let handle = CAPTURE
                    .get()
                    .and_then(|slot| slot.lock().unwrap_or_else(|e| e.into_inner()).take());
                std::thread::spawn(move || {
                    // Whatever happens, return the pill to idle (done -> idle).
                    let finish =
                        || handle_input(Input::Pipeline(PipelineEvent::Committed { session }));
                    let Some(res) = handle.and_then(|h| h.stop()) else {
                        eprintln!("[whimpr] no audio captured");
                        emit_receipt(
                            &app2,
                            ReceiptPayload {
                                ok: false,
                                action: "error",
                                app: target_app(),
                                words: 0,
                                confidence: None,
                                low_words: Vec::new(),
                                message: Some(
                                    "no audio was captured - check microphone access".to_string(),
                                ),
                            },
                        );
                        finish();
                        return;
                    };
                    let peak = res.samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
                    eprintln!(
                        "[whimpr] captured {} samples @ {} Hz (~{:.2}s), peak {:.4}",
                        res.samples.len(),
                        res.sample_rate,
                        res.duration_secs(),
                        peak
                    );
                    if peak < 0.005 {
                        eprintln!(
                            "[whimpr] ⚠ audio is silent  -  the mic isn't being captured. Grant \
                             Microphone access to your terminal (System Settings → Privacy & \
                             Security → Microphone), then fully quit + reopen it and rerun."
                        );
                    }
                    let Some(asr) = current_asr() else {
                        eprintln!("[whimpr] ASR not ready (model still loading or missing)");
                        emit_receipt(
                            &app2,
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
                        finish();
                        return;
                    };
                    let settings = current_settings();
                    // Long-form transcription only for a hands-free meeting
                    // session  -  push-to-talk clips stay single-segment.
                    let long_form = was_locked && settings.meeting_mode;
                    let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
                    let lang = effective_language(settings.language.as_deref());
                    match asr.transcribe_opts(&pcm, lang.as_deref(), long_form) {
                        Ok(t) => {
                            eprintln!("[whimpr] TRANSCRIPT: \"{}\"", t.text);
                            finalize_transcript(
                                &app2,
                                t.text,
                                t.confidence,
                                t.low_words,
                                res.duration_secs(),
                                was_locked,
                            );
                        }
                        Err(e) => {
                            eprintln!("[whimpr] ASR error: {e}");
                            emit_receipt(
                                &app2,
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
                    finish();
                });
            }
            Action::DiscardCapture { .. } => {
                PARTIAL_GEN.fetch_add(1, Ordering::SeqCst);
                if let Some(slot) = CAPTURE.get() {
                    if let Some(handle) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = handle.stop();
                    }
                }
            }
            // The ASR path (StopCaptureAndFinalize) now drives pipeline completion.
            Action::RunPipeline { .. } => {}
            // PlayPing / WarnSessionCap: no-ops for now.
            _ => {}
        }
    }

    /// macOS virtual keycode for a bindable [`whimpr_core::Key`]. -1 for anything
    /// outside the bindable set (letters/digits/Escape).
    fn keycode_for_key(key: whimpr_core::Key) -> i64 {
        use whimpr_core::Key;
        match key {
            Key::Escape => 53,
            Key::Char(c) => match c.to_ascii_uppercase() {
                'A' => 0,
                'B' => 11,
                'C' => 8,
                'D' => 2,
                'E' => 14,
                'F' => 3,
                'G' => 5,
                'H' => 4,
                'I' => 34,
                'J' => 38,
                'K' => 40,
                'L' => 37,
                'M' => 46,
                'N' => 45,
                'O' => 31,
                'P' => 35,
                'Q' => 12,
                'R' => 15,
                'S' => 1,
                'T' => 17,
                'U' => 32,
                'V' => 9,
                'W' => 13,
                'X' => 7,
                'Y' => 16,
                'Z' => 6,
                '0' => 29,
                '1' => 18,
                '2' => 19,
                '3' => 20,
                '4' => 21,
                '5' => 23,
                '6' => 22,
                '7' => 26,
                '8' => 28,
                '9' => 25,
                _ => -1,
            },
        }
    }

    /// Whether the CGEventFlags modifier bits match a [`whimpr_core::Chord`] exactly
    /// (all four, not "at least these").
    fn mods_match_chord(flags: u64, chord: &whimpr_core::Chord) -> bool {
        let meta = flags & KCG_FLAG_MASK_COMMAND != 0;
        let shift = flags & KCG_FLAG_MASK_SHIFT != 0;
        let ctrl = flags & KCG_FLAG_MASK_CONTROL != 0;
        let alt = flags & KCG_FLAG_MASK_ALTERNATE != 0;
        meta == chord.meta && shift == chord.shift && ctrl == chord.ctrl && alt == chord.alt
    }

    extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        etype: u32,
        event: CGEventRef,
        _info: *mut c_void,
    ) -> CGEventRef {
        if etype == K_CG_TAP_DISABLED_BY_TIMEOUT || etype == K_CG_TAP_DISABLED_BY_USER_INPUT {
            let port = TAP_PORT.load(Ordering::SeqCst);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port, true) };
            }
            return event;
        }
        if etype == K_CG_EVENT_FLAGS_CHANGED {
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
            if keycode == KEYCODE_FN {
                let flags = unsafe { CGEventGetFlags(event) };
                let down = (flags & FLAG_SECONDARY_FN) != 0;
                let was_down = FN_IS_DOWN.swap(down, Ordering::SeqCst);
                let at_ms = now_ms();
                if down && !was_down {
                    eprintln!("[whimpr] Fn DOWN");
                    // Snapshot the paste target now, while the user's app is focused.
                    let target = crate::appctx::frontmost_bundle_id();
                    *TARGET_APP
                        .get_or_init(|| Mutex::new(None))
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = target.clone();
                    // Context Capsule (opt-in): captured off the tap thread  -
                    // the AX selection read round-trips to the target app's
                    // process and must not stall key events. Bump the capsule
                    // generation and clear the slot NOW (both cheap) so a
                    // stale capture from an earlier Fn down can't serve the
                    // previous app's selection as this dictation's context.
                    let capsule_gen = CAPSULE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Some(m) = LAST_CAPSULE.get() {
                        *m.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    }
                    std::thread::spawn(move || capture_capsule(target, capsule_gen));
                    handle_input(Input::Trigger(TriggerToken::Down {
                        binding: BindingId::PushToTalk,
                        at_ms,
                    }));
                } else if !down && was_down {
                    eprintln!("[whimpr] Fn UP");
                    handle_input(Input::Trigger(TriggerToken::Up {
                        binding: BindingId::PushToTalk,
                        at_ms,
                    }));
                }
            }
        } else if etype == K_CG_EVENT_KEY_DOWN {
            // Ignore OS-synthesized auto-repeat keydowns  -  fire once per physical
            // press, not once per repeat tick while the chord is held.
            let autorepeat =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_AUTOREPEAT) };
            if autorepeat != 0 {
                return event;
            }
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
            let flags = unsafe { CGEventGetFlags(event) };
            // User-configurable bindings, read fresh so a rebind from the Shortcuts
            // UI takes effect immediately. Each must match its chord exactly.
            let bindings = SETTINGS
                .get()
                .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).keybindings)
                .unwrap_or_default();
            if keycode == keycode_for_key(bindings.cancel.key)
                && mods_match_chord(flags, &bindings.cancel)
            {
                handle_input(Input::Trigger(TriggerToken::Cancel { at_ms: now_ms() }));
            }
            if keycode == keycode_for_key(bindings.paste_last.key)
                && mods_match_chord(flags, &bindings.paste_last)
            {
                paste_last_transcript();
            }
            if keycode == keycode_for_key(bindings.copy_last.key)
                && mods_match_chord(flags, &bindings.copy_last)
            {
                copy_last_transcript();
            }
            if keycode == keycode_for_key(bindings.undo_last.key)
                && mods_match_chord(flags, &bindings.undo_last)
            {
                undo_last_cleanup();
            }
        }
        event
    }

    /// Run a Command Mode / Transform edit through whichever cleanup provider is
    /// configured (cloud). Falls back with an error when only a local provider is
    /// available, since the local worker's command-edit path isn't wired in this build.
    fn run_command_edit(selection: &str, instruction: &str) -> anyhow::Result<String> {
        let settings = current_settings();
        let run_local = |_selection: &str, _instruction: &str| -> anyhow::Result<String> {
            anyhow::bail!(
                "local Command Mode is unavailable in this build. Set Cleanup Engine to OpenAI                  or Anthropic in Settings to use Transforms or Command Mode"
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

    /// Manual hook for Command Mode / Transforms, reachable from the Hub without the
    /// Fn+Ctrl hotkey  -  exercises the prompt + provider layer directly.
    pub fn test_command_edit(selection: String, instruction: String) -> Result<String, String> {
        run_command_edit(&selection, &instruction).map_err(|e| e.to_string())
    }

    /// Copy every user data store into a timestamped backup folder. Note
    /// voice_memory.enc is only decryptable on the same machine  -  its AES
    /// key lives in the user's keychain, not in the backup.
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
    pub fn get_health() -> Health {
        Health {
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

    /// What the last Context Capsule contained, for the Privacy pane. `None`
    /// until a capsule has been captured this run.
    pub fn get_last_capsule() -> Option<CapsuleReport> {
        let capsule = LAST_CAPSULE
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone())?;
        Some(CapsuleReport {
            app: capsule.app,
            selection_preview: capsule
                .selection
                .as_deref()
                .map(|s| truncate_chars(s, PREVIEW_CHARS)),
            glossary: capsule.glossary,
            style: capsule.style,
            enabled: current_settings().capsule.enabled,
        })
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
    pub fn get_pending() -> Option<PendingPayload> {
        let slot = PENDING.get()?;
        let guard = slot.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|item| PendingPayload {
            name: item.name.clone(),
            preview: truncate_chars(&item.text, PREVIEW_CHARS),
        })
    }

    /// Approve the held workflow result: execute its destination now.
    ///
    /// Clicking Approve made the Hub frontmost, so a Paste destination would
    /// land in the Hub itself  -  re-activate the app the user dictated into
    /// first, and when that fails (app quit, nothing captured) deliver to the
    /// clipboard instead and say so in the receipt.
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
            let refocused = item
                .target_app
                .as_deref()
                .map(crate::appctx::activate_app)
                .unwrap_or(false);
            if !refocused {
                destination = WorkflowDestination::Clipboard;
                note = Some(
                    "the target app is not available - copied to the clipboard instead".to_string(),
                );
            }
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
    /// hex-encoded in the OS keychain so the encrypted file on disk is useless
    /// without the user's keychain.
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

    /// Persist the memory encrypted; a no-op when the keychain key is unavailable
    /// (memory then lives only for this run).
    fn save_voice_memory() {
        let (Some(m), Some(key)) = (VOICE_MEMORY.get(), VM_KEY.get()) else {
            return;
        };
        let vm = m.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = vm.save_encrypted(&voice_memory_path(), key) {
            eprintln!("[whimpr] voice memory save failed: {e}");
        }
    }

    /// Append one learned correction to Voice Memory and persist (encrypted).
    pub fn voice_memory_record(from: String, to: String, source: &str) {
        let Some(m) = VOICE_MEMORY.get() else {
            return;
        };
        m.lock().unwrap_or_else(|e| e.into_inner()).record(
            from,
            to,
            source.to_string(),
            unix_now(),
        );
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
        let style = current_settings().style;
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

    /// Screenshot the whole screen into the app's captures folder and return
    /// the path (macOS `screencapture -x`: silent, no camera sound).
    pub fn capture_screen() -> Result<String, String> {
        let dir = support_dir().join("captures");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("cap-{}.png", unix_now()));
        let status = std::process::Command::new("/usr/bin/screencapture")
            .arg("-x")
            .arg(&path)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("screencapture exited with {status}"));
        }
        Ok(path.display().to_string())
    }

    pub fn install(app: AppHandle) {
        let _ = APP.set(app);
        let _ = MACHINE.set(Mutex::new(StateMachine::new()));
        let _ = CLOCK.set(Instant::now());

        // Load settings + dictionary, and build cloud providers from stored keys.
        // Loaded synchronously (cheap file read) before the ASR thread below so
        // `model_path()` can pick an English-only vs. multilingual model file
        // based on the configured language.
        let settings = whimpr_core::Settings::load(&settings_path());
        let dict = whimpr_core::DictionaryStore::load(&dict_path());
        eprintln!(
            "[whimpr] cleanup mode: {:?}, level: {:?}, language: {:?}",
            settings.cleanup_mode, settings.cleanup_level, settings.language
        );
        let language_for_model = settings.language.clone();
        let retention_days = settings.retention_days;
        let _ = SETTINGS.set(Mutex::new(settings));
        let _ = DICTIONARY.set(Mutex::new(dict));
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

        // Voice Memory: key from the keychain (created on first run), then the
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
                    "[whimpr] voice memory key unavailable (keychain?)  -  memory is \
                     in-memory only this run"
                );
                let _ = VOICE_MEMORY.set(Mutex::new(whimpr_core::VoiceMemory::default()));
            }
        }

        // Load the speech-to-text model off the main thread (it takes ~1s).
        std::thread::spawn(move || {
            let path = model_path(language_for_model.as_deref());
            if !path.exists() {
                eprintln!("[whimpr] ASR model not found at {}", path.display());
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
                    eprintln!("[whimpr] ASR model loaded  -  ready to transcribe");
                }
                Err(e) => eprintln!("[whimpr] ASR model load failed: {e}"),
            }
        });

        // Start the local cleanup worker in the background (model load takes a few
        // seconds; the first local cleanup waits for it, subsequent ones are fast).
        std::thread::spawn(|| {
            let worker = crate::local_llm::spawn_default();
            let _ = LOCAL.set(Mutex::new(worker));
        });

        // Accessibility is the ONE permission that makes the Fn CGEventTap global AND
        // lets us post the Cmd+V paste into other apps. Without it, a keyboard tap is
        // silently limited to frontmost-only  -  the exact bug. Prompt for it up front.
        if crate::paste::is_trusted() {
            eprintln!("[whimpr] Accessibility granted  -  Fn works in every app, paste enabled");
        } else {
            eprintln!(
                "[whimpr] ⚠ Accessibility NOT granted  -  Fn only works while WhimprFlow is \
                 frontmost and paste is disabled. Prompting; grant WhimprFlow under System \
                 Settings → Privacy & Security → Accessibility (no relaunch needed)."
            );
            crate::paste::prompt_accessibility();
        }
        // Input Monitoring is NOT the gate for a CGEventTap  -  kept only as diagnostics.
        eprintln!(
            "[whimpr] (info) Input Monitoring: {}",
            crate::paste::input_monitoring_granted()
        );

        // Periodic tick drives the double-tap timeout / session cap.
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_millis(100));
            handle_input(Input::Tick { now_ms: now_ms() });
        });

        // The event tap runs on a thread with its own CFRunLoop. CRITICAL: create it
        // ONLY after the process is trusted for Accessibility. macOS fixes a keyboard
        // tap's privilege at CGEventTapCreate time  -  a tap born untrusted is
        // permanently frontmost-only and is NOT upgraded when the grant later arrives.
        // Polling here also means the Fn key starts working the moment the user grants
        // Accessibility, without a relaunch.
        std::thread::spawn(|| {
            while !crate::paste::is_trusted() {
                std::thread::sleep(Duration::from_millis(500));
            }
            eprintln!("[whimpr] Accessibility present  -  creating global Fn tap");
            let port = unsafe {
                CGEventTapCreate(
                    K_CG_SESSION_EVENT_TAP,
                    K_CG_HEAD_INSERT,
                    K_CG_TAP_OPTION_LISTEN_ONLY,
                    EVENTS_OF_INTEREST,
                    tap_callback,
                    null_mut(),
                )
            };
            if port.is_null() {
                eprintln!(
                    "[whimpr] Fn tap null despite Accessibility  -  likely a stale TCC entry from \
                     an earlier build. Run: tccutil reset Accessibility com.whimpr.whimprflow, \
                     then re-grant and relaunch."
                );
                return;
            }
            TAP_PORT.store(port, Ordering::SeqCst);
            unsafe {
                let source = CFMachPortCreateRunLoopSource(null(), port, 0);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);
                CGEventTapEnable(port, true);
                CFRunLoopRun();
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub use imp::{
    approve_pending, backup_data, cancel_dictation, capture_screen, clear_history_text,
    clear_voice_memory, confirm_dictation, current_settings, dictionary_add, dictionary_entries,
    dictionary_learn, dictionary_remove, export_voice_memory, get_health, get_last_capsule,
    get_voice_memory, history, install, rebuild_providers, reject_pending, snippet_add,
    snippet_entries, snippet_remove, stats_summary, test_command_edit, update_settings,
    voice_memory_record, workflow_add, workflow_entries, workflow_remove,
};
// Not yet consumed: their lib.rs command wrappers land with the Workflows /
// Privacy pane wiring. Kept in a separate `use` so the allow is scoped to them.
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use imp::{get_pending, ledger};

// Windows uses the real (but unverified) platform layer in `crate::win`,
// including the roadmap-15 additions (workflows, receipts, voice memory,
// health); Context Capsule and streaming preview remain macOS-only there.
#[cfg(target_os = "windows")]
pub use crate::win::{
    approve_pending, backup_data, cancel_dictation, capture_screen, clear_history_text,
    clear_voice_memory, confirm_dictation, current_settings, dictionary_add, dictionary_entries,
    dictionary_learn, dictionary_remove, export_voice_memory, get_health, get_last_capsule,
    get_pending, get_voice_memory, history, install, ledger, rebuild_providers, reject_pending,
    snippet_add, snippet_entries, snippet_remove, stats_summary, update_settings,
    voice_memory_record, workflow_add, workflow_entries, workflow_remove,
};

// Linux uses the real (but unverified) platform layer in `crate::linux`  -  X11 only
// for this pass; see that module's doc comment for the Wayland follow-up and the
// XGrabKey / xdotool simplifications made.
#[cfg(target_os = "linux")]
pub use crate::linux::{
    approve_pending, backup_data, cancel_dictation, capture_screen, clear_history_text,
    clear_voice_memory, confirm_dictation, current_settings, dictionary_add, dictionary_entries,
    dictionary_learn, dictionary_remove, export_voice_memory, get_health, get_last_capsule,
    get_pending, get_voice_memory, history, install, ledger, rebuild_providers, reject_pending,
    snippet_add, snippet_entries, snippet_remove, stats_summary, update_settings,
    voice_memory_record, workflow_add, workflow_entries, workflow_remove,
};

// Other platforms (BSD, etc.): inert stubs so the crate still builds.
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod other {
    pub fn install(_app: tauri::AppHandle) {}
    pub fn current_settings() -> whimpr_core::Settings {
        whimpr_core::Settings::default()
    }
    pub fn update_settings(_new: whimpr_core::Settings) {}
    pub fn rebuild_providers() {}
    pub fn confirm_dictation() {}
    pub fn cancel_dictation() {}
    pub fn stats_summary(tz_offset_minutes: i32) -> whimpr_core::StatsSummary {
        whimpr_core::StatsStore::default().summary(tz_offset_minutes, 0)
    }
    pub fn history(_limit: usize) -> Vec<whimpr_core::HistoryItem> {
        Vec::new()
    }
    pub fn ledger(_limit: usize) -> Vec<whimpr_core::HistoryItem> {
        Vec::new()
    }
    pub fn dictionary_entries() -> Vec<super::DictEntryDto> {
        Vec::new()
    }
    pub fn dictionary_add(_correct: String, _mishears: Vec<String>) {}
    pub fn dictionary_remove(_correct: &str) {}
    pub fn dictionary_learn(_correct: String, _mishears: Vec<String>) {}
    pub fn snippet_entries() -> Vec<whimpr_core::SnippetEntry> {
        Vec::new()
    }
    pub fn snippet_add(_trigger: String, _expansion: String) {}
    pub fn snippet_remove(_trigger: &str) {}
    pub fn backup_data() -> Result<String, String> {
        Err("backups are not implemented on this platform".to_string())
    }
    pub fn get_health() -> super::Health {
        super::Health {
            asr_ready: false,
            asr_model: None,
            local_llm_ready: false,
            microphone: false,
            accessibility: false,
        }
    }
    pub fn clear_history_text() -> usize {
        0
    }
    pub fn get_last_capsule() -> Option<super::CapsuleReport> {
        None
    }
    pub fn workflow_entries() -> Vec<whimpr_core::WorkflowEntry> {
        Vec::new()
    }
    pub fn workflow_add(
        _name: String,
        _trigger: String,
        _instruction: String,
        _destination: whimpr_core::WorkflowDestination,
        _require_approval: bool,
    ) {
    }
    pub fn workflow_remove(_name: &str) {}
    pub fn get_pending() -> Option<super::PendingPayload> {
        None
    }
    pub fn approve_pending() {}
    pub fn reject_pending() {}
    pub fn voice_memory_record(_from: String, _to: String, _source: &str) {}
    pub fn get_voice_memory() -> Vec<whimpr_core::CorrectionEvent> {
        Vec::new()
    }
    pub fn export_voice_memory() -> Result<String, String> {
        Err("voice memory is not implemented on this platform".to_string())
    }
    pub fn clear_voice_memory() {}
    pub fn capture_screen() -> Result<String, String> {
        Err("screen capture is only implemented on macOS".to_string())
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub use other::{
    approve_pending, backup_data, cancel_dictation, capture_screen, clear_history_text,
    clear_voice_memory, confirm_dictation, current_settings, dictionary_add, dictionary_entries,
    dictionary_learn, dictionary_remove, export_voice_memory, get_health, get_last_capsule,
    get_pending, get_voice_memory, history, install, ledger, rebuild_providers, reject_pending,
    snippet_add, snippet_entries, snippet_remove, stats_summary, update_settings,
    voice_memory_record, workflow_add, workflow_entries, workflow_remove,
};
