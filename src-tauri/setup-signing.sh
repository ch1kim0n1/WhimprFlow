#!/bin/bash
# Create + install a self-signed code-signing certificate for WhimprFlow.
#
# WHY: Tauri's default `signingIdentity: "-"` (ad-hoc) produces a different
# code signature every build, so macOS TCC (the privacy permission database)
# can't stably identify the app and re-prompts for Accessibility / Input
# Monitoring / Microphone on every launch (or after every rebuild). A
# self-signed cert with a stable Common Name gives a stable *designated
# requirement* (`certificate = "WhimprFlow Dev"`), so TCC persists grants
# across rebuilds ON THIS MACHINE. It is NOT for distribution  -  end users
# would still see a Gatekeeper warning. For distributable builds, use a
# paid Developer ID Application certificate instead.
#
# Run once:  bash src-tauri/setup-signing.sh
# Re-running is a no-op if the cert already exists in the login keychain.
#
# After this, `tauri build` (and `tauri dev`'s bundled build) signs the .app
# with "WhimprFlow Dev" automatically  -  tauri.conf.json points at it.
set -euo pipefail

CN="WhimprFlow Dev"
KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 1. Already installed? `security find-identity` lists codesigning identities.
if security find-identity -v -p codesigning "$KEYCHAIN" 2>/dev/null | grep -q "\"$CN\""; then
  echo "[signing] \"$CN\" already present in $KEYCHAIN  -  nothing to do."
  exit 0
fi

echo "[signing] Creating self-signed code-signing certificate \"$CN\"..."
echo "[signing]   (work dir: $WORK)"

# 2. Generate a self-signed cert with the codeSigning extended key usage.
openssl req -x509 -newkey rsa:2048 -sha256 \
  -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
  -days 3650 -nodes \
  -subj "/CN=$CN" \
  -addext "extendedKeyUsage=codeSigning" \
  -addext "keyUsage=digitalSignature" \
  2>/dev/null

# 3. Package as a .p12 (the import format the keychain expects).
P12_PW="whimpr-dev"
openssl pkcs12 -export -out "$WORK/cert.p12" \
  -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
  -name "$CN" -passout "pass:$P12_PW" 2>/dev/null

# 4. Import into the login keychain, granting /usr/bin/codesign access to the key.
security import "$WORK/cert.p12" -k "$KEYCHAIN" -P "$P12_PW" -T /usr/bin/codesign

# 5. Allow codesign to use the private key without prompting on every build.
#    This step needs the keychain password (it modifies the key's ACL). Read it
#    securely so it never lands in shell history.
echo
echo "[signing] To let codesign use the key without prompting each build,"
echo "[signing] enter your LOGIN KEYCHAIN password (the one you use to log in):"
read -s KEYCHAIN_PW
echo
security set-keypartitionlist -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PW" "$KEYCHAIN" >/dev/null
unset KEYCHAIN_PW

# 6. Verify.
if security find-identity -v -p codesigning "$KEYCHAIN" 2>/dev/null | grep -q "\"$CN\""; then
  echo "[signing] Done. \"$CN\" installed and trusted for code signing."
  echo "[signing] tauri.conf.json already references it  -  just run:  bash dev.sh build"
  echo "[signing]   (or: cd ui && npm exec tauri -- build)"
else
  echo "[signing] ERROR: cert import reported success but the identity is not"
  echo "[signing] listed by 'security find-identity'. Open Keychain Access and"
  echo "[signing] check the login keychain for \"$CN\"."
  exit 1
fi
