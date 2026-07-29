# Security posture

WhimprFlow is a local-first, single-user desktop app. This document is a lightweight
self-audit of what is actually true about how it handles data and permissions.
Update it when any of the following changes.

## What never leaves the device

- Audio, transcripts, dictionary, snippets, and usage stats are stored only in
  local JSON (or encrypted) files under the OS per-user app-support directory
  (`~/Library/Application Support/WhimprFlow` on macOS, `%APPDATA%\WhimprFlow`
  on Windows, `$XDG_CONFIG_HOME/WhimprFlow` or `~/.config/WhimprFlow` on Linux).
- There is no telemetry, analytics, or crash-reporting pipeline by default.
  Nothing is sent anywhere unless the user explicitly turns on cloud cleanup
  (and holds a license or active trial).

## What does leave the device (opt-in only)

- **Cloud cleanup mode** (OpenAI or Anthropic): the raw transcript, a small
  vocabulary hint, and up to ~200 chars of on-screen context are sent to
  whichever provider the user selected, over HTTPS (`reqwest` with rustls,
  redirects disabled, User-Agent set, private/RFC1918 base URLs rejected unless
  loopback is explicitly used).
- **Local mode** (default) and **Raw mode** send nothing over the network.

## Secrets

- API keys live only in the OS keychain (macOS Keychain / Windows Credential
  Manager / Linux Secret Service via the `keyring` crate), never in a plaintext
  file, never logged.
- License keys and trial start timestamps also live in the keychain.
- Voice Memory is AES-256-GCM encrypted; the key is in the keychain.

## Input handling

- IPC frames (`whimpr-ipc`) reject an oversize length prefix before allocating
  (property-tested).
- Dictionary words / mishears and snippet triggers / expansions are truncated to
  fixed character caps on set.

## Permissions

- **Accessibility** (macOS): required for the global Fn-key tap and for posting
  the paste keystroke into other apps.
- **Microphone**: required to record.
- Linux: X11/XWayland global grab; see `docs/LINUX-WAYLAND.md`.

## CI / verified platforms

- GitHub Actions builds and tests the Rust workspace on **macOS, Windows, and
  Ubuntu** (`ubuntu-latest`), including `linux.rs`.
- `cargo audit`, `npm audit --audit-level=high`, and `gitleaks` run in CI.
- Release workflow requires platform code-signing secrets and attaches an SBOM.

## Known gaps (tracked here, not hidden)

- Apple/Windows code-signing certificates must be purchased and uploaded before
  strangers can install without Gatekeeper/SmartScreen warnings
  (`docs/release/CODE_SIGNING.md`).
- Native Wayland global shortcuts are not implemented yet.
