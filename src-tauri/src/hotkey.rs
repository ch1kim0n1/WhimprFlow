//! Hold-Fn → pill wiring for the demo shell.
//!
//! This installs an in-process CoreGraphics event tap that feeds Fn key-down /
//! key-up into the real [`whimpr_core`] dictation state machine, and turns the
//! machine's actions into `whimpr://flowbar/state` events the overlay pill
//! renders. There is no audio or ASR yet, so a finalized session is simulated as
//! completing shortly after key release — enough to see the full
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

#[cfg(target_os = "macos")]
mod imp {
    use std::os::raw::c_void;
    use std::path::PathBuf;
    use super::DictEntryDto;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter};
    use whimpr_core::state::{Action, BarState};
    use whimpr_core::{
        AsrEngine, CleanupContext, CleanupMode, CleanupProvider, Input, PipelineEvent, StateMachine,
        TriggerToken,
    };
    use whimpr_ipc::BindingId;

    const OVERLAY_LABEL: &str = "whimpr_bar";

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
    // kCGEventKeyDown — needed (in addition to FlagsChanged) so we can detect the
    // Cmd+Shift+V / Cmd+Shift+C "paste/copy last transcript", Cmd+Shift+Z "undo
    // last cleanup edit", and plain Escape "cancel dictation" keys below; the Fn
    // push-to-talk key alone only ever needs FlagsChanged since it has no keycode.
    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const EVENTS_OF_INTEREST: u64 =
        (1 << K_CG_EVENT_FLAGS_CHANGED) | (1 << K_CG_EVENT_KEY_DOWN);
    const FLAG_SECONDARY_FN: u64 = 0x0080_0000;
    // CGEventFlags modifier bits (CGEventTypes.h). COMMAND is also used by
    // `paste::post_cmd_v`; the other three are new here for the hotkey combos.
    const KCG_FLAG_MASK_COMMAND: u64 = 0x0010_0000;
    const KCG_FLAG_MASK_SHIFT: u64 = 0x0002_0000;
    const KCG_FLAG_MASK_CONTROL: u64 = 0x0004_0000;
    const KCG_FLAG_MASK_ALTERNATE: u64 = 0x0008_0000;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
    // kCGKeyboardEventAutorepeat — nonzero for the synthetic repeat keydowns the OS
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
    static ASR: OnceLock<Arc<whimpr_asr::WhisperEngine>> = OnceLock::new();
    static OPENAI: OnceLock<Mutex<Option<whimpr_cleanup::OpenAiProvider>>> = OnceLock::new();
    static ANTHROPIC: OnceLock<Mutex<Option<whimpr_cleanup::AnthropicProvider>>> = OnceLock::new();
    static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
    static SETTINGS: OnceLock<Mutex<whimpr_core::Settings>> = OnceLock::new();
    static DICTIONARY: OnceLock<Mutex<whimpr_core::DictionaryStore>> = OnceLock::new();
    static SNIPPETS: OnceLock<Mutex<whimpr_core::SnippetStore>> = OnceLock::new();
    static STATS: OnceLock<Mutex<whimpr_core::StatsStore>> = OnceLock::new();
    /// (raw pre-cleanup text, final pasted text) from the most recent dictation —
    /// feeds the "undo last cleanup edit" hotkey (Cmd+Shift+Z). `None` until the
    /// first dictation completes this run.
    static LAST_TEXTS: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();

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

    /// The whisper ASR model to load: prefer the most accurate one present, in
    /// descending quality order, falling back to the small base model. Bigger
    /// English models mis-hear names/technical terms far less (and better ASR means
    /// less for cleanup and the dictionary to fix downstream).
    ///
    /// `language` is the user's selected ASR language (`None`/`Some("en")` = auto or
    /// English). `.en`-suffixed models are English-only — they cannot transcribe
    /// other languages at all — so when a specific *non-English* language is
    /// selected we only consider multilingual model files (no `.en` suffix).
    /// Otherwise we keep preferring `.en` models first (better English accuracy),
    /// falling back to multilingual files if no `.en` model is present.
    fn model_path(language: Option<&str>) -> PathBuf {
        let dir = support_dir().join("models");
        let needs_multilingual = matches!(language, Some(lang) if lang != "en");
        let candidates: &[&str] = if needs_multilingual {
            &[
                "ggml-large-v3-turbo.bin",
                "ggml-medium.bin",
                "ggml-small.bin",
                "ggml-base.bin",
            ]
        } else {
            &[
                "ggml-large-v3-turbo.bin",
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

    /// Seconds since the Unix epoch (UTC), or 0 if the clock is before the epoch.
    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Log one completed dictation to the stats store (words, speaking time, text,
    /// target app) and persist it. Powers both the Hub stats and the history list.
    pub fn record_dictation(text: &str, duration_secs: f32) {
        let words = whimpr_core::stats::count_words(text);
        if words == 0 {
            return;
        }
        let app = TARGET_APP.get().and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
        if let Some(m) = STATS.get() {
            let mut store = m.lock().unwrap_or_else(|e| e.into_inner());
            let duration_ms = (duration_secs.max(0.0) * 1000.0) as u32;
            let chars = text.chars().count() as u32;
            store.record(words, duration_ms, chars, unix_now(), text.to_string(), app);
            let _ = store.save(&stats_path());
        }
    }

    /// The most recent dictations for the Hub Home history list.
    pub fn history(limit: usize) -> Vec<whimpr_core::HistoryItem> {
        STATS
            .get()
            .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).history(limit))
            .unwrap_or_default()
    }

    /// The most recent dictation's text, for the "paste/copy last transcript"
    /// hotkeys. `None` if nothing has been dictated yet this run.
    fn latest_transcript() -> Option<String> {
        STATS
            .get()
            .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).history(1).into_iter().next().map(|r| r.text))
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
    /// cursor position — it does not attempt to find-and-replace the previously
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
            .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).summary(tz_offset_minutes, unix_now()))
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
    /// Apply new settings and rebuild the cloud providers (picks up model changes).
    pub fn update_settings(new: whimpr_core::Settings) {
        if let Some(m) = SETTINGS.get() {
            *m.lock().unwrap_or_else(|e| e.into_inner()) = new.clone();
        }
        let _ = new.save(&settings_path());
        rebuild_providers();
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
    /// Returns `(raw_out, final_text)`: `raw_out` is the pre-cleanup transcript after
    /// `pre_normalize_layout`/`post_process` (what would be pasted if cleanup were
    /// skipped or rejected — used by the "undo last cleanup edit" hotkey to restore
    /// the un-cleaned text), and `final_text` is what actually gets pasted.
    fn clean_transcript(raw: &str) -> (String, String) {
        let settings = current_settings();
        let level = settings.cleanup_level;
        if matches!(settings.cleanup_mode, CleanupMode::Raw) || level.bypasses_llm() {
            return (raw.to_string(), raw.to_string());
        }
        // Turn explicit spoken layout cues ("new line", "new paragraph") into break
        // markers up front — the model passes an opaque marker through reliably but
        // mangles the literal cue words. The model sees `raw` (with markers); the gate
        // and any raw fallback use `raw_out` (markers restored to real breaks) so we
        // never paste a "[[NL]]" token or lose an explicit break.
        let raw_norm = whimpr_core::cleanup::pre_normalize_layout(raw);
        let raw = raw_norm.as_str();
        let raw_out = whimpr_core::cleanup::post_process(&raw_norm);
        let vocab = DICTIONARY
            .get()
            .map(|d| d.lock().unwrap_or_else(|e| e.into_inner()).prefilter(raw, 15))
            .unwrap_or_default();
        let app_bundle_id = TARGET_APP.get().and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).clone());
        if let Some(app) = app_bundle_id.as_deref() {
            eprintln!("[whimpr] cleanup target app: {app}");
        }
        let ctx = CleanupContext {
            level,
            vocab,
            app_bundle_id,
            style: settings.style.to_instructions(),
            ..Default::default()
        };
        // Run the on-device model with the same prompt + per-app formatting.
        let run_local = || -> Option<anyhow::Result<String>> {
            LOCAL.get().and_then(|m| {
                m.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|w| {
                    // System prompt + few-shot demonstration turns + the transcript,
                    // so the on-device model actually produces newlines/lists and
                    // resolves self-corrections instead of just being told to.
                    let messages = whimpr_core::cleanup::build_messages(raw, &ctx);
                    w.cleanup(&messages)
                })
            })
        };
        // Selected provider, falling back to local when a cloud key can't be read
        // (so cleanup still runs) — and Local mode uses the worker directly.
        let result: Option<anyhow::Result<String>> = match settings.cleanup_mode {
            CleanupMode::OpenAi => OPENAI
                .get()
                .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.cleanup(raw, &ctx)))
                .or_else(run_local),
            CleanupMode::Anthropic => ANTHROPIC
                .get()
                .and_then(|m| m.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.cleanup(raw, &ctx)))
                .or_else(run_local),
            CleanupMode::Local => run_local(),
            CleanupMode::Raw => None,
        };
        let final_text = match result {
            Some(Ok(cleaned)) => {
                // Deterministic safety net: convert any leftover spoken layout cue the
                // model missed into real line breaks, strip stray code fences, cap blank
                // lines. Guarantees no "new line"/"new paragraph" word reaches the cursor.
                let cleaned = whimpr_core::cleanup::post_process(&cleaned);
                if whimpr_core::cleanup::evaluate_gates(&raw_out, &cleaned, level).passed() {
                    cleaned
                } else {
                    eprintln!("[whimpr] cleanup gate rejected the edit — pasting raw");
                    raw_out.clone()
                }
            }
            Some(Err(e)) => {
                eprintln!("[whimpr] cleanup failed ({e}) — pasting raw");
                raw_out.clone()
            }
            None => {
                if matches!(settings.cleanup_mode, CleanupMode::Local) {
                    eprintln!("[whimpr] local cleanup model not wired yet — pasting raw");
                } else {
                    eprintln!("[whimpr] cleanup provider has no API key — pasting raw");
                }
                raw_out.clone()
            }
        };
        (raw_out, final_text)
    }

    fn now_ms() -> u64 {
        CLOCK.get().map(|c| c.elapsed().as_millis() as u64).unwrap_or(0)
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
        let _ = app.emit_to(OVERLAY_LABEL, "whimpr://flowbar/state", BarPayload { state });
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
    /// end a locked hands-free session — synthesizes the same re-press-to-finalize
    /// input a second `PushToTalk` chord would produce while `Locked`. A no-op in
    /// every other state (Idle, mid-hold, AwaitingLock, Finalizing), matching the
    /// state machine's own handling of a stray `Down` there.
    pub fn confirm_dictation() {
        handle_input(Input::Trigger(TriggerToken::Down {
            binding: BindingId::PushToTalk,
            at_ms: now_ms(),
        }));
    }

    /// Invoked by the pill's Cancel button (Tauri `cancel_dictation` command) —
    /// synthesizes the same `Cancel` trigger the Escape key produces. A no-op
    /// from Idle.
    pub fn cancel_dictation() {
        handle_input(Input::Trigger(TriggerToken::Cancel { at_ms: now_ms() }));
    }

    fn apply_action(app: &AppHandle, action: Action) {
        match action {
            Action::ShowBar(bar) => {
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
                let app_thread = app.clone();
                std::thread::spawn(move || {
                    let app_cb = app_thread.clone();
                    match whimpr_audio::start(move |bars| {
                        let _ = app_cb.emit_to(
                            OVERLAY_LABEL,
                            "whimpr://audio/waveform",
                            WavePayload { bars: bars.to_vec() },
                        );
                    }) {
                        Ok(handle) => {
                            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
                        }
                        Err(e) => eprintln!("[whimpr] mic capture failed to start: {e}"),
                    }
                });
            }
            // Stop the mic, transcribe the buffered audio, and advance the machine.
            Action::StopCaptureAndFinalize { session } => {
                let app2 = app.clone();
                let handle = CAPTURE.get().and_then(|slot| slot.lock().unwrap_or_else(|e| e.into_inner()).take());
                std::thread::spawn(move || {
                    // Whatever happens, return the pill to idle (done -> idle).
                    let finish =
                        || handle_input(Input::Pipeline(PipelineEvent::Committed { session }));
                    let Some(res) = handle.and_then(|h| h.stop()) else {
                        eprintln!("[whimpr] no audio captured");
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
                            "[whimpr] ⚠ audio is silent — the mic isn't being captured. Grant \
                             Microphone access to your terminal (System Settings → Privacy & \
                             Security → Microphone), then fully quit + reopen it and rerun."
                        );
                    }
                    let Some(asr) = ASR.get().cloned() else {
                        eprintln!("[whimpr] ASR not ready (model still loading or missing)");
                        finish();
                        return;
                    };
                    let pcm = whimpr_audio::resample_to_16k(&res.samples, res.sample_rate);
                    match asr.transcribe(&pcm) {
                        Ok(t) => {
                            let raw = t.text;
                            eprintln!("[whimpr] TRANSCRIPT: \"{}\"", raw);
                            // Static snippets are checked first, on the raw transcript,
                            // before cleanup runs. A match pastes the expansion verbatim
                            // and skips the whole cleanup pipeline (no LLM call, no gates).
                            let snippet_expansion = SNIPPETS.get().and_then(|m| {
                                m.lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .find_match(&raw)
                                    .map(|entry| entry.expansion.clone())
                            });
                            let (raw_out, text) = match snippet_expansion {
                                Some(expansion) => {
                                    eprintln!("[whimpr] SNIPPET matched — pasting expansion directly");
                                    (expansion.clone(), expansion)
                                }
                                None => {
                                    // Clean the transcript (cloud LLM if configured), then paste.
                                    let (raw_out, text) = clean_transcript(&raw);
                                    if text != raw {
                                        eprintln!("[whimpr] CLEANED:   \"{}\"", text);
                                    }
                                    (raw_out, text)
                                }
                            };
                            if !text.is_empty() {
                                if let Err(e) = crate::paste::paste_text(&text) {
                                    eprintln!("[whimpr] paste failed: {e}");
                                }
                                // Stash (raw, final) for the "undo last cleanup edit"
                                // hotkey (Cmd+Shift+Z), right after the paste.
                                *LAST_TEXTS
                                    .get_or_init(|| Mutex::new(None))
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner()) = Some((raw_out, text.clone()));
                                // Log words + speaking time for the Hub stats (WPM, streak…).
                                record_dictation(&text, res.duration_secs());
                                // Watch the field for a post-paste correction to learn (✨).
                                crate::autolearn::watch_correction(&text);
                            }
                            let _ = app2.emit_to(
                                OVERLAY_LABEL,
                                "whimpr://transcript",
                                TranscriptPayload { text },
                            );
                        }
                        Err(e) => eprintln!("[whimpr] ASR error: {e}"),
                    }
                    finish();
                });
            }
            Action::DiscardCapture { .. } => {
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
                'A' => 0, 'B' => 11, 'C' => 8, 'D' => 2, 'E' => 14, 'F' => 3, 'G' => 5,
                'H' => 4, 'I' => 34, 'J' => 38, 'K' => 40, 'L' => 37, 'M' => 46, 'N' => 45,
                'O' => 31, 'P' => 35, 'Q' => 12, 'R' => 15, 'S' => 1, 'T' => 17, 'U' => 32,
                'V' => 9, 'W' => 13, 'X' => 7, 'Y' => 16, 'Z' => 6,
                '0' => 29, '1' => 18, '2' => 19, '3' => 20, '4' => 21, '5' => 23, '6' => 22,
                '7' => 26, '8' => 28, '9' => 25,
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
                    *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = target;
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
            // Ignore OS-synthesized auto-repeat keydowns — fire once per physical
            // press, not once per repeat tick while the chord is held.
            let autorepeat = unsafe {
                CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_AUTOREPEAT)
            };
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
                "local Command Mode is unavailable in this build — set Cleanup Engine to OpenAI                  or Anthropic (Settings) to use Transforms / Command Mode"
            )
        };
        match settings.cleanup_mode {
            CleanupMode::OpenAi => OPENAI
                .get()
                .and_then(|m| {
                    m.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_ref()
                        .map(|p| p.command_edit(selection, instruction))
                })
                .unwrap_or_else(|| run_local(selection, instruction)),
            CleanupMode::Anthropic => ANTHROPIC
                .get()
                .and_then(|m| {
                    m.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_ref()
                        .map(|p| p.command_edit(selection, instruction))
                })
                .unwrap_or_else(|| run_local(selection, instruction)),
            CleanupMode::Local | CleanupMode::Raw => run_local(selection, instruction),
        }
    }

    /// Manual hook for Command Mode / Transforms, reachable from the Hub without the
    /// Fn+Ctrl hotkey — exercises the prompt + provider layer directly.
    pub fn test_command_edit(selection: String, instruction: String) -> Result<String, String> {
        run_command_edit(&selection, &instruction).map_err(|e| e.to_string())
    }

    /// Copy settings/dictionary/snippets/stats into a timestamped backup folder.
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
        let _ = SETTINGS.set(Mutex::new(settings));
        let _ = DICTIONARY.set(Mutex::new(dict));
        let _ = SNIPPETS.set(Mutex::new(whimpr_core::SnippetStore::load(&snippets_path())));
        let _ = STATS.set(Mutex::new(whimpr_core::StatsStore::load(&stats_path())));
        rebuild_providers();

        // Load the speech-to-text model off the main thread (it takes ~1s).
        std::thread::spawn(move || {
            let path = model_path(language_for_model.as_deref());
            if !path.exists() {
                eprintln!("[whimpr] ASR model not found at {}", path.display());
                return;
            }
            match whimpr_asr::WhisperEngine::load(&path) {
                Ok(engine) => {
                    let _ = ASR.set(Arc::new(engine));
                    eprintln!("[whimpr] ASR model loaded — ready to transcribe");
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
        // silently limited to frontmost-only — the exact bug. Prompt for it up front.
        if crate::paste::is_trusted() {
            eprintln!("[whimpr] Accessibility granted — Fn works in every app, paste enabled");
        } else {
            eprintln!(
                "[whimpr] ⚠ Accessibility NOT granted — Fn only works while WhimprFlow is \
                 frontmost and paste is disabled. Prompting; grant WhimprFlow under System \
                 Settings → Privacy & Security → Accessibility (no relaunch needed)."
            );
            crate::paste::prompt_accessibility();
        }
        // Input Monitoring is NOT the gate for a CGEventTap — kept only as diagnostics.
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
        // tap's privilege at CGEventTapCreate time — a tap born untrusted is
        // permanently frontmost-only and is NOT upgraded when the grant later arrives.
        // Polling here also means the Fn key starts working the moment the user grants
        // Accessibility, without a relaunch.
        std::thread::spawn(|| {
            while !crate::paste::is_trusted() {
                std::thread::sleep(Duration::from_millis(500));
            }
            eprintln!("[whimpr] Accessibility present — creating global Fn tap");
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
                    "[whimpr] Fn tap null despite Accessibility — likely a stale TCC entry from \
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
    backup_data, cancel_dictation, confirm_dictation, current_settings, dictionary_add,
    dictionary_entries, dictionary_learn, dictionary_remove, history, install, rebuild_providers,
    snippet_add, snippet_entries, snippet_remove, stats_summary, test_command_edit, update_settings,
};

// Windows uses the real (but unverified) platform layer in `crate::win`.
#[cfg(target_os = "windows")]
pub use crate::win::{
    cancel_dictation, confirm_dictation, current_settings, dictionary_add, dictionary_entries,
    dictionary_learn, dictionary_remove, history, install, rebuild_providers, snippet_add,
    snippet_entries, snippet_remove, stats_summary, update_settings,
};

// Linux uses the real (but unverified) platform layer in `crate::linux` — X11 only
// for this pass; see that module's doc comment for the Wayland follow-up and the
// XGrabKey / xdotool simplifications made.
#[cfg(target_os = "linux")]
pub use crate::linux::{
    cancel_dictation, confirm_dictation, current_settings, dictionary_add, dictionary_entries,
    dictionary_learn, dictionary_remove, history, install, rebuild_providers, snippet_add,
    snippet_entries, snippet_remove, stats_summary, update_settings,
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
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub use other::{
    cancel_dictation, confirm_dictation, current_settings, dictionary_add, dictionary_entries,
    dictionary_learn, dictionary_remove, history, install, rebuild_providers, snippet_add,
    snippet_entries, snippet_remove, stats_summary, update_settings,
};
