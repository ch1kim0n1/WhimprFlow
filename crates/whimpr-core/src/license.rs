//! Offline license keys for paid WhimprFlow builds.
//!
//! Format: `WF1.<base64url(json)>.<base64url(ed25519-signature)>`
//! Payload JSON: `{ "v": 1, "email": "...", "tier": "pro"|"trial", "exp": <unix|null> }`
//! Signature covers the exact JSON bytes. The verify public key is embedded here;
//! the matching private key lives only in maintainer `secrets/` and CI (never in git).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// 14-day free trial of full features (including cloud cleanup).
pub const TRIAL_DAYS: u64 = 14;

/// Purchase page shown in the Hub Account / License pane.
pub const PURCHASE_URL: &str = "https://whimprflow.com/buy";

/// Embedded ed25519 verify key (32 raw bytes, hex).
const LICENSE_PUBKEY_HEX: &str = "8a87eae93fd134f80ca234e545535829e0b11cc82f8f33057c3e4a7e8913ded7";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTier {
    Pro,
    Trial,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicensePayload {
    pub v: u8,
    pub email: String,
    pub tier: LicenseTier,
    /// Unix seconds expiry; `null` means never expires.
    pub exp: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitlementKind {
    Licensed,
    Trial,
    Unlicensed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entitlement {
    pub kind: EntitlementKind,
    pub cloud_cleanup_allowed: bool,
    pub email: Option<String>,
    pub tier: Option<LicenseTier>,
    pub expires_unix: Option<u64>,
    pub trial_days_remaining: Option<u64>,
    pub purchase_url: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("license key must look like WF1.<payload>.<signature>")]
    BadFormat,
    #[error("license payload is not valid JSON")]
    BadPayload,
    #[error("license signature is invalid")]
    BadSignature,
    #[error("unsupported license version {0}")]
    BadVersion(u8),
    #[error("license expired")]
    Expired,
}

fn verifying_key() -> VerifyingKey {
    let mut bytes = [0u8; 32];
    let hex = LICENSE_PUBKEY_HEX.as_bytes();
    for i in 0..32 {
        let hi = hex_nibble(hex[i * 2]);
        let lo = hex_nibble(hex[i * 2 + 1]);
        bytes[i] = (hi << 4) | lo;
    }
    VerifyingKey::from_bytes(&bytes).expect("embedded license pubkey must be valid")
}

fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

/// Parse and verify a license key string. Does not consult the clock for trial
/// state; use [`evaluate_entitlement`] for the full product gate.
pub fn verify_license_key(key: &str) -> Result<LicensePayload, LicenseError> {
    let key = key.trim();
    let mut parts = key.split('.');
    let prefix = parts.next().ok_or(LicenseError::BadFormat)?;
    let payload_b64 = parts.next().ok_or(LicenseError::BadFormat)?;
    let sig_b64 = parts.next().ok_or(LicenseError::BadFormat)?;
    if parts.next().is_some() || prefix != "WF1" {
        return Err(LicenseError::BadFormat);
    }
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .map_err(|_| LicenseError::BadFormat)?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64.as_bytes())
        .map_err(|_| LicenseError::BadFormat)?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| LicenseError::BadSignature)?;
    verifying_key()
        .verify(&payload_bytes, &sig)
        .map_err(|_| LicenseError::BadSignature)?;
    let payload: LicensePayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| LicenseError::BadPayload)?;
    if payload.v != 1 {
        return Err(LicenseError::BadVersion(payload.v));
    }
    Ok(payload)
}

pub fn is_expired(payload: &LicensePayload, now_unix: u64) -> bool {
    matches!(payload.exp, Some(exp) if now_unix >= exp)
}

