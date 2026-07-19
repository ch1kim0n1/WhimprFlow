//! Dictation usage stats  -  words dictated, speaking time, words-per-minute,
//! streaks, and estimated time saved vs typing. One small record is appended per
//! completed dictation and persisted as JSON (same dependency-light pattern as
//! [`crate::settings`] and [`crate::dictionary`]).
//!
//! All "today"/"streak" bucketing is done against a timezone offset the UI passes
//! in (minutes to add to local time to reach UTC, i.e. JS `getTimezoneOffset()`),
//! so day boundaries line up with the user's own clock without pulling in a
//! timezone crate.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Average typing speed (words/min) we compare speaking against for "time saved".
/// 45 wpm matches Wispr Flow's own typed baseline (they cite 45 typed vs ~220 spoken).
const TYPING_WPM_BASELINE: f64 = 45.0;

const DAY_SECS: i64 = 86_400;

/// Where a dictation's text came from and where it went: which ASR engine
/// transcribed it, which cleanup route shaped it, whether any text left the
/// machine, and what the deterministic gate decided. Shown as the provenance
/// badge in history and the per-dictation Privacy ledger.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// ASR engine + model, e.g. "whisper.cpp:ggml-base.en.bin".
    #[serde(default)]
    pub asr_engine: String,
    /// Cleanup route: "raw" | "local" | "openai:<model>" | "anthropic:<model>"
    /// | "snippet" | "workflow:<name>".
    #[serde(default)]
    pub cleanup: String,
    /// True when any part of this dictation was sent to a cloud provider.
    #[serde(default)]
    pub sent_to_cloud: bool,
    /// Gate verdict: "passed" | "rejected" | "skipped".
    #[serde(default)]
    pub gate: String,
}

/// One completed dictation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    /// Seconds since the Unix epoch (UTC) when the dictation was committed.
    pub ts_unix: u64,
    /// Word count of the final inserted text.
    pub words: u32,
    /// Speaking duration in milliseconds.
    pub duration_ms: u32,
    /// Character count of the final inserted text.
    pub chars: u32,
    /// The cleaned/inserted text (for the Home history list). Older records may
    /// predate this field.
    #[serde(default)]
    pub text: String,
    /// Bundle id of the app the text was inserted into, if known.
    #[serde(default)]
    pub app: Option<String>,
    /// The raw (pre-cleanup) transcript, for the raw-vs-final diff and
    /// restore-raw. Older records predate this field.
    #[serde(default)]
    pub raw: String,
    /// Where this dictation's text came from and went (Privacy ledger).
    #[serde(default)]
    pub provenance: Provenance,
    /// Mean ASR token probability for this dictation, when the engine reports one.
    #[serde(default)]
    pub confidence: Option<f32>,
    /// Words whose ASR token probability averaged low (likely mishears).
    #[serde(default)]
    pub low_words: Vec<String>,
}

/// A history row for the Hub Home list (newest first). Trimmed view of a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub ts_unix: u64,
    pub text: String,
    pub app: Option<String>,
    pub words: u32,
    /// Raw (pre-cleanup) transcript, for the raw-vs-final diff view.
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub provenance: Provenance,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub low_words: Vec<String>,
}

impl From<&SessionRecord> for HistoryItem {
    fn from(s: &SessionRecord) -> Self {
        HistoryItem {
            ts_unix: s.ts_unix,
            text: s.text.clone(),
            app: s.app.clone(),
            words: s.words,
            raw: s.raw.clone(),
            provenance: s.provenance.clone(),
            confidence: s.confidence,
            low_words: s.low_words.clone(),
        }
    }
}

/// The persisted stats log.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatsStore {
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
}

