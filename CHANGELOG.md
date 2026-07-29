# Changelog

All notable changes to WhimprFlow are documented in this file.

## [1.0.0] — 2026-07-29

### Added

- Local-first dictation with Whisper ASR (macOS / Windows / Linux)
- Optional cloud cleanup (OpenAI-compatible + Anthropic) behind license/trial
- Hub: dictionary, snippets, style, transforms, workflows, Voice Memory, privacy
- Flow Bar overlay, system tray, launch watchdog with safe-mode + rollback
- Model catalog download with SHA-256 verification
- Diagnostics export, crash reports (opt-in), SBOM + cargo-auditable release builds
- Settings import/export, history search/export, Voice Memory search
- Autostart on login, minimize-to-tray, audio input device picker
- Auto-punctuation + custom filler words
- Toast notifications, accessibility improvements, keyboard cheatsheet
- In-app changelog / “What’s new” after updates
- Linux CI (check / test / clippy) and Wayland hotkey guidance

### Known limitations

- Pure Wayland sessions: global hotkeys unavailable (see `docs/LINUX-WAYLAND.md`)
- Public installers require Apple / Windows code-signing secrets
- Clipboard history clear is Windows-specific (Win+V); macOS/Linux documented

## [Unreleased]

Placeholder for post-1.0.0 work.
