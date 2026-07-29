//! Local data backup: copy the user's JSON stores into a timestamped folder.
//!
//! Keeps at most [`MAX_BACKUPS`] folders under `backup_root`, pruning the oldest
//! after each successful backup. Restore copies files from a chosen folder back
//! over the live store paths.

use std::path::{Path, PathBuf};

/// Maximum number of timestamped backup folders retained under `backups/`.
pub const MAX_BACKUPS: usize = 20;

/// Copy each existing file in `files` (display name, source path) into
/// `backup_root/<unix-timestamp>/`. A source that doesn't exist yet is skipped.
/// Returns the created backup folder. Prunes older folders beyond [`MAX_BACKUPS`].
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
    prune_old_backups(backup_root, MAX_BACKUPS)?;
    Ok(dest_dir)
}

/// List backup folder paths newest-first.
pub fn list_backups(backup_root: &Path) -> std::io::Result<Vec<PathBuf>> {
    if !backup_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(backup_root)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    Ok(dirs)
}

/// Copy every file from `backup_dir` into the matching destination path from
/// `files` (by file name). Unknown files in the backup folder are ignored.
pub fn restore_files(files: &[(&str, PathBuf)], backup_dir: &Path) -> std::io::Result<usize> {
    if !backup_dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "backup folder not found",
        ));
    }
    let mut restored = 0usize;
    for (name, dest) in files {
        let src = backup_dir.join(name);
        if !src.is_file() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, dest)?;
        restored += 1;
    }
    Ok(restored)
}

fn prune_old_backups(backup_root: &Path, keep: usize) -> std::io::Result<()> {
    let mut dirs = list_backups(backup_root)?;
    if dirs.len() <= keep {
        return Ok(());
    }
    // list_backups is newest-first; drop the oldest (tail).
    for old in dirs.drain(keep..) {
        let _ = std::fs::remove_dir_all(old);
    }
    Ok(())
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
        let missing = tmp.join("snippets.json");

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
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prunes_beyond_max_backups() {
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("backups");
        std::fs::create_dir_all(&root).unwrap();
        // Stamp folders by hand with distinct second names (backup_files uses
        // whole-second timestamps, so rapid calls would collide).
        for i in 0..(MAX_BACKUPS + 3) {
            let d = root.join(format!("{}", 1_700_000_000u64 + i as u64));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("settings.json"), "{}").unwrap();
        }
        prune_old_backups(&root, MAX_BACKUPS).unwrap();
        assert_eq!(list_backups(&root).unwrap().len(), MAX_BACKUPS);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn restore_overwrites_live_file() {
        let tmp = std::env::temp_dir().join(format!("whimpr-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let settings = tmp.join("settings.json");
        std::fs::write(&settings, "{\"v\":1}").unwrap();
        let backup =
            backup_files(&[("settings.json", settings.clone())], &tmp.join("backups")).unwrap();
        std::fs::write(&settings, "{\"v\":2}").unwrap();
        let n = restore_files(&[("settings.json", settings.clone())], &backup).unwrap();
        assert_eq!(n, 1);
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{\"v\":1}");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
