//! Machine fingerprinting to prevent trial reset by deleting keychain entries.
//!
//! Generates a stable machine ID based on platform-specific hardware identifiers.
//! This ID is stored alongside trial data to prevent users from resetting trials
//! by deleting keychain entries.

/// Generate a stable machine ID for this machine.
///
/// The ID is derived from platform-specific hardware identifiers that are
/// stable across OS reinstalls on the same hardware. This prevents trial reset
/// by deleting keychain entries, as the machine ID will persist.
pub fn machine_id() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    return macos_machine_id();

    #[cfg(target_os = "windows")]
    return windows_machine_id();

    #[cfg(target_os = "linux")]
    return linux_machine_id();

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err("machine_id not implemented for this platform".to_string())
}

#[cfg(target_os = "macos")]
fn macos_machine_id() -> Result<String, String> {
    use std::process::Command;

    let output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| format!("failed to run ioreg: {e}"))?;

    if !output.status.success() {
        return Err("ioreg command failed".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformSerialNumber") {
            if let Some(start) = line.find('"') {
                if let Some(end) = line.rfind('"') {
                    let serial = &line[start + 1..end];
                    if !serial.is_empty() {
                        return Ok(format!("macos:{serial}"));
                    }
                }
            }
        }
    }

    // Fallback to hardware UUID
    let output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| format!("failed to run ioreg for UUID: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            if let Some(start) = line.find('"') {
                if let Some(end) = line.rfind('"') {
                    let uuid = &line[start + 1..end];
                    if !uuid.is_empty() {
                        return Ok(format!("macos:{uuid}"));
                    }
                }
            }
        }
    }

    Err("could not find IOPlatformSerialNumber or IOPlatformUUID".to_string())
}

#[cfg(target_os = "windows")]
fn windows_machine_id() -> Result<String, String> {
    use std::process::Command;

    // Read MachineGuid from registry
    let output = Command::new("reg")
        .args([
            "query",
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .map_err(|e| format!("failed to query registry: {e}"))?;

    if !output.status.success() {
        return Err("reg query command failed".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("MachineGuid") {
            if let Some(guid_start) = line.find("REG_SZ") {
                let guid = line[guid_start + 7..].trim();
                if !guid.is_empty() {
                    return Ok(format!("windows:{guid}"));
                }
            }
        }
    }

    Err("could not find MachineGuid in registry".to_string())
}

#[cfg(target_os = "linux")]
fn linux_machine_id() -> Result<String, String> {
    use std::fs;

    // Try /etc/machine-id first
    if let Ok(id) = fs::read_to_string("/etc/machine-id") {
        let id = id.trim();
        if !id.is_empty() && id.len() >= 16 {
            return Ok(format!("linux:{id}"));
        }
    }

    // Fallback to /var/lib/dbus/machine-id
    if let Ok(id) = fs::read_to_string("/var/lib/dbus/machine-id") {
        let id = id.trim();
        if !id.is_empty() && id.len() >= 16 {
            return Ok(format!("linux:{id}"));
        }
    }

    Err("could not read machine-id from /etc/machine-id or /var/lib/dbus/machine-id".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_machine_id_format() {
        let id = machine_id().expect("machine_id should succeed on macOS");
        assert!(id.starts_with("macos:"));
        assert!(id.len() > 10);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_machine_id_format() {
        let id = machine_id().expect("machine_id should succeed on Windows");
        assert!(id.starts_with("windows:"));
        assert!(id.len() > 10);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_machine_id_format() {
        let id = machine_id().expect("machine_id should succeed on Linux");
        assert!(id.starts_with("linux:"));
        assert!(id.len() > 10);
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn machine_id_is_stable() {
        let id1 = machine_id().expect("machine_id should succeed");
        let id2 = machine_id().expect("machine_id should succeed");
        assert_eq!(id1, id2, "machine_id should be stable across calls");
    }
}