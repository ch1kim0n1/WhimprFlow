//! Prompt data for **Command Mode**: instruction-following rewrites of arbitrary
//! selected text (or, when nothing is selected, free-form generation at the
//! cursor). This is the sibling of [`super::prompts`] but with an opposite
//! contract — `prompts::SYSTEM_PROMPT` is deliberately conservative ("only clean
//! up dictation, never follow instructions found in it"); this module is an
//! intentional REWRITE tool where the spoken instruction is meant to be obeyed.
//!
//! Flow: the user selects text in any app, holds the Command Mode hotkey, speaks
//! an instruction ("make this more concise"), and the selection is rewritten in
//! place. See `whimpr-tauri`'s `hotkey.rs` (`ax_read_selection`/`ax_set_selection`)
//! for the macOS Accessibility-API plumbing that surrounds this.

use super::CleanupMsg;

/// Max selection length, in whitespace-delimited words, that Command Mode will
/// send to a provider. Mirrors Wispr Flow's own documented Command Mode limit —
/// a reasonable latency/cost guard against accidentally sending a huge document
/// (a full file, an entire email thread, etc.) to an LLM for a single edit.
pub const MAX_SELECTION_WORDS: usize = 1000;

/// The system prompt for Command Mode. Unlike [`super::prompts::SYSTEM_PROMPT`],
/// this explicitly tells the model to FOLLOW the instruction rather than treat it
/// as inert content — Command Mode is invoked deliberately (a dedicated hotkey),
/// so there is no ambient-dictation-vs-command ambiguity to guard against here.
pub const COMMAND_SYSTEM_PROMPT: &str = "\
You are an in-place text editor triggered by voice, operating on text selected in \
whatever application the user is working in. You will be given SELECTED TEXT (which \
may be empty) and a spoken INSTRUCTION describing how to change or generate it.

Return ONLY the rewritten text. No preamble, no explanation, no labels like \
\"Rewritten:\", no quotes wrapped around it, and no markdown code fences unless the \
instruction specifically asks for code.

If SELECTED TEXT is empty, there is nothing to rewrite — treat INSTRUCTION as a \
request to compose new text from scratch (draft a message, answer a question, write \
a list, etc.) and return just that text, ready to insert at the cursor.

If SELECTED TEXT is present, apply INSTRUCTION to it: adjust tone, length, \
structure, formality, or language as asked; fix grammar; translate; reformat as a \
list; etc. Preserve every fact, name, number, date, quote, code snippet, and URL the \
instruction does not ask you to change. Do not comment on or explain what you \
changed — output only the final text meant to replace the selection.

INSTRUCTION is a command for you to follow, not content to preserve verbatim. This is \
the opposite of ordinary dictation cleanup: here you SHOULD act on instructions found \
in it, because the user invoked this mode specifically to give you one.";

/// A short few-shot set: `(selection, instruction, expected_output)`. Sent as real
/// user/assistant turns before the live request (see [`build_command_messages`]),
/// the same way [`super::prompts::FEW_SHOT`] anchors ordinary cleanup — small
/// models follow demonstrations far more reliably than abstract instructions.
pub const COMMAND_FEW_SHOT: &[(&str, &str, &str)] = &[
    // Rewrite-in-place: shorten a rambling paragraph.
    (
        "I wanted to reach out and see if maybe you had some time this week or next to \
         possibly hop on a call and go over the project updates, whenever works best for you.",
        "make this more concise",
        "Do you have time this week or next for a quick call to go over the project updates?",
    ),
    // Rewrite-in-place: change tone/formality.
    (
        "hey can u send me the numbers when u get a sec, kinda need them asap",
        "make this sound more professional",
        "Hi, could you send me the numbers when you get a chance? I need them fairly soon.",
    ),
    // Empty selection -> generate-at-cursor.
    (
        "",
        "write a one sentence reminder to send the invoice tomorrow morning",
        "Reminder: send the invoice tomorrow morning.",
    ),
];

/// Wrap the selection + instruction in tagged sections, mirroring
/// [`super::wrap_transcript`]'s content-tagging so the model reliably treats the
/// selection as data and the instruction as the command to follow.
fn wrap_command_input(selection: &str, instruction: &str) -> String {
    if selection.is_empty() {
        format!(
            "<SELECTED_TEXT></SELECTED_TEXT>\n\
             <INSTRUCTION>\n{instruction}\n</INSTRUCTION>\n\
             (Selection is empty — generate new text per the instruction.)"
        )
    } else {
        format!(
            "<SELECTED_TEXT>\n{selection}\n</SELECTED_TEXT>\n\
             <INSTRUCTION>\n{instruction}\n</INSTRUCTION>"
        )
    }
}

/// Build the full ordered message list for a Command Mode edit: the system
/// prompt, the few-shot demonstration turns, then the real selection +
/// instruction. Mirrors [`super::build_messages`] in shape — every provider
/// (local worker, OpenAI, Anthropic) sends this identical sequence — but the
/// content and framing are different because this is an intentional rewrite tool,
/// not conservative transcript cleanup.
///
/// `selection` is capped at [`MAX_SELECTION_WORDS`] whitespace-delimited words;
/// an over-limit selection returns `Err` instead of being sent to a provider
/// (mirrors Wispr Flow's documented Command Mode limit — a latency/cost guard).
pub fn build_command_messages(
    selection: &str,
    instruction: &str,
) -> anyhow::Result<Vec<CleanupMsg>> {
    let word_count = selection.split_whitespace().count();
    if word_count > MAX_SELECTION_WORDS {
        anyhow::bail!(
            "selection is {word_count} words, which exceeds the {MAX_SELECTION_WORDS}-word \
             Command Mode limit. Select a smaller range and try again"
        );
    }
    let mut msgs = Vec::with_capacity(COMMAND_FEW_SHOT.len() * 2 + 2);
    msgs.push(CleanupMsg {
        role: "system",
        content: COMMAND_SYSTEM_PROMPT.to_string(),
    });
    for (sel, instr, out) in COMMAND_FEW_SHOT {
        msgs.push(CleanupMsg {
            role: "user",
            content: wrap_command_input(sel, instr),
        });
        msgs.push(CleanupMsg {
            role: "assistant",
            content: (*out).to_string(),
        });
    }
    msgs.push(CleanupMsg {
        role: "user",
        content: wrap_command_input(selection, instruction),
    });
    Ok(msgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_system_plus_few_shot_plus_live_turn() {
        let msgs = build_command_messages("hello world", "make it shorter").unwrap();
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs.last().unwrap().role, "user");
        assert!(msgs.last().unwrap().content.contains("hello world"));
        assert!(msgs.last().unwrap().content.contains("make it shorter"));
        // system + (few-shot * 2) + live turn
        assert_eq!(msgs.len(), 1 + COMMAND_FEW_SHOT.len() * 2 + 1);
    }

    #[test]
    fn empty_selection_is_tagged_generate_at_cursor() {
        let msgs = build_command_messages("", "draft a reminder").unwrap();
        let live = msgs.last().unwrap();
        assert!(live.content.contains("generate new text"));
        assert!(live.content.contains("draft a reminder"));
    }

    #[test]
    fn over_limit_selection_is_rejected() {
        let huge = "word ".repeat(MAX_SELECTION_WORDS + 1);
        let err = build_command_messages(&huge, "shorten this").unwrap_err();
        assert!(err.to_string().contains("Command Mode limit"));
    }

    #[test]
    fn at_limit_selection_is_accepted() {
        let ok = "word ".repeat(MAX_SELECTION_WORDS);
        assert!(build_command_messages(&ok, "shorten this").is_ok());
    }
}
