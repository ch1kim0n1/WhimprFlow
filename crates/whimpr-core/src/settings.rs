//! User settings, persisted as JSON. Drives the cleanup engine (which provider,
//! how aggressive) and other behavior. Kept dependency-light so it lives in core.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cleanup::CleanupLevel;

/// How formal the cleaned-up text should read. `Neutral` (the default) adds no
/// steering at all  -  the cleanup prompt's own defaults apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Formality {
    Casual,
    #[default]
    Neutral,
    Formal,
}

/// Defensive cap on the free-text style note, so a runaway note can't bloat
/// every cleanup prompt. The Shortcuts/Style UI also limits input length.
pub const MAX_STYLE_INSTRUCTIONS_LEN: usize = 600;

/// A user's personal writing style, applied to cleanup output as PRESENTATION
/// guidance only: it changes how the already-spoken words are shaped (tone,
/// formality, a free-text note), never what they say. The cleanup engine's
/// "never invent facts, greetings, or sign-offs" contract still holds  -  style
/// only picks among ways to present the real words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StyleProfile {
    #[serde(default)]
    pub formality: Formality,
    /// Free-text preference the user writes ("keep it punchy", "British
    /// spelling", "no exclamation marks"). Capped when rendered (see
    /// [`StyleProfile::to_instructions`]).
    #[serde(default)]
    pub custom_instructions: String,
}

impl StyleProfile {
    /// Render into a system-prompt fragment, or `None` when the profile is the
    /// neutral default (nothing to steer). The caller appends this under a
    /// "# Personal Style" heading; the text is presentation-only guidance.
    pub fn to_instructions(&self) -> Option<String> {
        let mut lines: Vec<String> = Vec::new();
        match self.formality {
            Formality::Casual => lines.push(
                "Lean casual and conversational: contractions are fine, keep it relaxed and \
                 plain-spoken."
                    .to_string(),
            ),
            Formality::Formal => lines.push(
                "Lean formal and professional: avoid slang and contractions, prefer complete, \
                 measured sentences."
                    .to_string(),
            ),
            Formality::Neutral => {}
        }
        let note: String = self
            .custom_instructions
            .trim()
            .chars()
            .take(MAX_STYLE_INSTRUCTIONS_LEN)
            .collect();
        if !note.is_empty() {
            lines.push(format!("Additional user preference: {note}"));
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }
}

/// Context Capsule configuration: the opt-in (default OFF) per-app context
/// bundle captured at record start (frontmost app, AX-selected text, glossary,
/// style). Everything defaults to off/empty so existing users see zero change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CapsuleSettings {
    /// Master switch. Off = no capsule is ever captured.
    #[serde(default)]
    pub enabled: bool,
    /// Also capture the AX-selected text in the target app (macOS).
    #[serde(default)]
    pub include_selection: bool,
    /// Bundle ids the capsule is limited to. Empty = all apps (when enabled).
    #[serde(default)]
    pub apps: Vec<String>,
}

/// Which cleanup engine processes transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMode {
    /// Paste the raw transcript (no cleanup).
    Raw,
    /// Local on-device model (default  -  works offline, no API key).
    #[default]
    Local,
    /// OpenAI cloud.
    OpenAi,
    /// Anthropic cloud.
    Anthropic,
}

/// One bindable key: the letters/digits the rebindable shortcuts actually use,
/// plus Escape for Cancel. Deliberately not a general keyboard-event type  -
/// bounded to what this app's rebindable actions need, which keeps the
/// per-platform native-keycode lookup a small, exhaustively-checkable table
/// instead of a full OS keycode enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Key {
    /// Always stored uppercase ASCII (`'A'..='Z'` or `'0'..='9'`).
    Char(char),
    Escape,
}

