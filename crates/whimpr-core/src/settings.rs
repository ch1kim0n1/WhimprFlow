//! User settings, persisted as JSON. Drives the cleanup engine (which provider,
//! how aggressive) and other behavior. Kept dependency-light so it lives in core.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cleanup::CleanupLevel;

/// How formal the cleaned-up text should read. `Neutral` (the default) adds no
/// steering at all — the cleanup prompt's own defaults apply.
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
/// "never invent facts, greetings, or sign-offs" contract still holds — style
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

/// Which cleanup engine processes transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMode {
    /// Paste the raw transcript (no cleanup).
    Raw,
    /// Local on-device model (default — works offline, no API key).
    #[default]
    Local,
    /// OpenAI cloud.
    OpenAi,
    /// Anthropic cloud.
    Anthropic,
}

/// One bindable key: the letters/digits the rebindable shortcuts actually use,
/// plus Escape for Cancel. Deliberately not a general keyboard-event type —
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
/// Option (macOS) / Alt (Windows). All four must match exactly — no
/// "at-least-these" matching — so a chord with no modifiers (like the default
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
        Self { meta, ctrl, alt, shift, key }
    }

    /// No modifiers held at all — used for Cancel's bare-Escape default and to
    /// reject a would-be binding that's just a plain letter with nothing held
    /// (would collide with normal typing).
    pub fn has_any_modifier(&self) -> bool {
        self.meta || self.ctrl || self.alt || self.shift
    }
}

/// The user's bindings for the shortcuts that are safe to rebind: a single
/// modifier-chord checked on an ordinary KeyDown event. Push-to-talk,
/// hands-free lock (double-tap push-to-talk), and Command Mode are
/// deliberately NOT here — push-to-talk/hands-free are tied to the platform's
/// special "hold key" gesture (Fn on macOS / Right Ctrl on Windows) and
/// Command Mode either rides that same gesture (macOS: Fn+Ctrl) or is a
/// not-yet-implemented stub (Windows) — none of the three fit the
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
            language: None,
            keybindings: KeyBindings::default(),
            style: StyleProfile::default(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.cleanup_mode, CleanupMode::Local);
        assert_eq!(s.cleanup_level, CleanupLevel::Light);
        assert_eq!(s.language, None);
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
            assert_eq!(kb.conflict_with(chord, name), None, "{name} should not conflict with itself");
        }
        // Cross-check: no two DIFFERENT default bindings share a chord.
        let entries = kb.entries();
        for i in 0..entries.len() {
            for j in 0..entries.len() {
                if i != j {
                    assert_ne!(entries[i].1, entries[j].1, "{} and {} collide", entries[i].0, entries[j].0);
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
        let s = StyleProfile { formality: Formality::Casual, custom_instructions: String::new() };
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
        let note_len = out.trim_start_matches("Additional user preference: ").chars().count();
        assert_eq!(note_len, MAX_STYLE_INSTRUCTIONS_LEN);
    }

    #[test]
    fn bare_letter_with_no_modifier_has_no_modifier_flagged() {
        let plain_v = Chord::new(false, false, false, false, Key::Char('V'));
        assert!(!plain_v.has_any_modifier());
        let cmd_shift_v = Chord::new(true, false, false, true, Key::Char('V'));
        assert!(cmd_shift_v.has_any_modifier());
    }
}
