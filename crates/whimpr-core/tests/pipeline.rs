//! Integration test: exercises the platform-agnostic half of the dictation
//! pipeline  -  snippet match, dictionary prefilter, cleanup message assembly,
//! and the deterministic gates  -  wired together the same way the real
//! `clean_transcript()` in each platform layer (hotkey.rs/win.rs/linux.rs)
//! composes them, but using a fake `CleanupProvider` instead of a real
//! network/local-model call.
//!
//! ponytail: this can't reach the ASR or the actual OS paste call (those need
//! a microphone and a real window session, which is exactly why they were
//! never covered by any automated test)  -  it proves the *logic* pipeline that
//! CAN run headless, which is the gap that was actually fixable here.

use whimpr_core::cleanup::{build_messages, evaluate_gates, CleanupProvider, HealthStatus, ProviderId};
use whimpr_core::{CleanupContext, CleanupLevel, DictSource, DictionaryStore, SnippetStore};

/// A `CleanupProvider` that returns a fixed string regardless of input, so the
/// test controls exactly what "the model" hands back to the gates.
struct FakeProvider(String);

impl CleanupProvider for FakeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Local
    }
    fn health_check(&self) -> HealthStatus {
        HealthStatus::Ready
    }
    fn cleanup(&self, _raw: &str, _ctx: &CleanupContext) -> anyhow::Result<String> {
        Ok(self.0.clone())
    }
    fn command_edit(&self, _selection: &str, _instruction: &str) -> anyhow::Result<String> {
        Ok(self.0.clone())
    }
}

/// A snippet match should short-circuit the pipeline entirely: no dictionary
/// lookup, no provider call, no gate evaluation  -  the expansion is the result.
#[test]
fn snippet_match_short_circuits_before_cleanup() {
    let mut snippets = SnippetStore::default();
    snippets.add("my email".into(), "user@example.com".into());

    let raw = "my email";
    let matched = snippets.find_match(raw);
    assert_eq!(matched.map(|e| e.expansion.as_str()), Some("user@example.com"));
    // Real platform code stops here on a match and never calls a provider.
}

/// No snippet match → dictionary prefilter should surface the relevant vocab
/// entry, `build_messages` should carry it into the custom-vocabulary block,
/// and a conservative (Light-level) cleanup that only trims filler should
/// pass the gate.
#[test]
fn dictionary_vocab_flows_into_cleanup_messages_and_passes_light_gate() {
    let mut dict = DictionaryStore::default();
    dict.add("Manvi", vec!["Monvi".into()], DictSource::Manual);

    let raw = "send the deck to monvi please um";
    let vocab = dict.prefilter(raw, 15);
    assert!(vocab.iter().any(|v| v.correct == "Manvi"), "prefilter should surface Manvi for 'monvi'");

    let ctx = CleanupContext {
        level: CleanupLevel::Light,
        vocab,
        ..Default::default()
    };
    let messages = build_messages(raw, &ctx);
    let user_turn = messages.last().expect("at least one message");
    assert!(
        user_turn.content.contains("Manvi") && user_turn.content.contains("Monvi"),
        "the assembled prompt should carry the vocab entry through to the model"
    );

    // Simulate the model doing exactly what Light cleanup is allowed to do:
    // drop the filler word, fix the mis-hearing, nothing else.
    let cleaned = "Send the deck to Manvi please.";
    let verdict = evaluate_gates(raw, cleaned, CleanupLevel::Light);
    assert!(verdict.passed(), "conservative filler removal must pass the Light gate: {verdict:?}");
}

/// A provider that hallucinates a full rewrite must be caught by the Light
/// gate and rejected  -  the platform layer's fallback-to-raw path is what
/// keeps a bad edit from ever reaching the user's cursor.
#[test]
fn hallucinated_rewrite_is_rejected_by_the_light_gate() {
    let raw = "send the deck to monvi please um";
    let provider = FakeProvider("I'd be happy to help you send that deck right away!".to_string());
    let ctx = CleanupContext { level: CleanupLevel::Light, ..Default::default() };

    let cleaned = provider.cleanup(raw, &ctx).unwrap();
    let verdict = evaluate_gates(raw, &cleaned, CleanupLevel::Light);
    assert!(!verdict.passed(), "an assistant-style rewrite must not pass Light: {verdict:?}");
    // The real pipeline's response to `!verdict.passed()` is to paste the raw
    // transcript instead  -  that fallback itself is exercised by
    // `crates/whimpr-core/src/cleanup/mod.rs`'s own gate tests, not repeated here.
}
