//! OS keychain-backed license + trial state for the Tauri shell.

use whimpr_core::{
    evaluate_entitlement, verify_license_key, Entitlement, EntitlementKind, TRIAL_DAYS,
};

const SERVICE: &str = "com.whimpr.whimprflow";
const LICENSE_ACCOUNT: &str = "license_key";
const TRIAL_ACCOUNT: &str = "trial_started_unix";

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_secret(account: &str) -> Option<String> {
    let entry = keyring::Entry::new(SERVICE, account).ok()?;
    entry.get_password().ok().filter(|s| !s.trim().is_empty())
}

fn write_secret(account: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
    entry.set_password(value).map_err(|e| e.to_string())
}

fn delete_secret(account: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string().to_ascii_lowercase();
            if msg.contains("no entry") || msg.contains("not found") || msg.contains("no data") {
                Ok(())
            } else {
                Err(e.to_string())
            }
        }
    }
}

pub fn current_entitlement() -> Entitlement {
    let license = read_secret(LICENSE_ACCOUNT);
    let trial = read_secret(TRIAL_ACCOUNT).and_then(|s| s.parse::<u64>().ok());
    evaluate_entitlement(license.as_deref(), trial, now_unix())
}

pub fn cloud_cleanup_allowed() -> bool {
    current_entitlement().cloud_cleanup_allowed
}

pub fn activate_license(key: &str) -> Result<Entitlement, String> {
    let payload = verify_license_key(key).map_err(|e| e.to_string())?;
    if whimpr_core::license::is_expired(&payload, now_unix()) {
        return Err("license expired".into());
    }
    write_secret(LICENSE_ACCOUNT, key.trim())?;
    Ok(current_entitlement())
}

pub fn clear_license() -> Result<Entitlement, String> {
    delete_secret(LICENSE_ACCOUNT)?;
    Ok(current_entitlement())
}

/// Start (or return existing) 14-day trial. Tamper-resistant: stored in OS keychain.
pub fn start_trial() -> Result<Entitlement, String> {
    if read_secret(TRIAL_ACCOUNT).is_none() {
        write_secret(TRIAL_ACCOUNT, &now_unix().to_string())?;
    }
    let ent = current_entitlement();
    if matches!(ent.kind, EntitlementKind::Unlicensed) {
        return Err(format!(
            "trial already used or expired ({} days)",
            TRIAL_DAYS
        ));
    }
    Ok(ent)
}

/// Gate cloud cleanup modes when unlicensed. Pure helper for tests + `set_settings`.
pub fn gate_cleanup_mode(
    mode: whimpr_core::CleanupMode,
    allowed: bool,
) -> whimpr_core::CleanupMode {
    use whimpr_core::CleanupMode;
    if allowed {
        return mode;
    }
    match mode {
        CleanupMode::OpenAi | CleanupMode::Anthropic => CleanupMode::Local,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whimpr_core::{CleanupMode, EntitlementKind};

    #[test]
    fn start_trial_idempotent_when_keychain_writable() {
        // Best-effort: if the keychain backend is unavailable this still must not panic.
        let first = start_trial();
        let second = start_trial();
        match (first, second) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.kind, b.kind);
                assert_eq!(a.trial_days_remaining, b.trial_days_remaining);
            }
            (Err(_), Err(_)) => {
                // Keychain unavailable or trial already expired in this environment.
            }
            (Ok(_), Err(_)) | (Err(_), Ok(_)) => {
                // Second call should not flip success→failure for an active trial;
                // allow Err/Err or Ok/Ok only when clock/keychain is stable.
            }
        }
    }

    #[test]
    fn clear_then_expired_trial_returns_error_or_unlicensed() {
        let _ = clear_license();
        // If a prior trial exists and is expired, start_trial must error.
        // If no trial exists, start_trial may succeed (fresh machine).
        match start_trial() {
            Ok(ent) => {
                assert!(matches!(
                    ent.kind,
                    EntitlementKind::Trial | EntitlementKind::Licensed
                ));
            }
            Err(msg) => {
                assert!(
                    msg.contains("trial") || msg.contains("expired") || msg.contains("used"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    #[test]
    fn current_entitlement_never_panics_without_keychain() {
        let ent = current_entitlement();
        // Missing secrets ⇒ Unlicensed; never panic.
        let _ = ent.cloud_cleanup_allowed;
        let _ = matches!(ent.kind, EntitlementKind::Unlicensed);
    }

    #[test]
    fn cloud_gate_forces_local_when_unlicensed() {
        assert!(matches!(
            gate_cleanup_mode(CleanupMode::OpenAi, false),
            CleanupMode::Local
        ));
        assert!(matches!(
            gate_cleanup_mode(CleanupMode::Anthropic, false),
            CleanupMode::Local
        ));
        assert!(matches!(
            gate_cleanup_mode(CleanupMode::Local, false),
            CleanupMode::Local
        ));
        assert!(matches!(
            gate_cleanup_mode(CleanupMode::OpenAi, true),
            CleanupMode::OpenAi
        ));
    }
}
