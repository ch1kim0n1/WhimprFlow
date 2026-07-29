# WhimprFlow architecture

## Crate graph

| Crate | Role |
| --- | --- |
| `whimpr-core` | State machine, settings, dictionary, stats, license verify, cleanup prompts/gates |
| `whimpr-asr` | Whisper (`whisper-rs`) transcription |
| `whimpr-audio` | Microphone capture via cpal + resample to 16 kHz |
| `whimpr-cleanup` | Hardened HTTP cloud providers (OpenAI-compatible, Anthropic) |
| `whimpr-ipc` | Shared IPC types / frame limits |
| `whimpr-llm-worker` | Local `llama.cpp` cleanup worker process |
| `whimpr-sidecar` | Optional native helper surface (legacy / future) |
| `whimpr-tauri` (`src-tauri`) | Desktop shell: hotkeys, paste, tray, Hub IPC, licensing store |

The React Hub and Flow Bar live under `ui/`.

## Data flow

```
PTT key down → audio capture → Whisper ASR → cleanup (raw / local / cloud)
  → validation gates → paste at cursor → history / stats / Voice Memory
```

1. Platform hotkey layer (`hotkey.rs` / `win.rs` / `linux.rs`) starts capture.
2. Samples are resampled to 16 kHz and transcribed on-device.
3. Optional cleanup runs; failures fall back to the raw transcript.
4. Text is pasted via clipboard + synthetic paste (or platform insert path).
5. Receipts and history update the Hub over Tauri events / commands.

## Design decisions

- **Local-first:** ASR always runs on-device; cloud cleanup is optional and gated by license/trial.
- **IPC frame limits:** Large payloads are bounded so a misbehaving frontend cannot OOM the shell.
- **License verification:** Offline ed25519 verification in `whimpr-core`; no phone-home for entitlement checks.
- **Hardened HTTP:** Cloud calls use a no-redirect, SSRF-aware client in `whimpr-cleanup::http`.
- **Crash loops:** Launch watchdog enters safe mode after three failed starts and can roll back via the updater.
