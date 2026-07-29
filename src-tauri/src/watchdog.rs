//! Launch watchdog: detect crash loops after updates and offer safe mode / rollback.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const SENTINEL: &str = "launching.sentinel";
const STATE: &str = "launch_watchdog.json";
const MAX_FAILS: u32 = 3;

#[derive(Debug, Default, Serialize, Deserialize)]
struct WatchState {
    failed_launches: u32,
    safe_mode: bool,
    /// Last version that completed a successful Hub-ready launch.
    #[serde(default)]
    current_version: Option<String>,
    /// Version before the most recent successful upgrade (for one-click rollback).
    #[serde(default)]
    previous_version: Option<String>,
}

fn support_dir() -> PathBuf {
    crate::logging::support_dir()
}

fn package_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
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
                "entering safe mode after {MAX_FAILS} consecutive failed launches; offer rollback or reinstall from GitHub Releases"
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

    let ver = package_version();
    if let Some(cur) = state.current_version.clone() {
        if cur != ver {
            state.previous_version = Some(cur);
        }
    }
    state.current_version = Some(ver);

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

/// Version that last launched successfully before the current one (if any).
pub fn previous_version() -> Option<String> {
    let path = support_dir().join(STATE);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<WatchState>(&s).ok())
        .and_then(|s| s.previous_version)
}
