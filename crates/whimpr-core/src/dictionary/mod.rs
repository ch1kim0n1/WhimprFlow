//! Custom dictionary: user vocabulary plus a pre-filter that injects only the
//! entries relevant to a given utterance into the cleanup prompt (fewer distractors
//! → higher LLM precision). Manual entries and auto-learned (✨) entries share the
//! same store; the auto-learn diff engine (needs accessibility reads) layers on top.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use rphonetic::DoubleMetaphone;
use serde::{Deserialize, Serialize};

use crate::cleanup::VocabEntry;

/// Top ~5000 English words by frequency, used as the "common word" gate for
/// auto-learn dictionary filtering (Gate 1 in
/// `docs/research/gap-auto-learned-dictionary.md`). A word that does NOT appear
/// here is treated as distinctive (proper noun / brand / technical term) and is a
/// candidate for auto-learning; a word that DOES appear here is filtered out to
/// avoid poisoning the dictionary with ordinary edits.
///
/// Source (fetched verbatim, first 5000 lines, lowercased, one word per line):
/// <https://raw.githubusercontent.com/first20hours/google-10000-english/master/google-10000-english-no-swears.txt>
/// (Google's 10,000-most-common-English-words list, "no swears" variant, derived
/// from the Google Web Trillion Word Corpus; public-domain word frequency data).
static COMMON_WORDS_RAW: &str = include_str!("common_words.txt");

/// Lazily-built lookup set over [`COMMON_WORDS_RAW`].
static COMMON_WORDS: OnceLock<HashSet<&'static str>> = OnceLock::new();

/// True if `word` is one of the ~5000 most common English words (case-insensitive).
/// Used as the primary "is this word too common to auto-learn?" gate.
pub fn is_common_word(word: &str) -> bool {
    let set =
        COMMON_WORDS.get_or_init(|| COMMON_WORDS_RAW.lines().filter(|l| !l.is_empty()).collect());
    let lc = word.to_lowercase();
    set.contains(lc.as_str())
}

/// Compute the Double Metaphone primary + alternate codes for `word` (uppercase
/// consonant-skeleton strings, e.g. "jumped" -> ("JMPT", "AMPT")). Empty strings
/// are returned for empty/non-alphabetic input rather than a meaningless match.
///
/// Algorithm: Double Metaphone (Lawrence Philips, 2000) via the `rphonetic` crate  -
/// chosen over Soundex because it emits primary+alternate codes and is
/// substantially better at English proper nouns/names (see
/// `docs/research/gap-auto-learned-dictionary.md` §6).
pub fn phonetic_codes(word: &str) -> (String, String) {
    if word.trim().is_empty() {
        return (String::new(), String::new());
    }
    let dm = DoubleMetaphone::default();
    let result = dm.double_metaphone(word);
    (result.primary(), result.alternate())
}

/// True if `a` and `b` are likely to sound alike: either word's primary or
/// alternate Double Metaphone code matches either code of the other. Empty codes
/// never match (avoids two empty/degenerate encodings being treated as "close").
pub fn phonetic_match(a: &str, b: &str) -> bool {
    let (ap, aa) = phonetic_codes(a);
    let (bp, ba) = phonetic_codes(b);
    if (ap.is_empty() && aa.is_empty()) || (bp.is_empty() && ba.is_empty()) {
        return false;
    }
    (!ap.is_empty() && (ap == bp || ap == ba)) || (!aa.is_empty() && (aa == bp || aa == ba))
}

/// How a dictionary entry was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictSource {
    Manual,
    Auto,
}

fn default_source() -> DictSource {
    DictSource::Manual
}

/// One vocabulary entry: the authoritative spelling and known mishears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub correct: String,
    #[serde(default)]
    pub mishears: Vec<String>,
    #[serde(default = "default_source")]
    pub source: DictSource,
    /// Cached Double Metaphone primary code for `correct`, computed once when the
    /// entry is added (or backfilled on load for entries persisted before this
    /// field existed). Empty until computed. `#[serde(default)]` so pre-existing
    /// dictionary JSON files without this field still load.
    #[serde(default)]
    pub phonetic_primary: String,
    /// Cached Double Metaphone alternate code for `correct`. See
    /// [`DictionaryEntry::phonetic_primary`].
    #[serde(default)]
    pub phonetic_alternate: String,
}

impl DictionaryEntry {
    /// Populate `phonetic_primary`/`phonetic_alternate` from `correct` if not
    /// already cached. Idempotent  -  safe to call on every load.
    fn ensure_phonetic(&mut self) {
        if self.phonetic_primary.is_empty() && self.phonetic_alternate.is_empty() {
            let (primary, alternate) = phonetic_codes(&self.correct);
            self.phonetic_primary = primary;
            self.phonetic_alternate = alternate;
        }
    }

    /// True if any of `codes` (a gram's own primary/alternate Double Metaphone
    /// codes) matches this entry's cached code for `correct`.
    fn phonetic_hit(&self, gram_primary: &str, gram_alternate: &str) -> bool {
        if self.phonetic_primary.is_empty() && self.phonetic_alternate.is_empty() {
            return false;
        }
        let hits = |code: &str| {
            !code.is_empty()
                && (code == self.phonetic_primary
                    || (!self.phonetic_alternate.is_empty() && code == self.phonetic_alternate))
        };
        hits(gram_primary) || hits(gram_alternate)
    }
}

