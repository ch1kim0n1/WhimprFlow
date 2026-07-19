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
export interface HistoryItem {
  ts_unix: number;
  text: string;
  app: string | null;
  words: number;
}

export async function getHistory(): Promise<HistoryItem[]> {
  try {
    return await invoke<HistoryItem[]>("get_history");
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
