//! Voice Workflows: named, versioned voice macros. Say a trigger phrase at the
//! START of an utterance and the remainder of the utterance is routed through
//! the command-edit provider path with the workflow's instruction, then sent to
//! its destination (paste / clipboard / note). Mirrors the snippet store's
//! persistence shape exactly; unlike snippets, entries are versioned and every
//! edit archives the prior revision.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where a workflow's result goes once produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDestination {
    /// Insert into the frontmost app (the normal dictation path).
    #[default]
    Paste,
    /// Copy to the clipboard only.
    Clipboard,
    /// Append to the Notes store.
    Note,
}

/// One archived prior version of a workflow's instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRevision {
    pub version: u32,
    pub instruction: String,
    pub updated_unix: u64,
}

/// One workflow: a spoken trigger prefix and the instruction the payload is
/// routed through. `version` starts at 1 and bumps on every edit; the replaced
/// revision is archived into `history`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEntry {
    pub name: String,
    pub trigger: String,
    pub instruction: String,
    #[serde(default)]
    pub destination: WorkflowDestination,
    #[serde(default)]
    pub require_approval: bool,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub updated_unix: u64,
    #[serde(default)]
    pub history: Vec<WorkflowRevision>,
}

/// The user's workflows, persisted as JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowStore {
    pub entries: Vec<WorkflowEntry>,
}

impl WorkflowStore {
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

    /// Add or update a workflow, keyed by name (case-insensitive)  -  one entry
    /// per name. Updating an existing entry archives its prior revision into
    /// `history` and bumps `version`; a new entry starts at version 1.
    pub fn add(
        &mut self,
        name: String,
        trigger: String,
        instruction: String,
        destination: WorkflowDestination,
        require_approval: bool,
        now_unix: u64,
    ) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.name.eq_ignore_ascii_case(&name))
        {
            existing.history.push(WorkflowRevision {
                version: existing.version,
                instruction: std::mem::take(&mut existing.instruction),
                updated_unix: existing.updated_unix,
            });
            existing.trigger = trigger;
            existing.instruction = instruction;
            existing.destination = destination;
            existing.require_approval = require_approval;
            existing.version += 1;
            existing.updated_unix = now_unix;
        } else {
            self.entries.push(WorkflowEntry {
                name,
                trigger,
                instruction,
                destination,
                require_approval,
                version: 1,
                updated_unix: now_unix,
                history: Vec::new(),
            });
        }
    }

    /// Remove an entry by its name (case-insensitive). Returns true if removed.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| !e.name.eq_ignore_ascii_case(name));
        self.entries.len() != before
    }

    /// Find the workflow whose trigger prefixes `raw_transcript`, returning the
    /// entry plus the payload (the utterance after the trigger). Matching is
    /// case-insensitive, anchored at the utterance start (after leading
    /// whitespace), and requires a whole-word boundary after the trigger  -  so
    /// trigger "log" matches "log lunch" but not "logging in". A trigger-only
    /// utterance, including with a trailing ASR '.'/'!'/'?', matches with an
    /// empty payload. When more than one trigger matches, the longest wins.
    pub fn find_match(&self, raw_transcript: &str) -> Option<(&WorkflowEntry, String)> {
        let trimmed = raw_transcript.trim();
        let chars: Vec<char> = trimmed.chars().collect();

        let mut best: Option<(&WorkflowEntry, String)> = None;
        for e in &self.entries {
            let Some(payload) = match_prefix(&chars, e.trigger.trim()) else {
                continue;
            };
            let is_longer = best
                .as_ref()
                .map(|(b, _)| e.trigger.chars().count() > b.trigger.chars().count())
                .unwrap_or(true);
            if is_longer {
                best = Some((e, payload));
            }
        }
        best
    }
}