/// Aggregated stats for the Hub. Everything the UI needs to draw the dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsSummary {
    pub total_words: u64,
    pub total_sessions: u64,
    pub total_speaking_secs: f64,
    /// Lifetime average speaking speed (words/min).
    pub avg_wpm: u32,
    /// Fastest single dictation (words/min), ignoring trivially short ones.
    pub best_wpm: u32,
    pub words_today: u64,
    pub wpm_today: u32,
    /// Consecutive days (up to today) with at least one dictation.
    pub day_streak: u32,
    /// Estimated seconds saved vs typing the same words at [`TYPING_WPM_BASELINE`].
    pub time_saved_secs: f64,
    /// Words per local day, oldest first; index 6 is today, 0 is six days ago.
    pub last7_words: [u64; 7],
}

/// Count whitespace-delimited words. Matches how the cleanup layer thinks of words.
pub fn count_words(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

/// The local calendar day index for a UTC timestamp, given the UI's tz offset
/// (minutes to add to local to get UTC, per JS `Date.getTimezoneOffset()`).
fn local_day(ts_unix: u64, tz_offset_minutes: i32) -> i64 {
    let local = ts_unix as i64 - (tz_offset_minutes as i64) * 60;
    local.div_euclid(DAY_SECS)
}

/// Words/min from words and a duration, rounded; 0 for empty/instant sessions.
fn wpm(words: u64, secs: f64) -> u32 {
    if secs <= 0.0 || words == 0 {
        return 0;
    }
    (words as f64 / (secs / 60.0)).round() as u32
}

impl StatsStore {
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

    /// Append one completed dictation with full provenance. This is the primary
    /// entry point; the older field-by-field [`StatsStore::record`] delegates here.
    pub fn record_full(&mut self, record: SessionRecord) {
        self.sessions.push(record);
    }

    /// Append one completed dictation (legacy signature). Fills the newer
    /// raw/provenance/confidence fields with their defaults.
    pub fn record(
        &mut self,
        words: u32,
        duration_ms: u32,
        chars: u32,
        ts_unix: u64,
        text: String,
        app: Option<String>,
    ) {
        self.record_full(SessionRecord {
            ts_unix,
            words,
            duration_ms,
            chars,
            text,
            app,
            raw: String::new(),
            provenance: Provenance::default(),
            confidence: None,
            low_words: Vec::new(),
        });
    }

    /// The most recent `limit` dictations, newest first, for the Home history
    /// list. Skips records whose text is gone (pruned, cleared, or never
    /// stored); the Privacy pane's ledger uses [`StatsStore::ledger`] instead.
    pub fn history(&self, limit: usize) -> Vec<HistoryItem> {
        self.sessions
            .iter()
            .rev()
            .filter(|s| !s.text.is_empty())
            .take(limit)
            .map(HistoryItem::from)
            .collect()
    }

    /// The most recent `limit` records, newest first, INCLUDING textless ones.
    /// Same rows as [`StatsStore::history`] minus the text filter: the Privacy
    /// dictation ledger audits every record (provenance, cloud flag, gate),
    /// even after its text was pruned or was never stored.
    pub fn ledger(&self, limit: usize) -> Vec<HistoryItem> {
        self.sessions
            .iter()
            .rev()
            .take(limit)
            .map(HistoryItem::from)
            .collect()
    }

    /// Retention pruning: strip the stored dictation content (`text`, `raw`,
    /// and the `low_words` mishear list) from every record older than
    /// `retention_days` before `now_unix`, keeping the numeric stats (words,
    /// duration, timestamps) so the dashboard history is unchanged. Returns how
    /// many records were stripped. `retention_days == 0` strips everything
    /// before `now_unix`.
    pub fn prune_texts(&mut self, now_unix: u64, retention_days: u32) -> usize {
        let cutoff = now_unix.saturating_sub(retention_days as u64 * DAY_SECS as u64);
        let mut pruned = 0usize;
        for s in &mut self.sessions {
            if s.ts_unix < cutoff
                && (!s.text.is_empty() || !s.raw.is_empty() || !s.low_words.is_empty())
            {
                s.text.clear();
                s.raw.clear();
                s.low_words.clear();
                pruned += 1;
            }
        }
        pruned
    }

    /// Delete-all-text: strip the stored dictation content (`text`, `raw`, and
    /// the `low_words` mishear list) from every record regardless of age,
    /// keeping the numeric stats. Returns how many records were stripped.
    pub fn clear_texts(&mut self) -> usize {
        let mut cleared = 0usize;
        for s in &mut self.sessions {
            if !s.text.is_empty() || !s.raw.is_empty() || !s.low_words.is_empty() {
                s.text.clear();
                s.raw.clear();
                s.low_words.clear();
                cleared += 1;
            }
        }
        cleared
    }

    /// Aggregate everything the dashboard shows. `now_unix` and `tz_offset_minutes`
    /// come from the caller so day math matches the user's local clock (and so the
    /// aggregation stays pure/testable).
    pub fn summary(&self, tz_offset_minutes: i32, now_unix: u64) -> StatsSummary {
        let total_words: u64 = self.sessions.iter().map(|s| s.words as u64).sum();
        let total_sessions = self.sessions.len() as u64;
        let total_speaking_secs: f64 = self
            .sessions
            .iter()
            .map(|s| s.duration_ms as f64 / 1000.0)
            .sum();

        let avg_wpm = wpm(total_words, total_speaking_secs);

        // Best WPM, ignoring blips that inflate the number (need real words + time).
        let best_wpm = self
            .sessions
            .iter()
            .filter(|s| s.words >= 3 && s.duration_ms >= 1000)
            .map(|s| wpm(s.words as u64, s.duration_ms as f64 / 1000.0))
            .max()
            .unwrap_or(0);

        let today = local_day(now_unix, tz_offset_minutes);

        let mut words_today: u64 = 0;
        let mut secs_today: f64 = 0.0;
        let mut last7_words = [0u64; 7];
        for s in &self.sessions {
            let day = local_day(s.ts_unix, tz_offset_minutes);
            if day == today {
                words_today += s.words as u64;
                secs_today += s.duration_ms as f64 / 1000.0;
            }
            let ago = today - day; // 0 = today, 6 = six days ago
            if (0..7).contains(&ago) {
                last7_words[(6 - ago) as usize] += s.words as u64;
            }
        }
        let wpm_today = wpm(words_today, secs_today);

        // Streak: consecutive days with activity, up to today. A day with no
        // dictations yet doesn't break the streak until it's fully past, so if
        // today is still empty we start counting from yesterday.
        use std::collections::HashSet;
        let active: HashSet<i64> = self
            .sessions
            .iter()
            .map(|s| local_day(s.ts_unix, tz_offset_minutes))
            .collect();
        let mut day_streak = 0u32;
        let mut d = if active.contains(&today) {
            today
        } else {
            today - 1
        };
        while active.contains(&d) {
            day_streak += 1;
            d -= 1;
        }

        // Time saved: how long these words would take to type at the baseline,
        // minus the time actually spent speaking. Never negative.
        let typed_secs = total_words as f64 / TYPING_WPM_BASELINE * 60.0;
        let time_saved_secs = (typed_secs - total_speaking_secs).max(0.0);

        StatsSummary {
            total_words,
            total_sessions,
            total_speaking_secs,
            avg_wpm,
            best_wpm,
            words_today,
            wpm_today,
            day_streak,
            time_saved_secs,
            last7_words,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fixed "now": 2021-01-08 12:00:00 UTC.
    const NOW: u64 = 1_610_107_200;
    const DAY: u64 = 86_400;

    #[test]
    fn counts_words() {
        assert_eq!(count_words("  hello   there  world "), 3);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn aggregates_totals_and_wpm() {
        let mut s = StatsStore::default();
        // 60 words in 60s -> 60 wpm.
        s.record(60, 60_000, 300, NOW, String::new(), None);
        // 30 words in 15s -> 120 wpm.
        s.record(30, 15_000, 150, NOW, String::new(), None);
        let sum = s.summary(0, NOW);
        assert_eq!(sum.total_words, 90);
        assert_eq!(sum.total_sessions, 2);
        // 90 words / 75s = 72 wpm.
        assert_eq!(sum.avg_wpm, 72);
        assert_eq!(sum.best_wpm, 120);
        assert_eq!(sum.words_today, 90);
    }

    #[test]
    fn streak_counts_consecutive_days_including_gap_today() {
        let mut s = StatsStore::default();
        // Activity yesterday, day-before, and three days ago (but NOT today).
        s.record(10, 5_000, 50, NOW - DAY, String::new(), None);
        s.record(10, 5_000, 50, NOW - 2 * DAY, String::new(), None);
        s.record(10, 5_000, 50, NOW - 3 * DAY, String::new(), None);
        // Gap at 4 days ago, then one more.
        s.record(10, 5_000, 50, NOW - 5 * DAY, String::new(), None);
        let sum = s.summary(0, NOW);
        // Today empty -> start at yesterday; 3 consecutive days back, then a gap.
        assert_eq!(sum.day_streak, 3);
        assert_eq!(sum.words_today, 0);
    }

    #[test]
    fn last7_buckets_by_local_day() {
        let mut s = StatsStore::default();
        s.record(5, 3_000, 25, NOW, String::new(), None); // today
        s.record(7, 3_000, 35, NOW - 2 * DAY, String::new(), None); // 2 days ago
        let sum = s.summary(0, NOW);
        assert_eq!(sum.last7_words[6], 5); // today
        assert_eq!(sum.last7_words[4], 7); // two days ago
        assert_eq!(sum.last7_words[5], 0);
    }

    /// A record with all the new fields populated, `days_ago` days before NOW.
    fn full_record(days_ago: u64, text: &str, raw: &str) -> SessionRecord {
        SessionRecord {
            ts_unix: NOW - days_ago * DAY,
            words: 10,
            duration_ms: 5_000,
            chars: 50,
            text: text.to_string(),
            app: Some("com.example.app".to_string()),
            raw: raw.to_string(),
            provenance: Provenance {
                asr_engine: "whisper.cpp:ggml-base.en.bin".to_string(),
                cleanup: "local".to_string(),
                sent_to_cloud: false,
                gate: "passed".to_string(),
            },
            confidence: Some(0.92),
            low_words: vec!["whimpr".to_string()],
        }
    }

    #[test]
    fn record_full_flows_through_history_with_provenance() {
        let mut s = StatsStore::default();
        s.record_full(full_record(0, "Hello there.", "um hello there"));
        let items = s.history(10);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].raw, "um hello there");
        assert_eq!(items[0].provenance.cleanup, "local");
        assert_eq!(items[0].confidence, Some(0.92));
        assert_eq!(items[0].low_words, vec!["whimpr".to_string()]);
    }

    #[test]
    fn prune_texts_strips_old_text_and_raw_but_keeps_numbers() {
        let mut s = StatsStore::default();
        s.record_full(full_record(10, "old text", "old raw")); // past cutoff
        s.record_full(full_record(1, "new text", "new raw")); // inside window
        let pruned = s.prune_texts(NOW, 7);
        assert_eq!(pruned, 1);
        assert_eq!(s.sessions[0].text, "");
        assert_eq!(s.sessions[0].raw, "");
        // low_words are dictated words too - retention removes them alongside.
        assert!(s.sessions[0].low_words.is_empty());
        // Numeric stats and provenance survive the prune.
        assert_eq!(s.sessions[0].words, 10);
        assert_eq!(s.sessions[0].duration_ms, 5_000);
        assert_eq!(s.sessions[0].provenance.cleanup, "local");
        // The recent record is untouched.
        assert_eq!(s.sessions[1].text, "new text");
        assert_eq!(s.sessions[1].raw, "new raw");
        assert_eq!(s.sessions[1].low_words, vec!["whimpr".to_string()]);
        // Second pass finds nothing left to strip.
        assert_eq!(s.prune_texts(NOW, 7), 0);
        // Summary totals are unchanged by pruning.
        assert_eq!(s.summary(0, NOW).total_words, 20);
    }

    #[test]
    fn prune_texts_with_zero_days_strips_everything_before_now() {
        let mut s = StatsStore::default();
        s.record_full(full_record(0, "today", "today raw"));
        s.record_full(full_record(1, "yesterday", "yesterday raw"));
        // retention_days = 0: never store text -> everything before now goes.
        let pruned = s.prune_texts(NOW + 1, 0);
        assert_eq!(pruned, 2);
        assert!(s
            .sessions
            .iter()
            .all(|r| r.text.is_empty() && r.raw.is_empty() && r.low_words.is_empty()));
    }

    #[test]
    fn prune_texts_strips_low_words_even_when_text_already_gone() {
        let mut s = StatsStore::default();
        // A record whose text/raw were already stripped but whose low_words
        // (actual dictated words) linger - e.g. written before low_words was
        // covered by pruning.
        let mut r = full_record(10, "", "");
        r.low_words = vec!["Manvi".to_string(), "acetaminophen.".to_string()];
        s.record_full(r);
        assert_eq!(s.prune_texts(NOW, 7), 1);
        assert!(s.sessions[0].low_words.is_empty());
        assert_eq!(s.prune_texts(NOW, 7), 0);
    }

    #[test]
    fn clear_texts_strips_all_records_and_reports_count() {
        let mut s = StatsStore::default();
        s.record_full(full_record(0, "a", ""));
        s.record_full(full_record(3, "", "b raw"));
        let mut low_only = full_record(5, "", "");
        low_only.low_words = vec!["mishear".to_string()]; // dictated words linger
        s.record_full(low_only);
        s.record(10, 5_000, 50, NOW, String::new(), None); // already textless
        assert_eq!(s.clear_texts(), 3);
        assert!(s
            .sessions
            .iter()
            .all(|r| r.text.is_empty() && r.raw.is_empty() && r.low_words.is_empty()));
        assert_eq!(s.clear_texts(), 0);
        assert_eq!(s.summary(0, NOW).total_sessions, 4);
    }

    #[test]
    fn ledger_includes_textless_records_history_skips_them() {
        let mut s = StatsStore::default();
        s.record_full(full_record(2, "kept text", "kept raw"));
        s.record(10, 5_000, 50, NOW - DAY, String::new(), None); // textless
        s.record_full(full_record(0, "newest", "newest raw"));

        // History (Home list) hides the textless record.
        let history = s.history(10);
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|i| !i.text.is_empty()));

        // The Privacy ledger audits every record, newest first.
        let ledger = s.ledger(10);
        assert_eq!(ledger.len(), 3);
        assert_eq!(ledger[0].text, "newest");
        assert_eq!(ledger[1].text, "");
        assert_eq!(ledger[1].words, 10);
        assert_eq!(ledger[2].text, "kept text");
        assert_eq!(ledger[2].provenance.cleanup, "local");

        // Limits: ledger counts textless rows toward the limit; history only
        // counts rows it shows.
        assert_eq!(s.ledger(2).len(), 2);
        assert_eq!(s.history(2).len(), 2);
        assert_eq!(s.history(2)[1].text, "kept text");
    }

    #[test]
    fn old_stats_json_without_new_fields_still_loads() {
        // Back-compat: a stats.json written before raw/provenance/confidence/
        // low_words existed must still parse, with the new fields defaulting.
        let json = r#"{
            "sessions": [
                {
                    "ts_unix": 1610107200,
                    "words": 12,
                    "duration_ms": 6000,
                    "chars": 60,
                    "text": "hello world",
                    "app": "com.apple.Notes"
                },
                {
                    "ts_unix": 1610020800,
                    "words": 4,
                    "duration_ms": 2000,
                    "chars": 20
                }
            ]
        }"#;
        let s: StatsStore = serde_json::from_str(json).unwrap();
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.sessions[0].text, "hello world");
        assert_eq!(s.sessions[0].raw, "");
        assert_eq!(s.sessions[0].provenance, Provenance::default());
        assert_eq!(s.sessions[0].confidence, None);
        assert!(s.sessions[0].low_words.is_empty());
        // Records that predate even `text`/`app` keep loading too.
        assert_eq!(s.sessions[1].text, "");
        assert_eq!(s.sessions[1].app, None);
    }
}