/// A modifier chord bound to one rebindable action, checked on a plain KeyDown
/// (not a hold gesture like push-to-talk). Field names describe the physical
/// key on each platform: `meta` = Cmd (macOS) / Win key (Windows); `alt` =
/// Option (macOS) / Alt (Windows). All four must match exactly  -  no
/// "at-least-these" matching  -  so a chord with no modifiers (like the default
/// Cancel = bare Escape) can't accidentally also fire with modifiers held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chord {
    pub meta: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

impl Chord {
    pub fn new(meta: bool, ctrl: bool, alt: bool, shift: bool, key: Key) -> Self {
        Self {
            meta,
            ctrl,
            alt,
            shift,
            key,
        }
    }

    /// No modifiers held at all  -  used for Cancel's bare-Escape default and to
    /// reject a would-be binding that's just a plain letter with nothing held
    /// (would collide with normal typing).
    pub fn has_any_modifier(&self) -> bool {
        self.meta || self.ctrl || self.alt || self.shift
    }
}

/// The user's bindings for the shortcuts that are safe to rebind: a single
/// modifier-chord checked on an ordinary KeyDown event. Push-to-talk,
/// hands-free lock (double-tap push-to-talk), and Command Mode are
/// deliberately NOT here  -  push-to-talk/hands-free are tied to the platform's
/// special "hold key" gesture (Fn on macOS / Right Ctrl on Windows) and
/// Command Mode either rides that same gesture (macOS: Fn+Ctrl) or is a
/// not-yet-implemented stub (Windows)  -  none of the three fit the
/// chord-on-keydown model these four do. The Shortcuts UI shows all of them,
/// but only these four have a "change" button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBindings {
    pub cancel: Chord,
    pub paste_last: Chord,
    pub copy_last: Chord,
    pub undo_last: Chord,
}

impl Default for KeyBindings {
    fn default() -> Self {
        // Matches what was hardcoded before this became configurable: macOS
        // used Cmd+Shift+<key>, Windows used Ctrl+Alt+<key>, for the same
        // three actions. Cancel is bare Escape on both.
        #[cfg(target_os = "macos")]
        {
            Self {
                cancel: Chord::new(false, false, false, false, Key::Escape),
                paste_last: Chord::new(true, false, false, true, Key::Char('V')),
                copy_last: Chord::new(true, false, false, true, Key::Char('C')),
                undo_last: Chord::new(true, false, false, true, Key::Char('Z')),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self {
                cancel: Chord::new(false, false, false, false, Key::Escape),
                paste_last: Chord::new(false, true, true, false, Key::Char('V')),
                copy_last: Chord::new(false, true, true, false, Key::Char('C')),
                undo_last: Chord::new(false, true, true, false, Key::Char('Z')),
            }
        }
    }
}

impl KeyBindings {
    /// All four bindings paired with a stable name, for iterating (conflict
    /// checks, the platform hotkey matcher, the Shortcuts UI).
    pub fn entries(&self) -> [(&'static str, Chord); 4] {
        [
            ("cancel", self.cancel),
            ("paste_last", self.paste_last),
            ("copy_last", self.copy_last),
            ("undo_last", self.undo_last),
        ]
    }

    /// The name of whichever binding (if any) collides with `chord`, excluding
    /// `except` (so re-saving a binding with its own unchanged value isn't
    /// flagged as colliding with itself).
    pub fn conflict_with(&self, chord: Chord, except: &str) -> Option<&'static str> {
        self.entries()
            .into_iter()
            .find(|(name, bound)| *name != except && *bound == chord)
            .map(|(name, _)| name)
    }