/// The user's dictionary, persisted as JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DictionaryStore {
    pub entries: Vec<DictionaryEntry>,
}

impl DictionaryStore {
    /// Load from `path`, returning an empty store if missing or unreadable.
    /// Backfills phonetic codes for entries persisted before that field existed.
    pub fn load(path: &Path) -> Self {
        let mut store: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        for e in &mut store.entries {
            e.ensure_phonetic();
        }
        store
    }

    /// Persist to `path` (creating parent dirs).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, json)
    }

    /// Add or merge an entry, de-duplicating by spelling (case-insensitive).
    pub fn add(&mut self, correct: impl Into<String>, mishears: Vec<String>, source: DictSource) {
        let correct = correct.into();
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.correct.eq_ignore_ascii_case(&correct))
        {
            for m in mishears {
                if !existing.mishears.iter().any(|x| x.eq_ignore_ascii_case(&m)) {
                    existing.mishears.push(m);
                }
            }
        } else {
            let (phonetic_primary, phonetic_alternate) = phonetic_codes(&correct);
            self.entries.push(DictionaryEntry {
                correct,
                mishears,
                source,
                phonetic_primary,
                phonetic_alternate,
            });
        }
    }

    /// Remove an entry by its spelling (case-insensitive). Returns true if removed.
    pub fn remove(&mut self, correct: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|e| !e.correct.eq_ignore_ascii_case(correct));
        self.entries.len() != before
    }

    /// Select the entries relevant to `utterance`  -  those whose spelling or a known
    /// mishear is edit-close to a spoken token (or adjacent token pair, to catch
    /// split words like "charge bee" → "ChargeBee")  -  capped to `max`.
    pub fn prefilter(&self, utterance: &str, max: usize) -> Vec<VocabEntry> {
        let toks: Vec<String> = utterance
            .split_whitespace()
            .map(|t| {
                t.trim_matches(|c: char| c.is_ascii_punctuation())
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty())
            .collect();

        let mut grams: Vec<String> = toks.clone();
        for w in toks.windows(2) {
            grams.push(format!("{}{}", w[0], w[1]));
        }

        // Precompute each gram's Double Metaphone codes once, up front  -  a cheap
        // first-pass filter checked against every entry's *cached* `correct`-word
        // codes before falling back to the fuller edit-distance scan over
        // correct + mishears (see docs/research/gap-auto-learned-dictionary.md §2/§3).
        let gram_codes: Vec<(String, String)> = grams.iter().map(|g| phonetic_codes(g)).collect();

        let mut out = Vec::new();
        for e in &self.entries {
            let phonetic_pass = gram_codes.iter().any(|(gp, ga)| e.phonetic_hit(gp, ga));

            let matched = phonetic_pass || {
                let targets: Vec<String> = std::iter::once(e.correct.to_lowercase())
                    .chain(e.mishears.iter().map(|m| m.to_lowercase()))
                    .collect();
                grams.iter().any(|g| targets.iter().any(|t| close(g, t)))
            };

            if matched {
                out.push(VocabEntry {
                    correct: e.correct.clone(),
                    mishears: e.mishears.clone(),
                });
                if out.len() >= max {
                    break;
                }
            }
        }
        out
    }
}

/// Two tokens are "close" if identical, phonetically alike (Double Metaphone  -
/// catches mishears that sound right but are spelled very differently), or within
/// a normalized edit distance of 0.34. The phonetic check is layered on top of the
/// original edit-distance gate, not a replacement for it.
fn close(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if phonetic_match(a, b) {
        return true;
    }
    let maxlen = a.chars().count().max(b.chars().count());
    if maxlen == 0 {
        return false;
    }
    (strsim::levenshtein(a, b) as f32 / maxlen as f32) <= 0.34
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> DictionaryStore {
        let mut s = DictionaryStore::default();
        s.add(
            "Manvi",
            vec!["Monvi".into(), "Manvee".into()],
            DictSource::Manual,
        );
        s.add("ChargeBee", vec!["charge bee".into()], DictSource::Manual);
        s
    }

    #[test]
    fn prefilter_selects_close_mishear() {
        // "monvi" is an exact mishear of Manvi.
        let v = store().prefilter("send the deck to monvi please", 15);
        assert!(v.iter().any(|e| e.correct == "Manvi"));
        assert!(!v.iter().any(|e| e.correct == "ChargeBee"));
    }

    #[test]
    fn prefilter_catches_split_word_via_bigram() {
        // "charge bee" spoken as two words → bigram "chargebee" matches.
        let v = store().prefilter("we should renew charge bee this month", 15);
        assert!(v.iter().any(|e| e.correct == "ChargeBee"));
    }

    #[test]
    fn prefilter_ignores_unrelated_utterance() {
        let v = store().prefilter("the weather is nice today", 15);
        assert!(v.is_empty());
    }

    #[test]
    fn add_merges_mishears_case_insensitively() {
        let mut s = store();
        s.add("manvi", vec!["Manvie".into()], DictSource::Auto);
        let e = s.entries.iter().find(|e| e.correct == "Manvi").unwrap();
        assert!(e.mishears.iter().any(|m| m == "Manvie"));
        assert_eq!(
            s.entries
                .iter()
                .filter(|e| e.correct.eq_ignore_ascii_case("manvi"))
                .count(),
            1
        );
    }
}
