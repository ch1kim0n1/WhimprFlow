//! Output-safety helpers applied after the selected cleanup provider returns text.
//!
//! The matcher intentionally uses exact, case-insensitive word matching rather
//! than fuzzy replacement so ordinary words are not unexpectedly modified.

const REDACTED: &str = "[redacted]";
const INAPPROPRIATE_WORDS: &[&str] = &[
    "asshole",
    "bastard",
    "bitch",
    "bullshit",
    "crap",
    "cunt",
    "damn",
    "dick",
    "fuck",
    "fucker",
    "fucking",
    "goddamn",
    "hell",
    "motherfucker",
    "piss",
    "shit",
    "slut",
    "whore",
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// Replace known curses and inappropriate words with a neutral marker.
///
/// Punctuation and whitespace are preserved. Matching happens only on complete
/// words, so a harmless larger word containing a matching sequence is unchanged.
pub fn redact_inappropriate_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();

    let flush = |word: &mut String, out: &mut String| {
        if !word.is_empty() {
            if INAPPROPRIATE_WORDS
                .iter()
                .any(|candidate| word.eq_ignore_ascii_case(candidate))
            {
                out.push_str(REDACTED);
            } else {
                out.push_str(word);
            }
            word.clear();
        }
    };

    for c in text.chars() {
        if is_word_char(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::redact_inappropriate_words;

    #[test]
    fn redacts_case_insensitive_complete_words_and_preserves_punctuation() {
        assert_eq!(
            redact_inappropriate_words("This is SHIT, not a shitshow."),
            "This is [redacted], not a shitshow."
        );
    }

    #[test]
    fn preserves_clean_text() {
        assert_eq!(
            redact_inappropriate_words("A thoughtful, professional note."),
            "A thoughtful, professional note."
        );
    }

    #[test]
    fn redacts_multiple_words_without_losing_spacing() {
        assert_eq!(
            redact_inappropriate_words("fuck\nthis shit!"),
            "[redacted]\nthis [redacted]!"
        );
    }
}