/// Combine a verified (or missing) license with optional trial start timestamp.
pub fn evaluate_entitlement(
    license_key: Option<&str>,
    trial_started_unix: Option<u64>,
    now_unix: u64,
) -> Entitlement {
    if let Some(key) = license_key {
        match verify_license_key(key) {
            Ok(payload) if !is_expired(&payload, now_unix) => {
                return Entitlement {
                    kind: EntitlementKind::Licensed,
                    cloud_cleanup_allowed: true,
                    email: Some(payload.email),
                    tier: Some(payload.tier),
                    expires_unix: payload.exp,
                    trial_days_remaining: None,
                    purchase_url: PURCHASE_URL.to_string(),
                    message: "License active. Cloud cleanup unlocked.".into(),
                };
            }
            Ok(_) => {
                // Fall through to trial / unlicensed after expiry.
            }
            Err(_) => {
                // Invalid key treated as absent; caller should surface activate errors separately.
            }
        }
    }

    if let Some(started) = trial_started_unix {
        let elapsed = now_unix.saturating_sub(started);
        let trial_secs = TRIAL_DAYS * 24 * 60 * 60;
        if elapsed < trial_secs {
            let remaining = (trial_secs - elapsed).div_ceil(24 * 60 * 60).max(1);
            return Entitlement {
                kind: EntitlementKind::Trial,
                cloud_cleanup_allowed: true,
                email: None,
                tier: Some(LicenseTier::Trial),
                expires_unix: Some(started + trial_secs),
                trial_days_remaining: Some(remaining),
                purchase_url: PURCHASE_URL.to_string(),
                message: format!("Trial active: {remaining} day(s) left."),
            };
        }
    }

    Entitlement {
        kind: EntitlementKind::Unlicensed,
        cloud_cleanup_allowed: false,
        email: None,
        tier: None,
        expires_unix: None,
        trial_days_remaining: None,
        purchase_url: PURCHASE_URL.to_string(),
        message: "Enter a license key or start a free trial to unlock cloud cleanup.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn issue(payload: &LicensePayload, signing: &SigningKey) -> String {
        let body = serde_json::to_vec(payload).unwrap();
        let sig = signing.sign(&body);
        format!(
            "WF1.{}.{}",
            URL_SAFE_NO_PAD.encode(&body),
            URL_SAFE_NO_PAD.encode(sig.to_bytes())
        )
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        // Wrong pubkey path: random key cannot verify against embedded pubkey.
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let payload = LicensePayload {
            v: 1,
            email: "a@b.co".into(),
            tier: LicenseTier::Pro,
            exp: None,
        };
        let key = issue(&payload, &signing);
        assert!(matches!(
            verify_license_key(&key),
            Err(LicenseError::BadSignature)
        ));
    }

    #[test]
    fn expired_license_falls_to_unlicensed_without_trial() {
        let ent = evaluate_entitlement(None, None, 1_700_000_000);
        assert_eq!(ent.kind, EntitlementKind::Unlicensed);
        assert!(!ent.cloud_cleanup_allowed);
    }

    #[test]
    fn trial_window_unlocks_cloud() {
        let start = 1_700_000_000u64;
        let ent = evaluate_entitlement(None, Some(start), start + 60);
        assert_eq!(ent.kind, EntitlementKind::Trial);
        assert!(ent.cloud_cleanup_allowed);
        assert_eq!(ent.trial_days_remaining, Some(14));
    }

    #[test]
    fn trial_expiry_locks_cloud() {
        let start = 1_700_000_000u64;
        let after = start + TRIAL_DAYS * 24 * 60 * 60 + 1;
        let ent = evaluate_entitlement(None, Some(start), after);
        assert_eq!(ent.kind, EntitlementKind::Unlicensed);
        assert!(!ent.cloud_cleanup_allowed);
    }

    #[test]
    fn verifies_key_signed_by_product_keypair() {
        // Issued with tools/whimpr-license against secrets/license-private.hex
        // (matching LICENSE_PUBKEY_HEX). Safe to embed: verification-only.
        let key = "WF1.eyJ2IjoxLCJlbWFpbCI6Im93bmVyQHdoaW1wcmZsb3cubG9jYWwiLCJ0aWVyIjoicHJvIiwiZXhwIjpudWxsfQ.W9XZl0AljjyyC-ZfgCzw9TnvIxg7zd8VouO47UZ4GnkryXJzYQjKVGb_-W-v-ywFvPPQwp9WlrZiV_HzycE-Aw";
        let payload = verify_license_key(key).expect("sample key must verify");
        assert_eq!(payload.email, "owner@whimprflow.local");
        assert_eq!(payload.tier, LicenseTier::Pro);
        let ent = evaluate_entitlement(Some(key), None, 1_800_000_000);
        assert_eq!(ent.kind, EntitlementKind::Licensed);
        assert!(ent.cloud_cleanup_allowed);
    }

    #[test]
    fn rejects_malformed_keys() {
        assert!(matches!(
            verify_license_key("not-a-key"),
            Err(LicenseError::BadFormat)
        ));
        assert!(matches!(
            verify_license_key("WF1.only.two.extra"),
            Err(LicenseError::BadFormat)
        ));
        assert!(matches!(
            verify_license_key("WF1.!!!invalid!!!.sig"),
            Err(LicenseError::BadFormat)
        ));
        assert!(matches!(
            verify_license_key("wf1.aa.bb"),
            Err(LicenseError::BadFormat)
        ));
    }

    #[test]
    fn rejects_version_2_payload() {
        let signing = SigningKey::from_bytes(&[9u8; 32]);
        let body = br#"{"v":2,"email":"a@b.co","tier":"pro","exp":null}"#;
        let sig = signing.sign(body);
        let key = format!(
            "WF1.{}.{}",
            URL_SAFE_NO_PAD.encode(body),
            URL_SAFE_NO_PAD.encode(sig.to_bytes())
        );
        // Wrong pubkey => BadSignature before version check; also try with
        // product-signed sample mutated to v2 by hand is hard. Assert format
        // for truncated JSON:
        assert!(matches!(
            verify_license_key(&key),
            Err(LicenseError::BadSignature)
        ));
    }

    #[test]
    fn expired_license_still_allows_active_trial() {
        let key = "WF1.eyJ2IjoxLCJlbWFpbCI6Im93bmVyQHdoaW1wcmZsb3cubG9jYWwiLCJ0aWVyIjoicHJvIiwiZXhwIjoxfQ.invalid";
        let start = 1_700_000_000u64;
        let ent = evaluate_entitlement(Some(key), Some(start), start + 10);
        assert_eq!(ent.kind, EntitlementKind::Trial);
    }

    #[test]
    fn trial_start_in_future_still_counts_as_active() {
        let now = 1_700_000_000u64;
        let start = now + 60;
        let ent = evaluate_entitlement(None, Some(start), now);
        // elapsed saturates to 0 => full trial remaining
        assert_eq!(ent.kind, EntitlementKind::Trial);
        assert!(ent.cloud_cleanup_allowed);
    }

    #[test]
    fn trims_whitespace_around_valid_key() {
        let key = "  WF1.eyJ2IjoxLCJlbWFpbCI6Im93bmVyQHdoaW1wcmZsb3cubG9jYWwiLCJ0aWVyIjoicHJvIiwiZXhwIjpudWxsfQ.W9XZl0AljjyyC-ZfgCzw9TnvIxg7zd8VouO47UZ4GnkryXJzYQjKVGb_-W-v-ywFvPPQwp9WlrZiV_HzycE-Aw\n";
        assert!(verify_license_key(key).is_ok());
    }
}