    /// Human-readable conflict descriptions for every pair of colliding bindings.
    pub fn validate_keybindings(&self) -> Vec<String> {
        let mut out = Vec::new();
        let entries = self.entries();
        for (i, (name, chord)) in entries.iter().enumerate() {
            for (other, other_chord) in entries.iter().skip(i + 1) {
                if chord == other_chord {
                    out.push(format!("Conflict with {other} (same key as {name})"));
                }
            }
        }
        out
    }
}

/// Persisted user configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub cleanup_mode: CleanupMode,
    pub cleanup_level: CleanupLevel,
    pub openai_model: String,
    /// API root for the "OpenAI" cleanup mode, e.g. `https://openrouter.ai/api/v1`
    /// to route through OpenRouter instead of OpenAI directly (same wire format).
    /// Empty string (the default) means OpenAI's own endpoint.
    #[serde(default)]
    pub openai_base_url: String,
    pub anthropic_model: String,
    /// Play the record-start ping.
    pub sound_on_start: bool,
    /// Redact curses and inappropriate words from inserted text.
    #[serde(default)]
    pub safe_mode: bool,
    /// ASR language, as a whisper.cpp language code (e.g. `"en"`, `"es"`).
    /// `None` (the default) means auto-detect. `#[serde(default)]` keeps older
    /// settings.json files (written before this field existed) loading cleanly.
    #[serde(default)]
    pub language: Option<String>,
    /// User-customizable hotkeys for cancel/paste-last/copy-last/undo-last.
    /// `#[serde(default)]` keeps older settings.json files loading cleanly.
    #[serde(default)]
    pub keybindings: KeyBindings,
    /// Personal writing style applied to cleanup output (tone/formality/note).
    /// `#[serde(default)]` keeps older settings.json files loading cleanly.
    #[serde(default)]
    pub style: StyleProfile,
    /// How many days of dictation text to keep in history. `None` keeps text
    /// forever; `Some(0)` never stores text at all. Default is 30 days.
    /// Numeric stats are always kept regardless.
    #[serde(default = "default_retention_days")]
    pub retention_days: Option<u32>,
    /// Context Capsule (opt-in per-app context bundle). Defaults to fully off.
    #[serde(default)]
    pub capsule: CapsuleSettings,
    /// Switch the cleanup prompt to the code-dictation variant when the target
    /// app is an IDE or terminal. Default on.
    #[serde(default = "default_true")]
    pub code_mode_auto: bool,
    /// Meeting mode: a locked (hands-free) session's transcript is appended to
    /// Notes instead of pasted. Default off.
    #[serde(default)]
    pub meeting_mode: bool,
    /// Show live provisional text in the FlowBar while recording. Default on.
    #[serde(default = "default_true")]
    pub streaming_preview: bool,
    /// Clear the system clipboard after paste when the previous clipboard was empty.
    #[serde(default = "default_true")]
    pub clear_clipboard_after_paste: bool,
    /// Soft RAM budget hint for the local LLM worker (0 = unlimited).
    #[serde(default)]
    pub max_ram_mb: u32,
    /// Unload Whisper after this many idle minutes (0 = never).
    #[serde(default)]
    pub unload_asr_after_idle_minutes: u32,
    /// Play a short confirmation sound after a successful paste.
    #[serde(default)]
    pub sound_on_complete: bool,
    /// When true, panic hook writes a local crash report file (never uploaded).
    #[serde(default)]
    pub crash_reporting_opt_in: bool,
    /// Close Hub to tray instead of quitting. Default on.
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    /// Preferred microphone name (`None` = system default).
    #[serde(default)]
    pub input_device: Option<String>,
    /// Let Whisper emit punctuation. Default on.
    #[serde(default = "default_true")]
    pub auto_punctuate: bool,
    /// Extra filler words stripped during cleanup (case-insensitive).
    #[serde(default = "default_fillers")]
    pub custom_fillers: Vec<String>,
    /// Active ASR model file path (under the models directory or absolute).
    #[serde(default)]
    pub asr_model: Option<String>,
    /// Settings schema version for forward migrations.
    #[serde(default = "default_settings_version")]
    pub settings_version: u32,
    /// Last app version whose changelog the user dismissed.
    #[serde(default)]
    pub last_seen_version: Option<String>,
    /// Onboarding wizard step (`None` = not started, `Some(5)` = complete).
    #[serde(default)]
    pub onboarding_step: Option<u32>,
}

