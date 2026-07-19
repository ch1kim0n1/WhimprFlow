//! Static text snippets: voice-triggered phrase expansion. Say a trigger phrase
//! (either as the whole utterance or as a standalone phrase within it) and it
//! expands to canned text  -  no LLM involved. Mirrors the dictionary store's
//! persistence shape exactly.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// One snippet entry: a spoken trigger phrase and the text it expands to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetEntry {
    pub trigger: String,
    pub expansion: String,
}

/// The user's snippets, persisted as JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnippetStore {
    pub entries: Vec<SnippetEntry>,
}

impl SnippetStore {
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

    /// Add an entry, de-duplicating by trigger (case-insensitive)  -  one rule per
    /// trigger, so re-adding an existing trigger replaces its expansion.
    pub fn add(&mut self, trigger: String, expansion: String) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.trigger.eq_ignore_ascii_case(&trigger))
        {
            existing.expansion = expansion;
        } else {
            self.entries.push(SnippetEntry { trigger, expansion });
        }
    }

    /// Remove an entry by its trigger (case-insensitive). Returns true if removed.
    pub fn remove(&mut self, trigger: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|e| !e.trigger.eq_ignore_ascii_case(trigger));
        self.entries.len() != before
    }

    /// Find the snippet whose trigger matches `raw_transcript`. Case-insensitive.
    /// Matches when either: the entire utterance (trimmed, with a trailing ASR
    /// '.'/'!'/'?' stripped) equals the trigger exactly; or the trigger occurs as a
    /// standalone whole-word run inside the utterance, with no adjacent
    /// alphanumeric character on either side (same boundary style as
    /// `cleanup::replace_cues`). When more than one entry matches, the longest
    /// trigger wins.
    pub fn find_match(&self, raw_transcript: &str) -> Option<&SnippetEntry> {
        let trimmed = raw_transcript.trim();
        let whole = trimmed.trim_end_matches(['.', '!', '?']);

        let mut best: Option<&SnippetEntry> = None;
        for e in &self.entries {
            let is_match =
                whole.eq_ignore_ascii_case(&e.trigger) || contains_whole_word(trimmed, &e.trigger);
            if is_match {
                let is_longer = best
                    .map(|b| e.trigger.chars().count() > b.trigger.chars().count())
                    .unwrap_or(true);
                if is_longer {
                    best = Some(e);
                }
            }
        }
        best
    }
}

/// Whether `phrase` occurs in `input` as a standalone whole-word run: matched
/// case-insensitively, bounded on both sides by either the string edge or a
/// non-alphanumeric character. Mirrors the boundary logic in
/// `cleanup::replace_cues`.
fn contains_whole_word(input: &str, phrase: &str) -> bool {
    let chars: Vec<char> = input.chars().collect();
    let p: Vec<char> = phrase.chars().collect();
    let n = chars.len();
    let plen = p.len();
    if plen == 0 || plen > n {
        return false;
    }
    for i in 0..=(n - plen) {
        let boundary_before = i == 0 || !chars[i - 1].is_alphanumeric();
        if !boundary_before {
            continue;
        }
        let matches = (0..plen).all(|k| chars[i + k].eq_ignore_ascii_case(&p[k]));
        if matches {
            let boundary_after = i + plen == n || !chars[i + plen].is_alphanumeric();
            if boundary_after {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SnippetStore {
        let mut s = SnippetStore::default();
        s.add("my email".into(), "user@example.com".into());
        s.add("best regards".into(), "Best regards,\nVadim".into());
        s
    }

    #[test]
    fn whole_utterance_match_with_trailing_asr_period() {
        let s = store();
        let m = s.find_match("my email.").expect("should match");
        assert_eq!(m.trigger, "my email");
        assert_eq!(m.expansion, "user@example.com");
    }

    #[test]
    fn mid_sentence_match_requires_whole_word_boundaries() {
        let s = store();
        // Standalone phrase inside a longer utterance -> matches.
        let m = s
            .find_match("please send my email now")
            .expect("should match");
        assert_eq!(m.trigger, "my email");

        // "my email" is a substring of "my emailing" but not a whole word -> no match.
        let mut only_email = SnippetStore::default();
        only_email.add("email".into(), "e-mail".into());
        assert!(
            only_email.find_match("check the emailing list").is_none(),
            "trigger must not match as a substring of a longer word"
        );
    }

    #[test]
    fn no_match_returns_none() {
        let s = store();
        assert!(s.find_match("the weather is nice today").is_none());
    }

    #[test]
    fn case_insensitive_trigger_matching() {
        let s = store();
        let m = s
            .find_match("MY EMAIL")
            .expect("should match case-insensitively");
        assert_eq!(m.trigger, "my email");

        let m2 = s
            .find_match("Please send Best Regards to the client")
            .expect("should match");
        assert_eq!(m2.trigger, "best regards");
    }

    #[test]
    fn longest_trigger_wins_on_overlap() {
        let mut s = SnippetStore::default();
        s.add("address".into(), "short".into());
        s.add("my address".into(), "long".into());
        let m = s
            .find_match("please send my address now")
            .expect("should match");
        assert_eq!(m.trigger, "my address");
    }

    #[test]
    fn add_dedupes_case_insensitively_and_replaces_expansion() {
        let mut s = store();
        s.add("My Email".into(), "new@example.com".into());
        assert_eq!(
            s.entries
                .iter()
                .filter(|e| e.trigger.eq_ignore_ascii_case("my email"))
                .count(),
            1
        );
        let e = s
            .entries
            .iter()
            .find(|e| e.trigger.eq_ignore_ascii_case("my email"))
            .unwrap();
        assert_eq!(e.expansion, "new@example.com");
    }

    #[test]
    fn remove_deletes_case_insensitively() {
        let mut s = store();
        assert!(s.remove("MY EMAIL"));
        assert!(s.find_match("my email").is_none());
        assert!(!s.remove("my email"), "second removal finds nothing left");
    }
}
