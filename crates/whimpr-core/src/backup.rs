//! Local data backup: copy the user's JSON stores into a timestamped folder.
//!
//! ponytail: no compression, no retention/rotation policy, no cloud upload  -
//! just "don't lose your dictionary if the JSON gets corrupted or you
//! fat-finger a delete." Add pruning of old backups later if `backups/`
//! actually grows large enough to matter; it hasn't yet for four small
//! JSON files.

use std::path::{Path, PathBuf};

/// Copy each existing file in `files` (display name, source path) into
/// `backup_root/<unix-timestamp>/`. A source that doesn't exist yet (e.g.
/// `snippets.json` before the user has added one) is skipped, not an error  -
/// only a real I/O failure (e.g. can't create the destination directory) is.
/// Returns the created backup folder.
pub fn backup_files(files: &[(&str, PathBuf)], backup_root: &Path) -> std::io::Result<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest_dir = backup_root.join(stamp.to_string());
    std::fs::create_dir_all(&dest_dir)?;
    for (name, src) in files {
        if src.exists() {
            std::fs::copy(src, dest_dir.join(name))?;
        }
    }
    Ok(dest_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_existing_files_and_skips_missing_ones() {
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let settings = tmp.join("settings.json");
        std::fs::write(&settings, "{}").unwrap();
        let missing = tmp.join("snippets.json"); // deliberately never created

        let dest = backup_files(
            &[
                ("settings.json", settings.clone()),
                ("snippets.json", missing),
            ],
            &tmp.join("backups"),
        )
        .unwrap();

        assert!(dest.join("settings.json").exists());
        assert!(!dest.join("snippets.json").exists());
        std::fs::read_to_string(dest.join("settings.json")).unwrap();

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
