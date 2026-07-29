# WhimprFlow v1.0.0 — release notes draft

## Highlights

- Local Whisper dictation with optional cloud cleanup (licensed / trial)
- Hub for dictionary, snippets, style, workflows, Voice Memory, privacy
- System tray, autostart, minimize-to-tray
- Model download with SHA-256 verification; in-Hub model management
- Launch watchdog with safe mode and one-click rollback
- Diagnostics export, SBOM, cargo-auditable builds
- Accessibility: focus rings, skip link, aria-live status regions

## Platform support

| OS | Status |
| --- | --- |
| macOS 14+ | Supported (signing required for distribution) |
| Windows 10/11 | Supported (signing required for distribution) |
| Linux (X11) | Supported; Wayland global hotkeys limited — see `docs/LINUX-WAYLAND.md` |

## Install

1. Download the installer for your OS from [GitHub Releases](https://github.com/ch1kim0n1/WhimprFlow/releases).
2. macOS: open the DMG / app; grant Accessibility + Microphone when prompted.
3. Windows: run the NSIS/MSI installer; grant mic access when prompted.
4. Linux: install the AppImage or `.deb`; install `xdotool` for paste.

## Known limitations

- Pure Wayland: no global PTT until portal shortcuts are wired.
- Public builds need Apple / Windows code-signing secrets in CI.
- Clipboard history clear applies to Windows Win+V; macOS/Linux documented separately.
