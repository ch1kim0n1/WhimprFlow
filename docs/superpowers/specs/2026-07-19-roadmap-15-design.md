# WhimprFlow 15-item roadmap: design + interface contract

Date: 2026-07-19. Source: product roadmap (Tier 1 trust wedge, Tier 2 moat trio,
Tier 3 mid-effort, Tier 4 moonshot v1s, Tier 5 positioning). This document pins the
cross-layer interfaces so core, shell, and UI can be built against one contract.

Scope rule: Tier 1-3 land at full v1 depth. Tier 4-5 land as honest minimal
versions built on the same foundations, each with a named ceiling and upgrade path
(marked `ponytail:` in code).

## Feature map (what each item becomes in this codebase)

1. **Reversible editing / raw-final diff / restore-raw.** Persist the raw
   (pre-cleanup) transcript per dictation in `SessionRecord`. Home history gets a
   word-level raw-vs-final diff view, provenance badge, and Copy-raw. The existing
   Cmd+Shift+Z hotkey stays the one-key "restore what I said".
2. **Privacy cockpit + retention.** Per-dictation `Provenance` (engine, cleanup
   route, sent_to_cloud, gate verdict) recorded in history. New Privacy pane:
   per-dictation ledger, retention control (`retention_days`), delete-all-text.
   Retention pruning strips stored text (keeps numeric stats) past the cutoff.
3. **Reliability + insertion receipt + confidence.** Whisper token probabilities
   produce per-dictation `confidence` plus `low_words`. Every finalize emits a
   `whimpr://receipt` event (pasted ok / saved to notes / error, word count,
   confidence). FlowBar flashes the receipt; Home shows health chips via
   `get_health` (ASR model loaded, which file, local LLM ready, mic, accessibility).
   Low-confidence words render underlined in history; click = add to dictionary.
4. **Voice Memory.** New `whimpr_core::voice_memory`: an AES-256-GCM-encrypted,
   local, auditable log of learned corrections (autolearn + manual dictionary
   edits), exportable/importable as plain JSON bundle together with dictionary +
   snippets + style. Key lives in the OS keychain. New Voice Memory pane: audit
   list, export, clear. Ceiling: no acoustic adaptation; corrections + vocab only.
5. **Context Capsule.** Opt-in (default OFF) per-app context bundle captured at
   record start: frontmost app, AX-selected text (macOS), dictionary glossary,
   style. Fills the already-existing `CleanupContext.window_context`. The full
   capsule is inspectable via `get_last_capsule` and shown in the Privacy pane, so
   the user sees exactly what a cleanup request would include.
6. **Voice Workflows.** New `whimpr_core::workflows`: named, versioned entries
   `{name, trigger, instruction, destination, require_approval}`. Spoken trigger
   prefix routes the remainder of the utterance through the command-edit provider
   path; destination = Paste | Clipboard | Note. `require_approval` holds the
   result as a pending item (event `whimpr://pending`; approve/reject commands).
   Every edit bumps `version` and archives the prior revision.
7. **Streaming provisional text + safe commit.** `whimpr-audio` gains a shared
   sample buffer + `CaptureHandle::snapshot()`. While recording (and
   `streaming_preview` on), a partial-transcription loop emits
   `whimpr://transcript/partial`; the FlowBar renders live text. Partials are never
   pasted - commit stays the verified finalize path. macOS-only v1.
8. **Voice Studio.** Scratchpad pane becomes Studio: Editor + Timeline + Notes
   tabs. Timeline = history grouped by day with raw/final versions, search (item
   10), insert-into-editor; export Markdown file, copy as GitHub-issue / Linear
   formats. Notes tab lists meeting/workflow/capture notes.
9. **Voice-native IDE (v1: Code Mode).** `code_mode_auto` setting (default on):
   when the target app is an IDE/terminal, the cleanup system prompt switches to a
   code-dictation variant (verbatim identifiers, spoken casing conventions, no
   prose autocorrect). Ceiling: prompt-level only; no project awareness.
10. **Knowledge retrieval (v1).** Search box over full history in Studio Timeline
    (substring + simple fuzzy). Ceiling: no embeddings, no voice query loop.
11. **Voice Compiler (v1).** Studio "Compile" button: structure the editor text
    into a typed draft plus an explicit `MISSING:` facts list via a canned
    command-edit instruction; UI renders the missing-facts card.
