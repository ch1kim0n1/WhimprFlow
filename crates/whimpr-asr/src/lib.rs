//! Local speech-to-text via whisper.cpp (whisper-rs), implementing
//! [`whimpr_core::AsrEngine`]. Expects 16 kHz mono f32 samples.

use std::path::Path;

use whimpr_core::asr::{AsrCaps, AsrEngine, AsrEngineId, Transcript};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Words whose tokens average below this probability are flagged low-confidence.
const LOW_WORD_THRESHOLD: f32 = 0.55;

/// A loaded whisper model ready to transcribe utterances.
pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    /// Load a GGML/GGUF whisper model from `model_path`.
    pub fn load(model_path: &Path) -> anyhow::Result<Self> {
        let path = model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("model path is not valid UTF-8"))?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| anyhow::anyhow!("failed to load whisper model: {e}"))?;
        Ok(Self { ctx })
    }

    /// Transcribe one utterance with explicit options.
    ///
    /// `language` is a whisper language code ("en", "de", ...); `None` means
    /// auto-detect. `long_form` disables single-segment mode for audio that
    /// genuinely spans segments (meeting capture); push-to-talk clips keep it on.
    pub fn transcribe_opts(
        &self,
        pcm16k: &[f32],
        language: Option<&str>,
        long_form: bool,
    ) -> anyhow::Result<Transcript> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("whisper create_state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // whisper.cpp spells auto-detection "auto".
        params.set_language(Some(language.unwrap_or("auto")));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Push-to-talk utterances are always one short clip, not long-form audio.
        // Without this, whisper.cpp can split it into multiple internal segments
        // that repeat the same words  -  which then get concatenated below,
        // producing the sentence twice. Single-segment mode avoids that. Long-form
        // (meeting) audio genuinely spans segments, so only then allow splitting.
        params.set_single_segment(!long_form);
        params.set_no_context(true);

        state
            .full(params, pcm16k)
            .map_err(|e| anyhow::anyhow!("whisper full: {e}"))?;

        let n = state
            .full_n_segments()
            .map_err(|e| anyhow::anyhow!("whisper n_segments: {e}"))?;
        // Special tokens (timestamps, EOT, ...) have ids >= eot; they carry no
        // spoken text and would skew the confidence average.
        let eot = self.ctx.token_eot();
        let mut text = String::new();
        let mut tokens: Vec<(String, f32)> = Vec::new();
        for i in 0..n {
            if let Ok(seg) = state.full_get_segment_text(i) {
                text.push_str(&seg);
            }
            let n_tok = state.full_n_tokens(i).unwrap_or(0);
            for t in 0..n_tok {
                match state.full_get_token_id(i, t) {
                    Ok(id) if id < eot => {}
                    _ => continue,
                }
                // ponytail: lossy token text means a multi-byte char split across
                // BPE tokens surfaces as U+FFFD inside low_words; upgrade path is
                // assembling raw token bytes per word and decoding at boundaries.
                let piece = match state.full_get_token_text_lossy(i, t) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let prob = state.full_get_token_prob(i, t).unwrap_or(0.0);
                tokens.push((piece, prob));
            }
        }

        let (confidence, low_words) = aggregate_token_probs(&tokens);
        Ok(Transcript {
            text: text.trim().to_string(),
            confidence,
            low_words,
        })
    }
}

/// Aggregate whisper token probabilities into a transcript-level confidence plus
/// the low-confidence words.
///
/// `tokens` are text pieces in transcript order with their probabilities; the
/// pieces concatenate into the transcript text. Confidence is the mean
/// probability across all tokens (`None` when there are none). Words are the
/// whitespace-separated runs of the concatenated text; each word averages the
/// probability of every token that contributed an alphanumeric character to it
/// (punctuation-only tokens carry no speech evidence and must not drag a
/// confidently recognized word below the threshold), and words averaging below
/// [`LOW_WORD_THRESHOLD`] are returned in order. A word with no speech tokens
/// at all (pure punctuation) is never flagged.
fn aggregate_token_probs(tokens: &[(String, f32)]) -> (Option<f32>, Vec<String>) {
    if tokens.is_empty() {
        return (None, Vec::new());
    }
    let mean = tokens.iter().map(|(_, p)| *p).sum::<f32>() / tokens.len() as f32;

    // Rebuild the whitespace words from the pieces, tracking per-word which token
    // probabilities apply. A piece may span a word boundary (e.g. ". The"); it
    // then counts toward every word it touches, but only once per word.
    let mut words: Vec<(String, Vec<f32>)> = Vec::new();
    let mut open = false; // currently inside a word
    for (piece, prob) in tokens {
        let mut counted = false; // this piece already credited to the open word
        for ch in piece.chars() {
            if ch.is_whitespace() {
                open = false;
                counted = false;
            } else {
                if !open {
                    words.push((String::new(), Vec::new()));
                    open = true;
                }
                let word = words.last_mut().expect("word opened above");
                word.0.push(ch);
                // Only speech (alphanumeric) characters credit the token's
                // probability; punctuation stays in the word text but its
                // token probability says nothing about the word itself.
                if !counted && ch.is_alphanumeric() {
                    word.1.push(*prob);
                    counted = true;
                }
            }
        }
    }

    let low_words = words
        .into_iter()
        .filter(|(_, probs)| {
            // No speech tokens (punctuation-only word): nothing to judge, never flag.
            !probs.is_empty()
                && probs.iter().sum::<f32>() / (probs.len() as f32) < LOW_WORD_THRESHOLD
        })
        .map(|(word, _)| word)
        .collect();
    (Some(mean), low_words)
}

