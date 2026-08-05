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

    // ── Edge case coverage: empty backups, missing dirs, unknown files ─────

    #[test]
    fn backup_with_no_source_files_creates_empty_backup() {
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let missing1 = tmp.join("a.json");
        let missing2 = tmp.join("b.json");

        let dest = backup_files(
            &[("a.json", missing1), ("b.json", missing2)],
            &tmp.join("backups"),
        )
        .unwrap();

        assert!(dest.exists(), "backup folder should be created even with no files");
        assert!(!dest.join("a.json").exists());
        assert!(!dest.join("b.json").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn restore_from_nonexistent_dir_errors() {
        let tmp = std::env::temp_dir().join(format!("whimpr-restore-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let res = restore_files(
            &[("settings.json", tmp.join("settings.json"))],
            &tmp.join("nonexistent-backup"),
        );
        assert!(res.is_err(), "restoring from a missing folder must error");
        assert_eq!(
            res.unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn restore_skips_unknown_files_in_backup() {
        // A backup folder with extra files not in the restore list → those
        // are ignored, only listed files are restored.
        let tmp = std::env::temp_dir().join(format!("whimpr-restore-unknown-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let backup_dir = tmp.join("manual-backup");
        std::fs::create_dir_all(&backup_dir).unwrap();
        std::fs::write(backup_dir.join("settings.json"), "{\"v\":1}").unwrap();
        std::fs::write(backup_dir.join("unknown.json"), "{}").unwrap();

        let dest = tmp.join("live").join("settings.json");
        let n = restore_files(&[("settings.json", dest.clone())], &backup_dir).unwrap();
        assert_eq!(n, 1);
        assert!(dest.exists());
        assert!(!tmp.join("live").join("unknown.json").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn restore_skips_missing_files_in_backup() {
        // Backup folder exists but the specific file is missing → skip, return 0.
        let tmp = std::env::temp_dir().join(format!("whimpr-restore-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let backup_dir = tmp.join("backup");
        std::fs::create_dir_all(&backup_dir).unwrap();
        // No settings.json in the backup folder.

        let n = restore_files(
            &[("settings.json", tmp.join("settings.json"))],
            &backup_dir,
        )
        .unwrap();
        assert_eq!(n, 0, "missing file in backup should be skipped, not error");
        assert!(!tmp.join("settings.json").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn list_backups_empty_for_nonexistent_root() {
        let res = list_backups(std::path::Path::new("/nonexistent/whimpr-backups"));
        assert!(res.unwrap().is_empty());
    }

    #[test]
    fn list_backups_returns_newest_first() {
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("backups");
        std::fs::create_dir_all(&root).unwrap();
        for i in 0..5 {
            let d = root.join(format!("{}", 1_700_000_000u64 + i as u64));
            std::fs::create_dir_all(&d).unwrap();
        }
        let dirs = list_backups(&root).unwrap();
        assert_eq!(dirs.len(), 5);
        // Newest first = highest timestamp first.
        assert_eq!(
            dirs[0].file_name().unwrap().to_string_lossy(),
            "1700000004"
        );
        assert_eq!(
            dirs[4].file_name().unwrap().to_string_lossy(),
            "1700000000"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn list_backups_ignores_files_only_dirs() {
        // A regular file in the backup root should not appear in the list.
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("backups");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("stray.txt"), "hello").unwrap();
        std::fs::create_dir_all(root.join("1700000000")).unwrap();

        let dirs = list_backups(&root).unwrap();
        assert_eq!(dirs.len(), 1, "only directories should be listed");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prune_keeps_exactly_max_backups() {
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-prune-exact-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("backups");
        std::fs::create_dir_all(&root).unwrap();
        for i in 0..MAX_BACKUPS {
            let d = root.join(format!("{}", 1_700_000_000u64 + i as u64));
            std::fs::create_dir_all(&d).unwrap();
        }
        prune_old_backups(&root, MAX_BACKUPS).unwrap();
        assert_eq!(list_backups(&root).unwrap().len(), MAX_BACKUPS);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prune_with_zero_backups_keeps_none() {
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-prune-zero-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("backups");
        std::fs::create_dir_all(&root).unwrap();
        for i in 0..3 {
            let d = root.join(format!("{}", 1_700_000_000u64 + i as u64));
            std::fs::create_dir_all(&d).unwrap();
        }
        prune_old_backups(&root, 0).unwrap();
        assert_eq!(list_backups(&root).unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn backup_then_restore_round_trip_multiple_files() {
        let tmp = std::env::temp_dir().join(format!("whimpr-backup-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let settings = tmp.join("settings.json");
        let dict = tmp.join("dictionary.json");
        std::fs::write(&settings, r#"{"mode":"local"}"#).unwrap();
        std::fs::write(&dict, r#"{"entries":[]}"#).unwrap();

        let files = vec![
            ("settings.json", settings.clone()),
            ("dictionary.json", dict.clone()),
        ];

        let backup = backup_files(&files, &tmp.join("backups")).unwrap();

        // Modify live files.
        std::fs::write(&settings, r#"{"mode":"openai"}"#).unwrap();
        std::fs::write(&dict, r#"{"entries":[{"x":1}]}"#).unwrap();

        // Restore.
        let n = restore_files(&files, &backup).unwrap();
        assert_eq!(n, 2);
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), r#"{"mode":"local"}"#);
        assert_eq!(std::fs::read_to_string(&dict).unwrap(), r#"{"entries":[]}"#);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
