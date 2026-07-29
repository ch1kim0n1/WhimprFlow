# WhimprFlow troubleshooting

## No audio / mic not detected

1. Open Hub > Settings > Permissions and grant Microphone.
2. Confirm the OS default input device is the mic you expect.
3. Use Hub > Onboarding (or License/Help) **Test microphone** if available; peak level should move when you speak.
4. Quit WhimprFlow fully and reopen after changing privacy grants.

## Fn / Right Ctrl key not working

| Platform | Default PTT |
| --- | --- |
| macOS | Hold Fn (Accessibility required) |
| Windows | Hold Right Ctrl |
| Linux (X11/XWayland) | Hold Right Ctrl; needs `xdotool` |

If the key does nothing: grant Accessibility (macOS), confirm no other app grabbed the key, on Linux see [LINUX-WAYLAND.md](./LINUX-WAYLAND.md).

## Paste not landing in the target app

1. Click into the destination field before releasing PTT.
2. On macOS, Accessibility must be granted for synthetic Cmd+V.
3. On Linux, install `xdotool`.
4. Some elevated/admin apps block synthetic input; paste manually from history.

## Cloud cleanup failing

1. Hub > License: need Licensed or Trial for OpenAI/Anthropic.
2. Check API key in Settings and network access.
3. Invalid/private `openai_base_url` values are rejected (SSRF guard).
4. On HTTP failure WhimprFlow pastes the **raw** transcript and emits `whimpr://cloud/unavailable`.

## Model download failing / corrupt

1. Use Hub download (SHA-256 verified). Do not hand-copy truncated files.
2. If dictation crashes immediately, delete the model under the app `models/` folder and re-download.
3. Check disk space (~75 MB tiny … ~488 MB small).

## App won't launch after update

After 3 consecutive failed launches the watchdog enters safe mode. Reinstall the previous release from GitHub Releases, then export diagnostics and contact support.

## License key rejected

Keys look like `WF1.<payload>.<signature>`. Whitespace is trimmed. Expired keys fall back to trial/unlicensed. Re-issue via support if lost.

## Trial expired unexpectedly

Trial start is stored in the OS keychain. Restoring a machine backup can restore an old start time. Contact support with your purchase email for a license key.

## Support

- Email: support@whimprflow.com
- Export diagnostics: Settings > Data backup / Help
- Privacy / Terms / EULA: [docs/legal/](./legal/)