12. **Multimodal thought capture (v1).** `capture_screen` command (macOS
    `screencapture -x` into app-support `captures/`); Studio "Snap + note" appends
    a note with the image path. Ceiling: image is stored + linked, not analyzed.
13. **Live Meeting Shadow (v1).** `meeting_mode` setting: a locked (hands-free)
    session's transcript is appended to Notes instead of pasted; long-form
    transcription path disables whisper single-segment mode. Ceiling: one
    transcript at stop; no rolling chunking, no diarization.
14. **Language Fusion (v1).** Fix the real bug: thread `settings.language` into
    whisper (`"auto"` when None + multilingual model; keep `"en"` for `.en`
    models). Language picker in Settings. Ceiling: session-level detection, not
    phrase-level code-switching.
15. **Accessibility-first pass.** Global `:focus-visible` rings, aria-labels on
    icon-only controls, `role="status"`/aria-live on FlowBar + receipt, reduced
    motion already respected; keep it.

## Pinned contracts

### whimpr-core

```rust
// stats.rs
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    #[serde(default)] pub asr_engine: String,   // e.g. "whisper.cpp:ggml-base.en.bin"
    #[serde(default)] pub cleanup: String,      // "raw" | "local" | "openai:<model>" | "anthropic:<model>" | "snippet" | "workflow:<name>"
    #[serde(default)] pub sent_to_cloud: bool,
    #[serde(default)] pub gate: String,         // "passed" | "rejected" | "skipped"
}
// SessionRecord adds: #[serde(default)] raw: String, provenance: Provenance,
//                     confidence: Option<f32>, low_words: Vec<String>
// HistoryItem adds the same four fields.
// StatsStore adds: record_full(SessionRecord)  (old record() kept, delegates),
//   prune_texts(now_unix: u64, retention_days: u32) -> usize  (clears text+raw, keeps numbers),
//   clear_texts() -> usize
```

```rust
// settings.rs additions (all #[serde(default)]):
pub retention_days: Option<u32>,      // None = keep forever; Some(0) = never store text
pub capsule: CapsuleSettings,         // { enabled: bool=false, include_selection: bool=false, apps: Vec<String>=[] (empty = all apps when enabled) }
pub code_mode_auto: bool,             // default true (fn default_true)
pub meeting_mode: bool,               // default false
pub streaming_preview: bool,          // default true
```

```rust
// workflows/mod.rs (mirrors snippets store pattern)
pub enum WorkflowDestination { Paste, Clipboard, Note }   // snake_case serde
pub struct WorkflowRevision { pub version: u32, pub instruction: String, pub updated_unix: u64 }
pub struct WorkflowEntry {
    pub name: String, pub trigger: String, pub instruction: String,
    pub destination: WorkflowDestination, pub require_approval: bool,
    pub version: u32, pub updated_unix: u64,
    #[serde(default)] pub history: Vec<WorkflowRevision>,
}
pub struct WorkflowStore { pub entries: Vec<WorkflowEntry> }
// load/save/add(upsert: bumps version, archives prior revision)/remove(name)
// find_match(raw) -> Option<(&WorkflowEntry, String /*payload after trigger*/)>
//   trigger matches case-insensitively at utterance start, whole-word boundary;
//   payload may be empty.
```

```rust
// voice_memory/mod.rs
pub struct CorrectionEvent { pub ts_unix: u64, pub from: String, pub to: String, pub source: String }
pub struct VoiceMemory { pub corrections: Vec<CorrectionEvent> }
// load_encrypted(path, key: &[u8; 32]) -> Self   (missing/undecryptable => default)
// save_encrypted(&self, path, key) -> anyhow::Result<()>  (AES-256-GCM, 12-byte random nonce prefix)
// record(from, to, source, ts)
// export_bundle(&self, dict, snippets, style) -> serde_json::Value (plain JSON)
// deps: aes-gcm + rand (or getrandom) in whimpr-core
```

```rust
// cleanup/prompts.rs: pub fn is_code_app(bundle_id: &str) -> bool
// system_for gains code-dictation guidance when the app is a code app AND the
// caller opted in; implement as system_for_ctx(level, app, code_mode: bool) with
// the old signatures preserved.
```