impl AsrEngine for WhisperEngine {
    fn id(&self) -> AsrEngineId {
        AsrEngineId::WhisperCpp
    }

    fn caps(&self) -> AsrCaps {
        AsrCaps {
            supports_streaming: false,
        }
    }

    fn transcribe(&self, pcm16k: &[f32]) -> anyhow::Result<Transcript> {
        // Historical default: English, short push-to-talk clip.
        self.transcribe_opts(pcm16k, Some("en"), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(pieces: &[(&str, f32)]) -> Vec<(String, f32)> {
        pieces.iter().map(|(t, p)| (t.to_string(), *p)).collect()
    }

    #[test]
    fn no_tokens_means_no_confidence() {
        assert_eq!(aggregate_token_probs(&[]), (None, Vec::new()));
    }

    #[test]
    fn confidence_is_mean_over_all_tokens() {
        let (conf, low) = aggregate_token_probs(&toks(&[(" hi", 0.8), (" there", 0.6)]));
        assert!((conf.unwrap() - 0.7).abs() < 1e-6);
        assert!(low.is_empty());
    }

    #[test]
    fn low_word_flagged_from_multi_token_average() {
        // "world" = " wor"(0.4) + "ld"(0.5) averages 0.45 < 0.55.
        let (conf, low) =
            aggregate_token_probs(&toks(&[(" hello", 0.9), (" wor", 0.4), ("ld", 0.5)]));
        assert!((conf.unwrap() - 0.6).abs() < 1e-6);
        assert_eq!(low, vec!["world".to_string()]);
    }

    #[test]
    fn word_at_threshold_is_not_low() {
        let (_, low) = aggregate_token_probs(&toks(&[(" fine", LOW_WORD_THRESHOLD)]));
        assert!(low.is_empty());
    }

    #[test]
    fn piece_spanning_boundary_credits_both_words() {
        // One low-probability piece covering the end of "a" and all of "b".
        let (_, low) = aggregate_token_probs(&toks(&[("a b", 0.4)]));
        assert_eq!(low, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn punctuation_piece_does_not_drag_word_down() {
        // A shaky terminal "." (common at utterance end) says nothing about
        // "ok" itself: only the speech token's 0.9 counts, so nothing is low.
        let (conf, low) = aggregate_token_probs(&toks(&[(" ok", 0.9), (".", 0.1)]));
        assert!(low.is_empty());
        // Transcript-level confidence still averages every token.
        assert!((conf.unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn low_speech_token_still_flags_word_despite_confident_punctuation() {
        // The speech token is the shaky one: "ok."(word text keeps the '.')
        // averages only the 0.4 from " ok" and is flagged.
        let (_, low) = aggregate_token_probs(&toks(&[(" ok", 0.4), (".", 0.9)]));
        assert_eq!(low, vec!["ok.".to_string()]);
    }

    #[test]
    fn punctuation_only_word_is_never_flagged() {
        // A standalone dash gets no speech probability at all - nothing to
        // judge, so it must not be flagged (and must not divide by zero).
        let (_, low) = aggregate_token_probs(&toks(&[(" ok", 0.9), (" -", 0.05)]));
        assert!(low.is_empty());
    }

    #[test]
    fn mixed_piece_credits_only_the_word_it_speaks_for() {
        // ". No" appends '.' to the previous word without crediting it, then
        // credits its probability to "No" via the alphanumeric 'N'.
        let (_, low) = aggregate_token_probs(&toks(&[(" ok", 0.9), (". No", 0.3)]));
        assert_eq!(low, vec!["No".to_string()]);
    }

    #[test]
    fn low_words_keep_transcript_order() {
        let (_, low) =
            aggregate_token_probs(&toks(&[(" bad1", 0.1), (" good", 0.9), (" bad2", 0.2)]));
        assert_eq!(low, vec!["bad1".to_string(), "bad2".to_string()]);
    }
}