/// If `trigger` prefixes `chars` (case-insensitive, whole-word boundary after
/// it), return the payload that follows; otherwise `None`. The payload is
/// trimmed, a leading ASR comma/colon/semicolon after the trigger is dropped,
/// and a remainder that is only sentence terminators ("." from ASR) becomes
/// the empty payload.
fn match_prefix(chars: &[char], trigger: &str) -> Option<String> {
    let t: Vec<char> = trigger.chars().collect();
    let plen = t.len();
    if plen == 0 || plen > chars.len() {
        return None;
    }
    // Full-Unicode case-insensitive: whisper sentence-cases transcripts, so an
    // accented trigger like "ecris" with a leading 'e'-acute must still match
    // its capitalized form. `char::to_lowercase` yields an iterator (a mapping
    // can be more than one char), so compare the iterators.
    let matches =
        (0..plen).all(|k| chars[k] == t[k] || chars[k].to_lowercase().eq(t[k].to_lowercase()));
    if !matches {
        return None;
    }
    let boundary_after = plen == chars.len() || !chars[plen].is_alphanumeric();
    if !boundary_after {
        return None;
    }
    let rest: String = chars[plen..].iter().collect();
    let payload = rest.trim().trim_start_matches([',', ':', ';']).trim_start();
    if payload.chars().all(|c| matches!(c, '.' | '!' | '?')) {
        return Some(String::new());
    }
    Some(payload.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> WorkflowStore {
        let mut s = WorkflowStore::default();
        s.add(
            "Jira".into(),
            "jira it".into(),
            "Turn this into a Jira ticket.".into(),
            WorkflowDestination::Clipboard,
            false,
            1_000,
        );
        s.add(
            "Log".into(),
            "log".into(),
            "Append as a dated log line.".into(),
            WorkflowDestination::Note,
            true,
            1_000,
        );
        s
    }

    #[test]
    fn trigger_matches_at_utterance_start_with_payload() {
        let s = store();
        let (e, payload) = s
            .find_match("jira it the login page crashes on submit")
            .expect("should match");
        assert_eq!(e.name, "Jira");
        assert_eq!(payload, "the login page crashes on submit");
    }

    #[test]
    fn trigger_matching_is_case_insensitive_and_tolerates_whitespace() {
        let s = store();
        let (e, payload) = s
            .find_match("  JIRA IT fix the header  ")
            .expect("should match");
        assert_eq!(e.name, "Jira");
        assert_eq!(payload, "fix the header");
    }

    #[test]
    fn trigger_matching_is_unicode_case_insensitive() {
        let mut s = store();
        // Trigger "ecris note" with a leading e-acute (U+00E9).
        s.add(
            "Note".into(),
            "\u{e9}cris note".into(),
            "Append to the note.".into(),
            WorkflowDestination::Note,
            false,
            1_000,
        );
        // Whisper sentence-cases the utterance: leading E-acute (U+00C9),
        // which an ASCII-only comparison treats as a different character.
        let (e, payload) = s
            .find_match("\u{c9}cris note acheter du lait.")
            .expect("accented trigger should match its capitalized form");
        assert_eq!(e.name, "Note");
        assert_eq!(payload, "acheter du lait.");
    }

    #[test]
    fn trigger_only_utterance_with_trailing_asr_period_gives_empty_payload() {
        let s = store();
        let (e, payload) = s.find_match("Jira it.").expect("should match");
        assert_eq!(e.name, "Jira");
        assert_eq!(payload, "");

        let (_, payload2) = s.find_match("log").expect("bare trigger matches too");
        assert_eq!(payload2, "");
    }

    #[test]
    fn trigger_requires_whole_word_boundary() {
        let s = store();
        // "log" must not fire inside "logging".
        assert!(s.find_match("logging in is broken").is_none());
        // Mid-utterance mention is not a trigger  -  match is start-anchored.
        assert!(s.find_match("please jira it later").is_none());
    }

    #[test]
    fn longest_trigger_wins_on_overlap() {
        let mut s = store();
        s.add(
            "LogLunch".into(),
            "log lunch".into(),
            "Log a lunch entry.".into(),
            WorkflowDestination::Note,
            false,
            1_000,
        );
        let (e, payload) = s.find_match("log lunch tacos").expect("should match");
        assert_eq!(e.name, "LogLunch");
        assert_eq!(payload, "tacos");
    }

    #[test]
    fn add_upserts_by_name_bumping_version_and_archiving_prior_revision() {
        let mut s = store();
        s.add(
            "jira".into(), // case-insensitive upsert key
            "file it".into(),
            "Turn this into a DETAILED Jira ticket.".into(),
            WorkflowDestination::Paste,
            true,
            2_000,
        );
        assert_eq!(
            s.entries
                .iter()
                .filter(|e| e.name.eq_ignore_ascii_case("jira"))
                .count(),
            1
        );
        let e = s
            .entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case("jira"))
            .unwrap();
        assert_eq!(e.version, 2);
        assert_eq!(e.trigger, "file it");
        assert_eq!(e.instruction, "Turn this into a DETAILED Jira ticket.");
        assert_eq!(e.destination, WorkflowDestination::Paste);
        assert!(e.require_approval);
        assert_eq!(e.updated_unix, 2_000);
        // The prior revision is archived intact.
        assert_eq!(e.history.len(), 1);
        assert_eq!(e.history[0].version, 1);
        assert_eq!(e.history[0].instruction, "Turn this into a Jira ticket.");
        assert_eq!(e.history[0].updated_unix, 1_000);
    }

    #[test]
    fn remove_deletes_case_insensitively() {
        let mut s = store();
        assert!(s.remove("JIRA"));
        assert!(s.find_match("jira it something").is_none());
        assert!(!s.remove("jira"), "second removal finds nothing left");
    }

    #[test]
    fn destination_serializes_snake_case_and_history_defaults_on_old_json() {
        let json = serde_json::to_string(&WorkflowDestination::Clipboard).unwrap();
        assert_eq!(json, "\"clipboard\"");

        // An entry persisted without the newer optional fields still loads.
        let entry_json = r#"{
            "name": "Jira",
            "trigger": "jira it",
            "instruction": "Turn this into a Jira ticket."
        }"#;
        let e: WorkflowEntry = serde_json::from_str(entry_json).unwrap();
        assert_eq!(e.destination, WorkflowDestination::Paste);
        assert!(!e.require_approval);
        assert!(e.history.is_empty());
    }
}
