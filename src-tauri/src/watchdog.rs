//! Launch watchdog: detect crash loops after updates and offer safe mode.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const SENTINEL: &str = "launching.sentinel";
const STATE: &str = "launch_watchdog.json";
const MAX_FAILS: u32 = 3;

#[derive(Debug, Default, Serialize, Deserialize)]
struct WatchState {
    failed_launches: u32,
    safe_mode: bool,
}

fn support_dir() -> PathBuf {
    crate::logging::support_dir()
}

pub fn note_launch() -> Result<(), String> {
    let dir = support_dir();
    let _ = std::fs::create_dir_all(&dir);
    let sentinel = dir.join(SENTINEL);
    let state_path = dir.join(STATE);
    let mut state: WatchState = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    if sentinel.exists() {
        state.failed_launches = state.failed_launches.saturating_add(1);
        tracing::warn!(
            target: "whimpr",
            fails = state.failed_launches,
            "previous launch did not clear sentinel"
        );
        if state.failed_launches >= MAX_FAILS {
            state.safe_mode = true;
            tracing::error!(
                target: "whimpr",
                "entering safe mode after {MAX_FAILS} consecutive failed launches; reinstall previous version from GitHub Releases"
            );
        }
        let _ = std::fs::write(
            &state_path,
            serde_json::to_string_pretty(&state).unwrap_or_default(),
        );
    }

    std::fs::write(&sentinel, b"1").map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clear_launch_sentinel() -> Result<(), String> {
    let dir = support_dir();
    let _ = std::fs::remove_file(dir.join(SENTINEL));
    let state_path = dir.join(STATE);
    let mut state: WatchState = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    state.failed_launches = 0;
    state.safe_mode = false;
    let _ = std::fs::write(
        &state_path,
        serde_json::to_string_pretty(&state).unwrap_or_default(),
    );
    Ok(())
}

pub fn in_safe_mode() -> bool {
    let path = support_dir().join(STATE);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<WatchState>(&s).ok())
        .map(|s| s.safe_mode)
        .unwrap_or(false)
}
