//! Load JSON stores with corrupt-file recovery (never panic on bad JSON).

use std::path::Path;

use serde::de::DeserializeOwned;

/// Read `path` as JSON into `T`. On parse failure of an existing file, rename it
/// to `<name>.corrupt-<unix-ts>` and return `T::default()`. Missing files also
/// yield defaults.
pub fn load_or_recover<T: Default + DeserializeOwned>(path: &Path) -> T {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return T::default();
    };
    match serde_json::from_str::<T>(&raw) {
        Ok(v) => v,
        Err(err) => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!(
                    "{}.corrupt-{ts}",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("store.json")
                ));
            let _ = std::fs::rename(path, &backup);
            tracing::warn!(
                target: "whimpr",
                path = %path.display(),
                backup = %backup.display(),
                error = %err,
                "corrupt JSON store recovered with defaults"
            );
            T::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct Tiny {
        n: u32,
    }

    #[test]
    fn recovers_corrupt_file() {
        let dir = std::env::temp_dir().join(format!("whimpr-json-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{not json").unwrap();
        let v: Tiny = load_or_recover(&path);
        assert_eq!(v, Tiny::default());
        assert!(!path.exists());
        let corrupt = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
        assert!(corrupt);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
