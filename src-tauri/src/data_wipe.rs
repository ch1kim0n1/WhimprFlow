//! GDPR-style wipe of local WhimprFlow data (keeps models unless opted in).

use std::path::Path;

fn support_dir() -> std::path::PathBuf {
    crate::logging::support_dir()
}

fn delete_keychain(account: &str) {
    if let Ok(entry) = keyring::Entry::new("com.whimpr.whimprflow", account) {
        let _ = entry.delete_credential();
    }
}

fn remove_path(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

pub fn wipe_all(delete_models: bool) -> Result<(), String> {
    let root = support_dir();
    for name in [
        "settings.json",
        "dictionary.json",
        "snippets.json",
        "stats.json",
        "workflows.json",
        "notes.json",
        "voice_memory.enc",
        "launching.sentinel",
        "launch_watchdog.json",
    ] {
        remove_path(&root.join(name));
    }
    // Also wipe any .corrupt-* siblings.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains(".corrupt-") || name.starts_with("diagnostics-") {
                remove_path(&entry.path());
            }
        }
    }
    remove_path(&root.join("backups"));
    remove_path(&root.join("logs"));
    if delete_models {
        remove_path(&root.join("models"));
    }

    for account in [
        "license_key",
        "trial_started_unix",
        "openai_api_key",
        "anthropic_api_key",
        "voice_memory_key",
    ] {
        delete_keychain(account);
    }

    tracing::info!(target: "whimpr", delete_models, "wipe_all_data completed");
    Ok(())
}
