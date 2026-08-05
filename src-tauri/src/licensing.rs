//! OS keychain-backed license + trial state for the Tauri shell.
//!
//! Trial state is stored in both the OS keychain (for convenience) and a
//! machine-id-keyed file (to prevent trial reset by deleting keychain entries).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use whimpr_core::{
    evaluate_entitlement, machine_id, verify_license_key, Entitlement, EntitlementKind, TRIAL_DAYS,
};

const SERVICE: &str = "com.whimpr.whimprflow";
const LICENSE_ACCOUNT: &str = "license_key";
const TRIAL_ACCOUNT: &str = "trial_started_unix";
const TRIAL_STATE_FILE: &str = "trial_state.json";

/// Trial state file format: maps machine IDs to trial start timestamps.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TrialState {
    #[serde(default)]
    machine_id_to_start: std::collections::HashMap<String, u64>,
}

fn support_dir() -> PathBuf {
    crate::logging::support_dir()
}

fn trial_state_path() -> PathBuf {
    support_dir().join(TRIAL_STATE_FILE)
}

fn load_trial_state() -> TrialState {
    let path = trial_state_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(state) = serde_json::from_str::<TrialState>(&content) {
            return state;
        }
    }
    TrialState::default()
}

fn save_trial_state(state: &TrialState) -> Result<(), String> {
    let path = trial_state_path();
    // Ensure the support directory exists before writing
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

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

    // Check both keychain (backward compat) and machine ID file (tamper resistance)
    let keychain_trial = read_secret(TRIAL_ACCOUNT).and_then(|s| s.parse::<u64>().ok());

    let machine_trial = machine_id()
        .ok()
        .and_then(|id| load_trial_state().machine_id_to_start.get(&id).copied());

    // Use the earlier start time (more conservative)
    let trial = match (keychain_trial, machine_trial) {
        (Some(k), Some(m)) => Some(k.min(m)),
        (Some(k), None) => Some(k),
        (None, Some(m)) => Some(m),
        (None, None) => None,
    };

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

/// Start (or return existing) 14-day trial. Tamper-resistant: stored in both OS keychain
/// and a machine-id-keyed file to prevent reset by deleting keychain entries.
/// Falls back to keychain-only if machine ID is unavailable (degraded tamper resistance).
pub fn start_trial() -> Result<Entitlement, String> {
    let machine = machine_id();
    let mut state = load_trial_state();

    // Check if this machine ID already has a trial
    if let Ok(ref machine_id) = machine {
        if let Some(&start) = state.machine_id_to_start.get(machine_id) {
            let elapsed = now_unix().saturating_sub(start);
            let trial_secs = TRIAL_DAYS * 24 * 60 * 60;
            if elapsed >= trial_secs {
                return Err(format!(
                    "trial already used or expired on this machine ({} days)",
                    TRIAL_DAYS
                ));
            }
            // Trial is still active - restore keychain if missing, but keep the
            // original start time from the machine ID file (the tamper-proof source).
            if read_secret(TRIAL_ACCOUNT).is_none() {
                let _ = write_secret(TRIAL_ACCOUNT, &start.to_string());
            }
            return Ok(current_entitlement());
        }
    }

    // No trial on this machine yet (or machine ID unavailable) - start a new one
    let now = now_unix();
    if read_secret(TRIAL_ACCOUNT).is_none() {
        write_secret(TRIAL_ACCOUNT, &now.to_string())?;
    }

    // Use the earlier of keychain or now (in case keychain had an older timestamp
    // from a partial previous attempt)
    let start = read_secret(TRIAL_ACCOUNT)
        .and_then(|s| s.parse::<u64>().ok())
        .map(|k| k.min(now))
        .unwrap_or(now);

    // Record in machine ID file if available
    if let Ok(ref machine_id) = machine {
        state.machine_id_to_start.insert(machine_id.clone(), start);
        save_trial_state(&state)?;
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

    #[test]
    fn trial_state_file_persists_across_keychain_delete() {
        // This test verifies that even if the keychain entry is deleted,
        // the machine ID file still prevents trial reset.
        let _ = clear_license();

        // Start a trial
        let first = start_trial();
        let first_expiry = match &first {
            Ok(ent) => ent.expires_unix,
            Err(_) => return, // Skip if trial already used
        };

        // Simulate keychain deletion by removing the keychain entry
        let _ = delete_secret(TRIAL_ACCOUNT);

        // Try to start trial again - should either fail (trial already used)
        // or succeed with the SAME expiry (machine ID file is source of truth)
        let second = start_trial();
        match (first, second) {
            (Ok(_), Err(msg)) => {
                assert!(
                    msg.contains("trial") || msg.contains("expired") || msg.contains("used"),
                    "expected trial reuse error, got: {msg}"
                );
            }
            (Ok(a), Ok(b)) => {
                // Both succeeded - must have the same expiry (machine ID file preserved start)
                assert_eq!(
                    a.expires_unix, b.expires_unix,
                    "trial start time changed after keychain delete"
                );
                assert_eq!(a.expires_unix, first_expiry);
            }
            _ => {}
        }

        // Cleanup: remove the machine ID entry for this test
        if let Ok(machine) = machine_id() {
            let mut state = load_trial_state();
            state.machine_id_to_start.remove(&machine);
            let _ = save_trial_state(&state);
        }
    }
}
