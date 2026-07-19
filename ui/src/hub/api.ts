// Typed wrappers over the Tauri command surface. In a plain browser (vite dev
// without the shell) the invoke import fails and we fall back to defaults so the
// Hub still renders for iteration.

export type CleanupMode = "raw" | "local" | "open_ai" | "anthropic";
export type CleanupLevel = "none" | "light" | "medium" | "high";

// Mirrors whimpr-core's Key enum, serialized via serde's adjacently-tagged
// representation: {"kind":"char","value":"V"} or {"kind":"escape"}.
export type KeyJson = { kind: "char"; value: string } | { kind: "escape" };

// Mirrors whimpr-core's Chord: one modifier combination bound to a rebindable
// action (checked on a plain KeyDown, not a hold gesture).
export interface ChordJson {
  meta: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  key: KeyJson;
}

// Mirrors whimpr-core's KeyBindings: the four shortcuts safe to rebind.
export interface KeyBindings {
  cancel: ChordJson;
  paste_last: ChordJson;
  copy_last: ChordJson;
  undo_last: ChordJson;
}

// Mirrors whimpr-core's Formality enum.
export type Formality = "casual" | "neutral" | "formal";

// Mirrors whimpr-core's StyleProfile: personal writing style applied to cleanup
// output as presentation guidance (tone/formality/free-text note).
export interface StyleProfile {
  formality: Formality;
  custom_instructions: string;
}

// Keep in sync with whimpr-core's MAX_STYLE_INSTRUCTIONS_LEN.
export const MAX_STYLE_INSTRUCTIONS_LEN = 600;

// Mirrors whimpr-core's CapsuleSettings: the opt-in per-app context bundle.
export interface CapsuleSettings {
  enabled: boolean;
  include_selection: boolean;
  // Bundle ids the capsule is limited to. Empty = all apps (when enabled).
  apps: string[];
}

export interface Settings {
  cleanup_mode: CleanupMode;
  cleanup_level: CleanupLevel;
  openai_model: string;
  // API root for "OpenAI" mode; leave blank for OpenAI itself, or point at an
  // OpenAI-compatible endpoint like OpenRouter (https://openrouter.ai/api/v1).
  openai_base_url: string;
  anthropic_model: string;
  sound_on_start: boolean;
  safe_mode: boolean;
  // ASR language, as a whisper.cpp language code (e.g. "en", "es"). null means
  // auto-detect.
  language: string | null;
  keybindings: KeyBindings;
  style: StyleProfile;
  // Days of dictation text kept in history. null = keep forever; 0 = never
  // store text. Numeric stats are always kept.
  retention_days: number | null;
  capsule: CapsuleSettings;
  // Switch the cleanup prompt to the code-dictation variant in IDEs/terminals.
  code_mode_auto: boolean;
  // Hands-free (locked) sessions go to Studio notes instead of the cursor.
  meeting_mode: boolean;
  // Show live provisional text in the FlowBar while recording.
  streaming_preview: boolean;
}

export interface Status {
  accessibility: boolean;
  microphone: boolean;
  input_monitoring: boolean;
  has_openai_key: boolean;
  has_anthropic_key: boolean;
}

export interface StatsSummary {
  total_words: number;
  total_sessions: number;
  total_speaking_secs: number;
  avg_wpm: number;
  best_wpm: number;
  words_today: number;
  wpm_today: number;
  day_streak: number;
  time_saved_secs: number;
  last7_words: number[];
}

export const EMPTY_STATS: StatsSummary = {
  total_words: 0,
  total_sessions: 0,
  total_speaking_secs: 0,
  avg_wpm: 0,
  best_wpm: 0,
  words_today: 0,
  wpm_today: 0,
  day_streak: 0,
  time_saved_secs: 0,
  last7_words: [0, 0, 0, 0, 0, 0, 0],
};

// Browser-preview fallback only; the real app always loads the platform's actual
// bindings from the backend. Mirrors whimpr-core's macOS default.
export const DEFAULT_KEYBINDINGS: KeyBindings = {
  cancel: { meta: false, ctrl: false, alt: false, shift: false, key: { kind: "escape" } },
  paste_last: { meta: true, ctrl: false, alt: false, shift: true, key: { kind: "char", value: "V" } },
  copy_last: { meta: true, ctrl: false, alt: false, shift: true, key: { kind: "char", value: "C" } },
  undo_last: { meta: true, ctrl: false, alt: false, shift: true, key: { kind: "char", value: "Z" } },
};