fn default_fillers() -> Vec<String> {
    ["um", "uh", "er", "ah", "like"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn default_settings_version() -> u32 {
    1
}

/// Serde default for the settings that ship ON (see `default = "default_true"`).
fn default_true() -> bool {
    true
}

fn default_retention_days() -> Option<u32> {
    Some(30)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cleanup_mode: CleanupMode::default(),
            cleanup_level: CleanupLevel::Light,
            openai_model: "gpt-4o-mini".to_string(),
            openai_base_url: String::new(),
            anthropic_model: "claude-haiku-4-5".to_string(),
            sound_on_start: true,
            safe_mode: false,
            language: None,
            keybindings: KeyBindings::default(),
            style: StyleProfile::default(),
            retention_days: Some(30),
            capsule: CapsuleSettings::default(),
            code_mode_auto: true,
            meeting_mode: false,
            streaming_preview: true,
            clear_clipboard_after_paste: true,
            max_ram_mb: 0,
            unload_asr_after_idle_minutes: 0,
            sound_on_complete: false,
            crash_reporting_opt_in: false,
            minimize_to_tray: true,
            input_device: None,
            auto_punctuate: true,
            custom_fillers: default_fillers(),
            asr_model: None,
            settings_version: default_settings_version(),
            last_seen_version: None,
            onboarding_step: None,
        }
    }
}

