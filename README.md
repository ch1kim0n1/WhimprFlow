# WhimprFlow

A **local-first, cross-platform voice dictation app** — hold a key, speak, and clean text lands wherever your cursor is. Speech is transcribed on-device with Whisper and cleaned up (filler removal, self-corrections, punctuation, lists/newlines) by a local LLM, with an optional cloud path. It re-creates the workflow of a Wispr-Flow-style dictation tool from scratch, with its own name, palette, and code.

> ⚠️ **This is a proof of concept, vibe-coded in a few hours.** It works and the core loop is real, but it is rough and needs a lot of polish, testing, and hardening before it's anything like production quality. Treat it as a starting point, not a finished product.

---

## Platform status

| Platform | Status |
|----------|--------|
| **macOS 14+** | **Built and working** — developed and tested locally (Apple Silicon). This is the path that actually runs. |
| **Windows 10/11** | **Built but UNTESTED.** The Windows platform layer (low-level keyboard hook, SendInput paste, foreground-app detection, the dictation pipeline) is written and wired up — but it has **never been compiled or run on Windows**. It is `cfg(target_os = "windows")` code authored on macOS and will almost certainly need fixes before it builds and runs. Do not assume it works. |

**Disclaimer:** Windows support is present in the source so the codebase is cross-platform in intent, but it is unverified. If you're on Windows, expect to debug the Win32 glue (`src-tauri/src/win.rs`) before anything happens.

---

## What's in it

- **On-device ASR** — Whisper (via `whisper.cpp`), running on the GPU. Ships a small English model by default; larger models are auto-preferred if present.
- **Local LLM cleanup** — Qwen3-4B-Instruct (via `llama.cpp`) runs as a separate worker process and cleans the transcript: removes fillers, resolves spoken self-corrections ("meet at 2… no wait, 3" → "3"), applies spoken punctuation, and formats lists/paragraphs. Deterministic gates guard against over-editing, with a raw-transcript fallback.
- **Optional cloud cleanup** — OpenAI (default) / Anthropic, behind one trait. Keys are stored in the OS keychain (macOS Keychain / Windows Credential Manager), **never in a file**.
- **Floating pill UI** — a small always-on-top bar showing idle / recording / processing states.
- **Personal dictionary + auto-learn** — teach it names and terms; on macOS a post-paste Accessibility observer watches for a one-word correction and learns it automatically (conservative filters to avoid junk). *Auto-learn capture is macOS-only so far.*
- **Usage stats** — words dictated, words-per-minute, day streak, time saved, 7-day activity, all stored locally.

## Architecture

Tauri v2 (Rust core + React/TypeScript webviews). Platform-agnostic logic lives in `crates/whimpr-core` (state machine, cleanup prompts/gates, dictionary, stats). ASR, audio, and the LLM worker are separate crates. The Tauri app in `src-tauri/` hosts the UI and wires the native hotkey/injection per platform (`hotkey.rs` on macOS, `win.rs` on Windows).

```
crates/
  whimpr-core/       state machine, cleanup (prompts/gates/levels), dictionary, stats
  whimpr-asr/        Whisper ASR
  whimpr-audio/      mic capture + resampling
  whimpr-cleanup/    OpenAI / Anthropic cloud providers
  whimpr-llm-worker/ local llama.cpp cleanup worker (separate process)
src-tauri/           Tauri shell: hotkey/paste/autolearn (macOS), win.rs (Windows)
ui/                  React Hub + overlay pill
docs/                spec, architecture notes, research
```

## Build (macOS)

Requires Rust (stable), Node + pnpm, and the Xcode command-line tools.

```bash
cd ui && pnpm install && cd ..
# Dev:
./dev.sh
# Or a signed .app bundle:
ui/node_modules/.bin/tauri build --bundles app
```

Models are **not** committed (they're multi-GB). Place them under
`~/Library/Application Support/WhimprFlow/models/` (macOS) —
a Whisper `ggml-*.en.bin` and a Qwen GGUF for local cleanup.

## Notes & disclaimers

- **Not affiliated with, endorsed by, or connected to Wispr Flow or any other product.** WhimprFlow is an independent, from-scratch reimplementation of the dictation workflow, with its own name, branding, colors, strings, and code. No third-party code or assets are included.
- **Proof of concept.** Rushed, under-tested, and missing plenty (Windows is unverified, auto-learn is macOS-only and conservative, no installer/notarization pipeline, error handling is thin). Contributions and fixes welcome.
- **Privacy.** ASR and default cleanup run on-device. Cloud cleanup is opt-in and only sends the transcript (not audio) to the provider you choose. API keys never touch disk in plaintext.

## License

MIT — see [LICENSE](LICENSE).
