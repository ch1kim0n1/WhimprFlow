# WhimprFlow Privacy Policy

**Last updated:** 2026-07-29

## Summary

WhimprFlow is local-first. Your microphone audio is processed on your device for speech recognition. We do not operate a WhimprFlow telemetry or crash-reporting backend by default.

## What stays on your device

- Microphone audio used for dictation
- Whisper model files you download
- Settings, dictionary, snippets, workflows, history, and notes stored under the app data directory
- Voice Memory (encrypted at rest with a key kept in the OS keychain)
- Optional local LLM cleanup worker

## OS keychain

WhimprFlow stores the following in the operating system credential store (macOS Keychain, Windows Credential Manager, or Linux Secret Service):

- Cloud provider API keys you enter
- Voice Memory encryption key
- License key and trial start timestamp (when used)

On Windows, Credential Manager entries are per-user. Encryption-at-rest depends on the OS and device configuration (for example BitLocker). Other apps running as your user may be able to read Credential Manager entries on some configurations; treat a logged-in session as trusted.

## What may leave your device

Only when you enable a cloud cleanup engine (OpenAI-compatible or Anthropic) and you are licensed or in an active trial:

- The raw transcript text
- Optional short screen/app context (roughly a couple hundred characters) if Context Capsule is enabled
- Your requests go to the provider base URL you configure (defaults to that provider's API)

We do not receive those API calls; they go to the provider you chose. Review that provider's privacy policy and DPA if you process personal data (especially in the EU).

## Auto-updater

The app may contact GitHub Releases to fetch `latest.json` and download signed update packages. Those requests go to GitHub's infrastructure.

## Model download

If you use the in-app model download, the app fetches the Whisper model from the configured Hugging Face mirror and verifies a SHA-256 checksum.

## Retention

Dictation history retention is controlled in Settings > Privacy. You can clear stored transcript text. License and trial state remain in the keychain until you remove them.

## Your rights (GDPR-style)

Because data is local, you control it:

- Export: use Hub export/backup features for local stores and Voice Memory
- Erasure: delete app data directories, clear keychain entries via the app where available, and uninstall

For privacy questions: support@whimprflow.com

## Children

WhimprFlow is not directed at children under 16.

## Changes

We may update this policy by changing this document. Material changes will bump the "Last updated" date.
