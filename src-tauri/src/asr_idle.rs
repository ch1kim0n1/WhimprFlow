//! Idle unload of the Whisper engine based on `unload_asr_after_idle_minutes`.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Once;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static LAST_DICTATION_UNIX: AtomicI64 = AtomicI64::new(0);
static WATCHER_STARTED: Once = Once::new();
static UNLOAD_LOGGED: AtomicBool = AtomicBool::new(false);

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Call at the start of every push-to-talk / dictation session.
pub fn touch_dictation() {
    LAST_DICTATION_UNIX.store(now_unix(), Ordering::SeqCst);
    UNLOAD_LOGGED.store(false, Ordering::SeqCst);
}

/// Spawn a once-per-process watcher. `unload` should drop the platform ASR slot.
pub fn spawn_watcher(unload: fn()) {
    WATCHER_STARTED.call_once(|| {
        // Seed so we don't unload immediately after cold start with no dictation yet
        // until the configured idle window has elapsed from first spawn.
        touch_dictation();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(60));
            let mins = crate::hotkey::current_settings().unload_asr_after_idle_minutes;
            if mins == 0 {
                continue;
            }
            let last = LAST_DICTATION_UNIX.load(Ordering::SeqCst);
            let idle_secs = now_unix().saturating_sub(last);
            if idle_secs >= i64::from(mins) * 60 {
                unload();
                if !UNLOAD_LOGGED.swap(true, Ordering::SeqCst) {
                    tracing::info!(
                        target: "whimpr",
                        idle_secs,
                        mins,
                        "ASR unloaded after idle (unload_asr_after_idle_minutes)"
                    );
                }
            }
        });
    });
}