### whimpr-asr / whimpr-audio

```rust
// Transcript (core asr/mod.rs) adds: #[serde(default)] pub low_words: Vec<String>
// WhisperEngine adds inherent:
//   transcribe_opts(&self, pcm16k: &[f32], language: Option<&str>, long_form: bool)
//     -> anyhow::Result<Transcript>
//   language: Some("en") etc; None => "auto". long_form => single_segment(false).
//   confidence = mean token probability; low_words = words whose tokens avg < 0.55.
//   Trait transcribe() delegates to transcribe_opts(pcm, Some("en"), false).
// whimpr-audio: samples accumulate into a shared Arc<Mutex<Vec<f32>>>;
//   CaptureHandle::snapshot(&self) -> CaptureResult (copy so far).
```

### Tauri shell (src-tauri)

Events (emitted to overlay label `whimpr_bar`, and hub where noted):

- `whimpr://transcript/partial` `{ text: string }`
- `whimpr://receipt` `{ ok: bool, action: "pasted"|"noted"|"clipboard"|"pending"|"error", app: string|null, words: number, confidence: number|null, low_words: string[], message: string|null }` (also emitted to hub `main`)
- `whimpr://pending` `{ name: string, preview: string }` (hub too)

New/changed commands (lib.rs):

- `get_health() -> Health { asr_ready, asr_model: Option<String>, local_llm_ready, microphone, accessibility }`
- `clear_history_text() -> usize`
- `get_last_capsule() -> Option<CapsuleReport { app: Option<String>, selection_preview: Option<String>, glossary: Vec<String>, style: bool, enabled: bool }>`
- `get_workflows() -> Vec<WorkflowEntry>` / `add_workflow(entry fields)` / `remove_workflow(name)`
- `approve_pending()` / `reject_pending()`
- `get_voice_memory() -> Vec<CorrectionEvent>` / `export_voice_memory() -> String (path)` / `clear_voice_memory()`
- `get_notes() -> Vec<Note { ts_unix, title, text, image_path: Option<String> }>` / `add_note(title, text)` / `remove_note(ts_unix)`
- `capture_screen() -> String (path)` (macOS; error elsewhere)

Notes persist at support dir `notes.json` (shell-level store mirroring the
snippets pattern; kept in the shell because it is app-glue, not core logic).

Finalize pipeline order (all three platform files where applicable):

1. transcribe with `settings.language` + confidence (long_form when session was locked and meeting_mode)
2. workflow trigger match -> command_edit(payload, instruction) -> destination / pending
3. snippet match (existing)
4. `clean_transcript` -> returns `CleanOutcome { raw_out, final_text, provenance }`
5. meeting_mode + locked session -> append note, receipt `noted`, skip paste
6. paste -> receipt event (ok/error) -> `record_full` (raw + provenance + confidence) -> autolearn watch (which also records into voice memory)

Platform parity: macOS = full. Windows/Linux mirror items 1-3, 6, 13, 14
(provenance, receipt, workflows, language, meeting notes); capsule (AX) and
streaming preview stay macOS-only in this pass with a `ponytail:` note.

### UI (ui/src)

- `api.ts`: mirror every new type/command/event above.
- New pages: `privacy`, `workflows`, `memory`; `scratchpad` becomes Voice Studio
  (page key stays `scratchpad`).
- Home: receipt banner (live event), health chips, history diff (raw vs final,
  word-level), provenance badge (Local / Cloud + gate), low-word underline with
  add-to-dictionary popover.
- Settings: language picker, streaming toggle, meeting-mode toggle, code-mode
  toggle. Privacy pane owns retention + capsule + ledger + delete-all-text.
- FlowBar: partial-text line while recording; receipt flash; `role="status"`.
- Accessibility: global `:focus-visible` outline, aria-labels on icon-only
  buttons.

## Testing

- Core: unit tests per new module (workflow matching incl. trailing-period ASR
  case; voice-memory encrypt/decrypt round-trip + wrong-key fallback; retention
  pruning; provenance serde back-compat with old stats.json).
- ASR: confidence math unit-testable helper (token probs -> mean + low words).
- Shell: compiles + existing tests; manual smoke via dev build.
- UI: `npm run build` type-checks everything.
- Back-compat: every new serde field defaults; old JSON files must load.