/// Free-function alias used by the Hub / IPC layer.
pub fn validate_keybindings(kb: &KeyBindings) -> Vec<String> {
    kb.validate_keybindings()
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut s: Self = crate::json_store::load_or_recover(path);
        const EXPECTED: u32 = 1;
        if s.settings_version < EXPECTED {
            tracing::info!(
                target: "whimpr",
                from = s.settings_version,
                to = EXPECTED,
                "migrating settings schema"
            );
            // Placeholder for migrate_v1_to_v2 etc.
            s.settings_version = EXPECTED;
            let _ = s.save(path);
        }
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

/// Remove whole-word filler tokens (case-insensitive) from a transcript.
pub fn strip_fillers(text: &str, fillers: &[String]) -> String {
    if fillers.is_empty() {
        return text.to_string();
    }
    let set: std::collections::HashSet<String> = fillers
        .iter()
        .map(|f| f.trim().to_ascii_lowercase())
        .filter(|f| !f.is_empty())
        .collect();
    if set.is_empty() {
        return text.to_string();
    }
    text.split_whitespace()
        .filter(|w| {
            let bare = w
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_ascii_lowercase();
            !set.contains(&bare)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drop ASCII sentence punctuation when auto-punctuation is disabled.
pub fn strip_auto_punctuation(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                c,
                ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\u{201c}' | '\u{201d}'
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_fillers_removes_listed_words() {
        let out = strip_fillers("um hello uh world", &default_fillers());
        assert_eq!(out, "hello world");
    }

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.cleanup_mode, CleanupMode::Local);
        assert_eq!(s.cleanup_level, CleanupLevel::Light);
        assert_eq!(s.language, None);
        assert!(!s.safe_mode);
    }

    #[test]
    fn language_absent_in_json_defaults_to_none() {
        // Back-compat: a settings.json written before `language` existed should
        // still load, with `language` defaulting to `None` (auto-detect).
        let json = r#"{
            "cleanup_mode": "local",
            "cleanup_level": "light",
            "openai_model": "gpt-4o-mini",
            "anthropic_model": "claude-haiku-4-5",
            "sound_on_start": true
        }"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.language, None);
        assert!(!s.safe_mode);
    }

    #[test]
    fn safe_mode_round_trips_and_old_settings_remain_compatible() {
        let old_json = r#"{
            "cleanup_mode": "local",
            "cleanup_level": "light",
            "openai_model": "gpt-4o-mini",
            "anthropic_model": "claude-haiku-4-5",
            "sound_on_start": true
        }"#;
        let old: Settings = serde_json::from_str(old_json).unwrap();
        assert!(
            !old.safe_mode,
            "safe mode must stay opt-in for existing users"
        );

        let enabled_json = r#"{
            "cleanup_mode": "local",
            "cleanup_level": "light",
            "openai_model": "gpt-4o-mini",
            "anthropic_model": "claude-haiku-4-5",
            "sound_on_start": true,
            "safe_mode": true
        }"#;
        let enabled: Settings = serde_json::from_str(enabled_json).unwrap();
        assert!(enabled.safe_mode);
    }

    #[test]
    fn round_trips_json() {
        let s = Settings {
            cleanup_mode: CleanupMode::Local,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cleanup_mode, CleanupMode::Local);
        assert!(!back.safe_mode);
    }

    #[test]
    fn keybindings_absent_in_json_uses_platform_default() {
        // Back-compat: a settings.json written before `keybindings` existed
        // should still load, falling back to the platform default chords.
        let json = r#"{
            "cleanup_mode": "local",
            "cleanup_level": "light",
            "openai_model": "gpt-4o-mini",
            "anthropic_model": "claude-haiku-4-5",
            "sound_on_start": true
        }"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.keybindings, KeyBindings::default());
    }

    #[test]
    fn default_bindings_have_no_conflicts_with_each_other() {
        let kb = KeyBindings::default();
        for (name, chord) in kb.entries() {
            assert_eq!(
                kb.conflict_with(chord, name),
                None,
                "{name} should not conflict with itself"
            );
        }
        // Cross-check: no two DIFFERENT default bindings share a chord.
        let entries = kb.entries();
        for i in 0..entries.len() {
            for j in 0..entries.len() {
                if i != j {
                    assert_ne!(
                        entries[i].1, entries[j].1,
                        "{} and {} collide",
                        entries[i].0, entries[j].0
                    );
                }
            }
        }
    }

    #[test]
    fn conflict_with_detects_a_rebind_that_collides_with_another_action() {
        let kb = KeyBindings::default();
        // Try to rebind "copy_last" to the same chord "paste_last" already uses.
        let collision = kb.conflict_with(kb.paste_last, "copy_last");
        assert_eq!(collision, Some("paste_last"));
    }

    #[test]
    fn conflict_with_ignores_the_bindings_own_unchanged_value() {
        let kb = KeyBindings::default();
        // Re-saving "paste_last" with the value it already has must not flag
        // itself as a conflict.
        assert_eq!(kb.conflict_with(kb.paste_last, "paste_last"), None);
    }

    #[test]
    fn neutral_style_with_no_note_renders_nothing() {
        let s = StyleProfile::default();
        assert_eq!(s.formality, Formality::Neutral);
        assert_eq!(s.to_instructions(), None);
    }

    #[test]
    fn formality_and_note_both_render() {
        let s = StyleProfile {
            formality: Formality::Formal,
            custom_instructions: "  British spelling  ".to_string(),
        };
        let out = s.to_instructions().expect("some instructions");
        assert!(out.contains("formal"));
        assert!(out.contains("British spelling"));
        // Trimmed, no leading/trailing whitespace leaked into the note line.
        assert!(out.contains("Additional user preference: British spelling"));
    }

    #[test]
    fn casual_alone_renders_without_a_note() {
        let s = StyleProfile {
            formality: Formality::Casual,
            custom_instructions: String::new(),
        };
        let out = s.to_instructions().expect("some instructions");
        assert!(out.contains("casual"));
        assert!(!out.contains("Additional user preference"));
    }

    #[test]
    fn long_note_is_capped() {
        let s = StyleProfile {
            formality: Formality::Neutral,
            custom_instructions: "x".repeat(MAX_STYLE_INSTRUCTIONS_LEN + 50),
        };
        let out = s.to_instructions().expect("some instructions");
        // "Additional user preference: " prefix + exactly MAX chars of note.
        let note_len = out
            .trim_start_matches("Additional user preference: ")
            .chars()
            .count();
        assert_eq!(note_len, MAX_STYLE_INSTRUCTIONS_LEN);
    }

    #[test]
    fn new_privacy_and_mode_fields_default_correctly_from_old_json() {
        // Back-compat: a settings.json written before retention/capsule/code
        // mode/meeting mode/streaming preview existed must still load, with
        // each field at its designed default.
        let json = r#"{
            "cleanup_mode": "local",
            "cleanup_level": "light",
            "openai_model": "gpt-4o-mini",
            "anthropic_model": "claude-haiku-4-5",
            "sound_on_start": true
        }"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.retention_days, Some(30), "default retention is 30 days");
        assert_eq!(s.capsule, CapsuleSettings::default());
        assert!(!s.capsule.enabled, "capsule is strictly opt-in");
        assert!(!s.capsule.include_selection);
        assert!(s.capsule.apps.is_empty());
        assert!(s.code_mode_auto, "code mode auto ships ON");
        assert!(!s.meeting_mode, "meeting mode ships OFF");
        assert!(s.streaming_preview, "streaming preview ships ON");
    }

    #[test]
    fn new_fields_round_trip_non_default_values() {
        let s = Settings {
            retention_days: Some(30),
            capsule: CapsuleSettings {
                enabled: true,
                include_selection: true,
                apps: vec!["com.apple.mail".to_string()],
            },
            code_mode_auto: false,
            meeting_mode: true,
            streaming_preview: false,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.retention_days, Some(30));
        assert!(back.capsule.enabled);
        assert!(back.capsule.include_selection);
        assert_eq!(back.capsule.apps, vec!["com.apple.mail".to_string()]);
        assert!(!back.code_mode_auto);
        assert!(back.meeting_mode);
        assert!(!back.streaming_preview);
    }

    #[test]
    fn bare_letter_with_no_modifier_has_no_modifier_flagged() {
        let plain_v = Chord::new(false, false, false, false, Key::Char('V'));
        assert!(!plain_v.has_any_modifier());
        let cmd_shift_v = Chord::new(true, false, false, true, Key::Char('V'));
        assert!(cmd_shift_v.has_any_modifier());
    }

    // ── Edge case coverage: settings validation, strip_fillers, retention ──

    #[test]
    fn validate_keybindings_empty_for_defaults() {
        let kb = KeyBindings::default();
        assert!(
            kb.validate_keybindings().is_empty(),
            "defaults must have no conflicts"
        );
    }

    #[test]
    fn validate_keybindings_detects_all_collisions() {
        // Set all four bindings to the same chord → 6 pairwise conflicts (4 choose 2).
        let chord = Chord::new(true, false, false, false, Key::Char('X'));
        let kb = KeyBindings {
            cancel: chord,
            paste_last: chord,
            copy_last: chord,
            undo_last: chord,
        };
        let conflicts = kb.validate_keybindings();
        // 4 bindings, all same → C(4,2) = 6 pairs.
        assert_eq!(
            conflicts.len(),
            6,
            "4 identical bindings → 6 pairwise conflicts"
        );
    }

    #[test]
    fn validate_keybindings_detects_partial_collision() {
        // Only two bindings collide.
        let kb = KeyBindings {
            paste_last: Chord::new(true, false, false, false, Key::Char('P')),
            copy_last: Chord::new(true, false, false, false, Key::Char('P')),
            ..KeyBindings::default()
        };
        let conflicts = kb.validate_keybindings();
        assert_eq!(conflicts.len(), 1, "one pair collides");
        assert!(conflicts[0].contains("paste_last") || conflicts[0].contains("copy_last"));
    }

    #[test]
    fn strip_fillers_empty_list_returns_unchanged() {
        let out = strip_fillers("um hello world", &[]);
        assert_eq!(out, "um hello world");
    }

    #[test]
    fn strip_fillers_only_filler_words() {
        let out = strip_fillers("um uh er", &default_fillers());
        assert_eq!(out, "");
    }

    #[test]
    fn strip_fillers_case_insensitive() {
        let out = strip_fillers("UM Hello UH World", &default_fillers());
        assert_eq!(out, "Hello World");
    }

    #[test]
    fn strip_fillers_preserves_punctuation_on_kept_words() {
        // Filler "um" is stripped, but "hello," keeps its comma.
        let out = strip_fillers("um hello, world", &default_fillers());
        assert_eq!(out, "hello, world");
    }

    #[test]
    fn strip_fillers_custom_list() {
        let out = strip_fillers(
            "like totally yeah",
            &["like".to_string(), "yeah".to_string()],
        );
        assert_eq!(out, "totally");
    }

    #[test]
    fn strip_fillers_empty_string() {
        let out = strip_fillers("", &default_fillers());
        assert_eq!(out, "");
    }

    #[test]
    fn strip_fillers_whitespace_only() {
        let out = strip_fillers("   ", &default_fillers());
        assert_eq!(out, "");
    }

    #[test]
    fn strip_auto_punctuation_removes_sentence_punctuation() {
        let out = strip_auto_punctuation("Hello, world! How are you?");
        assert_eq!(out, "Hello world How are you");
    }

    #[test]
    fn strip_auto_punctuation_preserves_unicode_punctuation() {
        // Unicode quotes are stripped, but other unicode chars survive.
        let out = strip_auto_punctuation("“hello” — world");
        assert_eq!(out, "hello — world");
    }

    #[test]
    fn strip_auto_punctuation_empty_string() {
        assert_eq!(strip_auto_punctuation(""), "");
    }

    #[test]
    fn settings_save_and_load_round_trip() {
        let tmp = std::env::temp_dir().join(format!("whimpr-settings-rt-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        let s = Settings {
            safe_mode: true,
            language: Some("es".to_string()),
            retention_days: Some(7),
            ..Settings::default()
        };
        s.save(&tmp).unwrap();
        let loaded = Settings::load(&tmp);
        assert!(loaded.safe_mode);
        assert_eq!(loaded.language, Some("es".to_string()));
        assert_eq!(loaded.retention_days, Some(7));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn settings_load_missing_file_returns_default() {
        let s = Settings::load(Path::new("/nonexistent/whimpr-settings.json"));
        assert_eq!(s.cleanup_mode, CleanupMode::Local);
        assert!(!s.safe_mode);
    }

    #[test]
    fn settings_load_corrupt_file_returns_default() {
        let tmp =
            std::env::temp_dir().join(format!("whimpr-settings-corrupt-{}", std::process::id()));
        std::fs::write(&tmp, b"{ this is not valid json }").unwrap();
        let s = Settings::load(&tmp);
        assert_eq!(s.cleanup_mode, CleanupMode::Local, "corrupt file → default");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn retention_days_zero_means_never_store_text() {
        let s = Settings {
            retention_days: Some(0),
            ..Settings::default()
        };
        assert_eq!(s.retention_days, Some(0));
    }

    #[test]
    fn retention_days_none_means_keep_forever() {
        let s = Settings {
            retention_days: None,
            ..Settings::default()
        };
        assert_eq!(s.retention_days, None);
    }

    #[test]
    fn chord_equality_and_inequality() {
        let a = Chord::new(true, false, false, false, Key::Char('V'));
        let b = Chord::new(true, false, false, false, Key::Char('V'));
        let c = Chord::new(false, true, false, false, Key::Char('V'));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn chord_escape_key() {
        let esc = Chord::new(false, false, false, false, Key::Escape);
        assert!(!esc.has_any_modifier());
        assert!(matches!(esc.key, Key::Escape));
    }
}
