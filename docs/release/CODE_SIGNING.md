# Code signing for WhimprFlow releases

Updater signing keys are already generated and stored as GitHub Actions secrets
`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The matching
public key is embedded in `src-tauri/tauri.conf.json`.

Platform certificates still require purchase/enrollment outside this repo.

## Apple (macOS Gatekeeper + notarization)

1. Enroll in the [Apple Developer Program](https://developer.apple.com/programs/).
2. Create a **Developer ID Application** certificate in Certificates, Identifiers & Profiles.
3. Export it as a `.p12` from Keychain Access.
4. Create an app-specific password for your Apple ID (appleid.apple.com).
5. Base64-encode the `.p12` (no line breaks):

   ```bash
   base64 -i DeveloperID.p12 | pbcopy   # macOS
   ```

6. Set GitHub Actions secrets:

   | Secret | Value |
   | --- | --- |
   | `APPLE_CERTIFICATE` | base64 `.p12` |
   | `APPLE_CERTIFICATE_PASSWORD` | `.p12` password |
   | `APPLE_ID` | Apple ID email |
   | `APPLE_PASSWORD` | app-specific password |
   | `APPLE_TEAM_ID` | 10-character Team ID |
   | `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |

## Windows (SmartScreen)

1. Buy a code-signing certificate (EV preferred for reputation).
2. Export as `.pfx` / PKCS#12.
3. Base64-encode it and set:

   | Secret | Value |
   | --- | --- |
   | `WINDOWS_CERTIFICATE` | base64 `.pfx` |
   | `WINDOWS_CERTIFICATE_PASSWORD` | `.pfx` password |

## Upload helper

With `gh` authenticated:

```powershell
.\scripts\set-signing-secrets.ps1
```

(Interactive prompts for Apple/Windows materials. Tauri updater secrets are already set.)

## Verify

1. Tag `v1.0.0` (or newer) and push the tag.
2. Confirm the Release workflow is green on macOS and Windows.
3. Download the `.dmg` / `.msi` on clean VMs: Gatekeeper and SmartScreen should accept them.
4. Confirm `https://github.com/ch1kim0n1/WhimprFlow/releases/latest/download/latest.json` is public.