export const DEFAULT_STYLE: StyleProfile = {
  formality: "neutral",
  custom_instructions: "",
};

export const DEFAULT_CAPSULE: CapsuleSettings = {
  enabled: false,
  include_selection: false,
  apps: [],
};

export const DEFAULT_SETTINGS: Settings = {
  cleanup_mode: "open_ai",
  cleanup_level: "light",
  openai_model: "gpt-4o-mini",
  openai_base_url: "",
  anthropic_model: "claude-haiku-4-5",
  sound_on_start: true,
  safe_mode: false,
  language: null,
  keybindings: DEFAULT_KEYBINDINGS,
  style: DEFAULT_STYLE,
  retention_days: null,
  capsule: DEFAULT_CAPSULE,
  code_mode_auto: true,
  meeting_mode: false,
  streaming_preview: true,
};

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export async function getSettings(): Promise<Settings> {
  try {
    return await invoke<Settings>("get_settings");
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export async function setSettings(settings: Settings): Promise<void> {
  try {
    await invoke<void>("set_settings", { settings });
  } catch {
    /* browser preview  no-op */
  }
}

export async function getStatus(): Promise<Status> {
  try {
    return await invoke<Status>("get_status");
  } catch {
    return {
      accessibility: false,
      microphone: false,
      input_monitoring: false,
      has_openai_key: false,
      has_anthropic_key: false,
    };
  }
}

export async function getStats(): Promise<StatsSummary> {
  try {
    const tz = new Date().getTimezoneOffset(); // minutes to add to local -> UTC
    return await invoke<StatsSummary>("get_stats", { tzOffsetMinutes: tz });
  } catch {
    return EMPTY_STATS;
  }
}

export async function requestMicrophone(): Promise<void> {
  try {
    await invoke<void>("request_microphone");
  } catch {
    /* browser preview */
  }
}

export async function requestAccessibility(): Promise<void> {
  try {
    await invoke<void>("request_accessibility");
  } catch {
    /* browser preview */
  }
}

export async function requestInputMonitoring(): Promise<void> {
  try {
    await invoke<void>("request_input_monitoring");
  } catch {
    /* browser preview */
  }
}

export async function setApiKey(provider: "openai" | "anthropic", key: string): Promise<void> {
  try {
    await invoke<void>("set_api_key", { provider, key });
  } catch {
    /* browser preview */
  }
}

// History

// Mirrors whimpr-core's Provenance: where a dictation's text came from.
export interface Provenance {
  // ASR engine + model, e.g. "whisper.cpp:ggml-base.en.bin".
  asr_engine: string;
  // "raw" | "local" | "openai:<model>" | "anthropic:<model>" | "snippet" | "workflow:<name>"
  cleanup: string;
  sent_to_cloud: boolean;
  gate: "passed" | "rejected" | "skipped" | string;
}

export interface HistoryItem {
  ts_unix: number;
  text: string;
  app: string | null;
  words: number;
  // Raw (pre-cleanup) transcript, for the raw-vs-final diff view.
  raw: string;
  provenance: Provenance;
  confidence: number | null;
  low_words: string[];
}

// Recent dictations with stored text, newest first. `limit` defaults to 200 on
// the backend; the Studio Timeline passes a higher cap so search covers the
// full history.
export async function getHistory(limit?: number): Promise<HistoryItem[]> {
  try {
    return await invoke<HistoryItem[]>("get_history", { limit });
  } catch {
    return [];
  }
}

// The Privacy pane's dictation ledger: same rows as getHistory but INCLUDING
// textless records (pruned or never stored), so every dictation is auditable.
export async function getLedger(limit?: number): Promise<HistoryItem[]> {
  try {
    return await invoke<HistoryItem[]>("get_ledger", { limit });
  } catch {
    return [];
  }
}

// Dictionary
export interface DictEntry {
  correct: string;
  mishears: string[];
  auto: boolean;
}

export async function getDictionary(): Promise<DictEntry[]> {
  try {
    return await invoke<DictEntry[]>("get_dictionary");
  } catch {
    return [];
  }
}

export async function addDictionaryEntry(correct: string, mishears: string[]): Promise<void> {
  try {
    await invoke<void>("add_dictionary_entry", { correct, mishears });
  } catch {
    /* browser preview  no-op */
  }
}

export async function removeDictionaryEntry(correct: string): Promise<void> {
  try {
    await invoke<void>("remove_dictionary_entry", { correct });
  } catch {
    /* browser preview  no-op */
  }
}

// Snippets
export interface SnippetEntry {
  trigger: string;
  expansion: string;
}

export async function getSnippets(): Promise<SnippetEntry[]> {
  try {
    return await invoke<SnippetEntry[]>("get_snippets");
  } catch {
    return [];
  }
}

export async function addSnippet(trigger: string, expansion: string): Promise<void> {
  try {
    await invoke<void>("add_snippet", { trigger, expansion });
  } catch {
    /* browser preview  no-op */
  }
}

export async function removeSnippet(trigger: string): Promise<void> {
  try {
    await invoke<void>("remove_snippet", { trigger });
  } catch {
    /* browser preview  no-op */
  }
}

// Backup: user-initiated one-off action; surfaces real success/failure.
export async function backupData(): Promise<string> {
  return invoke<string>("backup_data");
}

// Command Mode (manual test hook): runs the instruction-following rewrite
// against selection through the configured provider. macOS-only; rejects elsewhere.
export async function testCommandEdit(selection: string, instruction: string): Promise<string> {
  return invoke<string>("test_command_edit", { selection, instruction });
}

// Transforms: rewrites text per a canned instruction through the configured
// cleanup provider (reuses the Command Mode path). macOS-only; rejects elsewhere.
export async function runTransform(text: string, instruction: string): Promise<string> {
  return invoke<string>("run_transform", { text, instruction });
}

// Health: is dictation actually ready end to end (ASR model, local LLM,
// mic + accessibility permissions). Mirrors the shell's hotkey::Health.
export interface Health {
  asr_ready: boolean;
  asr_model: string | null;
  local_llm_ready: boolean;
  microphone: boolean;
  accessibility: boolean;
}

export async function getHealth(): Promise<Health> {
  try {
    return await invoke<Health>("get_health");
  } catch {
    return {
      asr_ready: false,
      asr_model: null,
      local_llm_ready: false,
      microphone: false,
      accessibility: false,
    };
  }
}

// Privacy: strip stored dictation text from history (numeric stats stay).
// Returns how many entries were cleared.
export async function clearHistoryText(): Promise<number> {
  try {
    return await invoke<number>("clear_history_text");
  } catch {
    return 0;
  }
}

// What the last Context Capsule contained - exactly what a cleanup request
// would include. null until a capsule has been captured this run.
export interface CapsuleReport {
  app: string | null;
  selection_preview: string | null;
  glossary: string[];
  style: boolean;
  enabled: boolean;
}

export async function getLastCapsule(): Promise<CapsuleReport | null> {
  try {
    return await invoke<CapsuleReport | null>("get_last_capsule");
  } catch {
    return null;
  }
}

// Workflows: spoken-trigger routines. Mirrors whimpr-core's workflows module.
export type WorkflowDestination = "paste" | "clipboard" | "note";

export interface WorkflowRevision {
  version: number;
  instruction: string;
  updated_unix: number;
}

export interface WorkflowEntry {
  name: string;
  trigger: string;
  instruction: string;
  destination: WorkflowDestination;
  require_approval: boolean;
  version: number;
  updated_unix: number;
  history: WorkflowRevision[];
}

export async function getWorkflows(): Promise<WorkflowEntry[]> {
  try {
    return await invoke<WorkflowEntry[]>("get_workflows");
  } catch {
    return [];
  }
}

// Add or update (keyed by name); an update bumps the version and archives the
// prior revision on the backend.
export async function addWorkflow(
  name: string,
  trigger: string,
  instruction: string,
  destination: WorkflowDestination,
  requireApproval: boolean,
): Promise<void> {
  try {
    await invoke<void>("add_workflow", { name, trigger, instruction, destination, requireApproval });
  } catch {
    /* browser preview  no-op */
  }
}

export async function removeWorkflow(name: string): Promise<void> {
  try {
    await invoke<void>("remove_workflow", { name });
  } catch {
    /* browser preview  no-op */
  }
}

// Approve/reject the workflow result currently held for approval (see the
// "whimpr://pending" event).
export async function approvePending(): Promise<void> {
  try {
    await invoke<void>("approve_pending");
  } catch {
    /* browser preview  no-op */
  }
}

export async function rejectPending(): Promise<void> {
  try {
    await invoke<void>("reject_pending");
  } catch {
    /* browser preview  no-op */
  }
}

// The workflow result currently held for approval, if any - lets the Workflows
// pane seed itself on mount, since the "whimpr://pending" event is
// fire-and-forget and may have fired before the pane existed.
export async function getPending(): Promise<PendingEvent | null> {
  try {
    return await invoke<PendingEvent | null>("get_pending");
  } catch {
    return null;
  }
}

// Voice Memory: the encrypted, auditable log of learned corrections.
export interface CorrectionEvent {
  ts_unix: number;
  // What the recognizer produced.
  from: string;
  // What the user corrected it to.
  to: string;
  // Where the correction came from, e.g. "autolearn" | "manual".
  source: string;
}

export async function getVoiceMemory(): Promise<CorrectionEvent[]> {
  try {
    return await invoke<CorrectionEvent[]>("get_voice_memory");
  } catch {
    return [];
  }
}

// Export corrections + dictionary + snippets + style as a plain-JSON bundle;
// resolves to the written file's path. User-initiated: surfaces real failure.
export async function exportVoiceMemory(): Promise<string> {
  return invoke<string>("export_voice_memory");
}

export async function clearVoiceMemory(): Promise<void> {
  try {
    await invoke<void>("clear_voice_memory");
  } catch {
    /* browser preview  no-op */
  }
}

// Screenshot into the app's captures folder; resolves to the image path.
// macOS-only, user-initiated: surfaces real failure.
export async function captureScreen(): Promise<string> {
  return invoke<string>("capture_screen");
}

// Notes (meeting transcripts, workflow notes, snap-notes), newest first.
export interface Note {
  ts_unix: number;
  title: string;
  text: string;
  // Path of a captured screenshot linked to this note, if any.
  image_path: string | null;
}

export async function getNotes(): Promise<Note[]> {
  try {
    return await invoke<Note[]>("get_notes");
  } catch {
    return [];
  }
}

export async function addNote(title: string, text: string, imagePath: string | null = null): Promise<void> {
  try {
    await invoke<void>("add_note", { title, text, imagePath });
  } catch {
    /* browser preview  no-op */
  }
}

export async function removeNote(tsUnix: number): Promise<void> {
  try {
    await invoke<void>("remove_note", { tsUnix });
  } catch {
    /* browser preview  no-op */
  }
}

// Shell events (mirrors src-tauri's payload structs). The overlay pill and the
// Hub both subscribe to these.
export const EVENT_TRANSCRIPT_PARTIAL = "whimpr://transcript/partial";
export const EVENT_RECEIPT = "whimpr://receipt";
export const EVENT_PENDING = "whimpr://pending";

// Live provisional text while recording (streaming preview).
export interface PartialTranscriptEvent {
  text: string;
}

export type ReceiptAction = "pasted" | "noted" | "clipboard" | "pending" | "error";

// The insertion receipt emitted after every finalize.
export interface ReceiptEvent {
  ok: boolean;
  action: ReceiptAction;
  app: string | null;
  words: number;
  confidence: number | null;
  low_words: string[];
  message: string | null;
}

// A workflow result awaiting approval.
export interface PendingEvent {
  name: string;
  preview: string;
}

// Subscribe to a shell event. In a plain browser the event import fails and
// the returned unsubscribe is a no-op (same fallback pattern as invoke).
export async function listenEvent<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return await listen<T>(event, (e) => cb(e.payload as T));
  } catch {
    return () => {};
  }
}

export function onTranscriptPartial(cb: (p: PartialTranscriptEvent) => void): Promise<() => void> {
  return listenEvent<PartialTranscriptEvent>(EVENT_TRANSCRIPT_PARTIAL, cb);
}

export function onReceipt(cb: (p: ReceiptEvent) => void): Promise<() => void> {
  return listenEvent<ReceiptEvent>(EVENT_RECEIPT, cb);
}

export function onPending(cb: (p: PendingEvent) => void): Promise<() => void> {
  return listenEvent<PendingEvent>(EVENT_PENDING, cb);
}
