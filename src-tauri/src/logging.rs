//! Structured logging to stderr + rotating daily files under the support dir.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static COLD_START_MS: OnceLock<u64> = OnceLock::new();
static PROCESS_START: OnceLock<std::time::Instant> = OnceLock::new();

pub fn support_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(base).join("WhimprFlow")
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("WhimprFlow");
            }
        }
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config").join("WhimprFlow")
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/WhimprFlow")
    }
}

pub fn mark_process_start() {
    let _ = PROCESS_START.set(std::time::Instant::now());
}

pub fn mark_hub_ready() {
    if let Some(start) = PROCESS_START.get() {
        let ms = start.elapsed().as_millis() as u64;
        let _ = COLD_START_MS.set(ms);
        tracing::info!(target: "whimpr", cold_start_ms = ms, "Hub window shown");
    }
}

pub fn last_cold_start_ms() -> Option<u64> {
    COLD_START_MS.get().copied()
}

/// Initialize tracing to stderr + daily rotating file. Keeps 7 days of logs.
pub fn init() {
    let dir = support_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let _ = LOG_DIR.set(dir.clone());
    prune_old_logs(&dir, 7);

    let file_appender = tracing_appender::rolling::daily(&dir, "whimpr");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leak the guard so the worker thread lives for the process lifetime.
    std::mem::forget(guard);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr).with_target(true))
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true),
        )
        .try_init();

    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let msg = if let Some(s) = info.payload().downcast_ref::<&'static str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<Any>".into()
        };
        tracing::error!(target: "whimpr", location = %loc, panic = %msg, "panic");
        maybe_write_crash_report(&loc, &msg);
    }));

    tracing::info!(target: "whimpr", log_dir = %dir.display(), "logging initialized");
}

fn prune_old_logs(dir: &Path, keep_days: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let cutoff = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(keep_days * 24 * 60 * 60))
        .unwrap_or(0);
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("whimpr") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let secs = modified
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if secs < cutoff {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Zip last 7 days of logs + redacted settings into diagnostics-<ts>.zip.
pub fn export_diagnostics() -> Result<String, String> {
    let support = support_dir();
    let logs = support.join("logs");
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let out = support.join(format!("diagnostics-{ts}.zip"));

    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    if logs.is_dir() {
        for entry in std::fs::read_dir(&logs)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("log.txt");
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            zip.start_file(format!("logs/{name}"), opts)
                .map_err(|e| e.to_string())?;
            use std::io::Write;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }

    let settings_path = support.join("settings.json");
    let redacted = redact_settings_snapshot(&settings_path);
    zip.start_file("settings.redacted.json", opts)
        .map_err(|e| e.to_string())?;
    {
        use std::io::Write;
        zip.write_all(redacted.as_bytes())
            .map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(out.display().to_string())
}

fn redact_settings_snapshot(path: &Path) -> String {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return "{\"error\":\"settings missing\"}".into();
    };
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return "{\"error\":\"settings unreadable\"}".into();
    };
    if let Some(obj) = v.as_object_mut() {
        obj.remove("email");
        // Never include secrets; only note whether cloud modes are configured via status.
        obj.insert(
            "api_keys_redacted".into(),
            serde_json::json!("see Hub status.has_*_key"),
        );
    }
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())
}

fn crash_reporting_opted_in() -> bool {
    let path = support_dir().join("settings.json");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("crash_reporting_opt_in")?.as_bool())
        .unwrap_or(false)
}

fn maybe_write_crash_report(loc: &str, msg: &str) {
    if !crash_reporting_opted_in() {
        return;
    }
    let dir = support_dir();
    let _ = std::fs::create_dir_all(&dir);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("crash-{ts}.txt"));
    let mut body = format!("panic at {loc}\n{msg}\n\n--- recent logs ---\n");
    let logs = dir.join("logs");
    if let Ok(rd) = std::fs::read_dir(&logs) {
        let mut files: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        if let Some(last) = files.last() {
            if let Ok(text) = std::fs::read_to_string(last) {
                let lines: Vec<&str> = text.lines().rev().take(100).collect();
                for line in lines.into_iter().rev() {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    let _ = std::fs::write(path, body);
}

/// Paths of local crash report files (opt-in panic dumps).
pub fn list_crash_reports() -> Vec<String> {
    let dir = support_dir();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            if name.starts_with("crash-") && name.ends_with(".txt") {
                Some(path.display().to_string())
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out.reverse();
    out
}
