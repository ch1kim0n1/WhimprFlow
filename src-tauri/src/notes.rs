//! Shell-level notes store: meeting-mode transcripts, workflow "note"
//! destinations, and Studio snap-notes land here. Persisted as JSON at the app
//! support dir's `notes.json`, mirroring the snippets store pattern (load ->
//! mutate -> save). Kept in the shell because it is app glue, not core logic  -
//! and platform-uniform, so every platform's pipeline can append to it.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// One note. `ts_unix` doubles as the removal key: `add` bumps a colliding
/// timestamp past the newest entry (see `next_ts`), so ids created here are
/// unique even within the same second. `remove` still clears every match, in
/// case an old file carries pre-fix duplicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub ts_unix: u64,
    pub title: String,
    pub text: String,
    /// Path of a captured screenshot linked to this note, if any.
    #[serde(default)]
    pub image_path: Option<String>,
}

/// The persisted notes log.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotesStore {
    #[serde(default)]
    pub entries: Vec<Note>,
}

impl NotesStore {
    /// Load from `path`, returning an empty store if missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persist to `path` (creating parent dirs).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, json)
    }
}

/// Platform application-support dir (same shape as `local_llm::app_support_dir`).
fn support_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(base).join("WhimprFlow")
    }
    #[cfg(target_os = "linux")]
    {
        // $XDG_CONFIG_HOME/WhimprFlow, falling back to ~/.config/WhimprFlow per
        // the XDG Base Directory spec  -  matches `linux.rs::support_dir()` so
        // notes.json lands beside the other Linux stores.
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("WhimprFlow");
            }
        }
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config").join("WhimprFlow")
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/WhimprFlow")
    }
}

fn notes_path() -> PathBuf {
    support_dir().join("notes.json")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

static NOTES: OnceLock<Mutex<NotesStore>> = OnceLock::new();

fn store() -> &'static Mutex<NotesStore> {
    NOTES.get_or_init(|| Mutex::new(NotesStore::load(&notes_path())))
}

/// All notes, newest first (the Studio Notes tab reads top-down).
pub fn entries() -> Vec<Note> {
    let guard = store().lock().unwrap_or_else(|e| e.into_inner());
    guard.entries.iter().rev().cloned().collect()
}

/// The creation timestamp for a new note: `now`, bumped past the newest
/// existing entry when they collide. Keeps `ts_unix` (the removal key and the
/// UI's React key) unique  -  two notes created within the same second would
/// otherwise share a key and be removed together.
fn next_ts(entries: &[Note], now: u64) -> u64 {
    match entries.last() {
        Some(last) if now <= last.ts_unix => last.ts_unix + 1,
        _ => now,
    }
}

/// Append a note (timestamped now, unique) and persist.
pub fn add(title: String, text: String, image_path: Option<String>) {
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let ts_unix = next_ts(&guard.entries, unix_now());
    guard.entries.push(Note {
        ts_unix,
        title,
        text,
        image_path,
    });
    let _ = guard.save(&notes_path());
}

/// Remove the note(s) with this timestamp and persist. Returns true if removed.
pub fn remove(ts_unix: u64) -> bool {
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let before = guard.entries.len();
    guard.entries.retain(|n| n.ts_unix != ts_unix);
    let removed = guard.entries.len() != before;
    if removed {
        let _ = guard.save(&notes_path());
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(ts_unix: u64) -> Note {
        Note {
            ts_unix,
            title: String::new(),
            text: String::new(),
            image_path: None,
        }
    }

    #[test]
    fn first_note_uses_now() {
        assert_eq!(next_ts(&[], 100), 100);
    }

    #[test]
    fn later_now_is_kept() {
        assert_eq!(next_ts(&[note(100)], 101), 101);
    }

    #[test]
    fn same_second_bumps_past_last() {
        // Two notes within the same second must not share the removal key.
        assert_eq!(next_ts(&[note(100)], 100), 101);
    }

    #[test]
    fn clock_going_backwards_still_bumps() {
        assert_eq!(next_ts(&[note(100), note(105)], 100), 106);
    }
}
