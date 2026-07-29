//! Wave-4 Hub helpers: settings transfer, history export, models, network, API keys.

use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use whimpr_cleanup::{client, validate_base_url};

#[derive(Clone, serde::Serialize)]
pub struct InstalledModel {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub active: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct KeyBindingsDto {
    pub cancel: whimpr_core::Chord,
    pub paste_last: whimpr_core::Chord,
    pub copy_last: whimpr_core::Chord,
    pub undo_last: whimpr_core::Chord,
}

pub fn export_settings() -> Result<String, String> {
    let s = crate::hotkey::current_settings();
    serde_json::to_string_pretty(&s).map_err(|e| e.to_string())
}

pub fn import_settings(json: String) -> Result<whimpr_core::Settings, String> {
    let mut settings: whimpr_core::Settings =
        serde_json::from_str(&json).map_err(|e| format!("invalid settings JSON: {e}"))?;
    settings.settings_version = settings.settings_version.max(1);
    crate::hotkey::update_settings(settings.clone());
    Ok(settings)
}

pub fn export_history(format: String) -> Result<String, String> {
    let items = crate::hotkey::history(10_000);
    let dir = crate::logging::support_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match format.as_str() {
        "json" => {
            let path = dir.join(format!("history-{ts}.json"));
            let body = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
            std::fs::write(&path, body).map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
        "txt" => {
            let path = dir.join(format!("history-{ts}.txt"));
            let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            for it in items {
                let line = it.text.replace('\n', " ").trim().to_string();
                if !line.is_empty() {
                    writeln!(f, "{line}").map_err(|e| e.to_string())?;
                }
            }
            Ok(path.display().to_string())
        }
        other => Err(format!(
            "unsupported export format: {other} (use json or txt)"
        )),
    }
}

pub fn list_installed_models() -> Vec<InstalledModel> {
    let dir = crate::models::models_dir();
    let active = crate::hotkey::current_settings()
        .asr_model
        .clone()
        .unwrap_or_default();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let meta = entry.metadata().ok();
        let size = meta.map(|m| m.len()).unwrap_or(0);
        let path_s = path.display().to_string();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("model")
            .to_string();
        let is_active = !active.is_empty()
            && (path_s == active
                || path.file_name().and_then(|n| n.to_str()) == Some(active.as_str()));
        out.push(InstalledModel {
            name,
            path: path_s,
            size_bytes: size,
            active: is_active,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn set_active_model(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(format!("model file not found: {path}"));
    }
    let mut s = crate::hotkey::current_settings();
    s.asr_model = Some(path);
    crate::hotkey::update_settings(s);
    crate::hotkey::reload_asr();
    Ok(())
}

pub fn delete_model(path: String) -> Result<(), String> {
    let active = crate::hotkey::current_settings().asr_model.clone();
    if active.as_deref() == Some(path.as_str()) {
        return Err("cannot delete the active speech model".into());
    }
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(format!("model file not found: {path}"));
    }
    // Only allow deletes under the models directory.
    let models = crate::models::models_dir();
    let canon = p.canonicalize().map_err(|e| e.to_string())?;
    let models_canon = models.canonicalize().map_err(|e| e.to_string())?;
    if !canon.starts_with(&models_canon) {
        return Err("refusing to delete a file outside the models directory".into());
    }
    std::fs::remove_file(&canon).map_err(|e| e.to_string())
}

pub fn list_input_devices() -> Vec<whimpr_audio::InputDevice> {
    whimpr_audio::list_input_devices()
}

pub fn set_input_device(name: String) -> Result<(), String> {
    let mut s = crate::hotkey::current_settings();
    s.input_device = if name.trim().is_empty() {
        None
    } else {
        Some(name)
    };
    crate::hotkey::update_settings(s);
    Ok(())
}

pub fn check_network() -> Result<bool, String> {
    match TcpStream::connect_timeout(
        &"1.1.1.1:443"
            .parse()
            .map_err(|e| format!("parse addr: {e}"))?,
        Duration::from_secs(2),
    ) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

pub fn validate_api_key(
    provider: String,
    key: String,
    base_url: Option<String>,
) -> Result<bool, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API key is empty".into());
    }
    let client = client();
    match provider.as_str() {
        "openai" => {
            let root = base_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("https://api.openai.com/v1");
            validate_base_url(root).map_err(|e| e.to_string())?;
            let url = format!("{}/models", root.trim_end_matches('/'));
            let resp = client
                .get(&url)
                .bearer_auth(key)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status().is_success() {
                Ok(true)
            } else {
                Err(format!("API key rejected ({})", resp.status()))
            }
        }
        "anthropic" => {
            let url = "https://api.anthropic.com/v1/messages";
            let body = serde_json::json!({
                "model": "claude-haiku-4-5",
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "ping"}]
            });
            let resp = client
                .post(url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status().is_success() || resp.status().as_u16() == 400 {
                // 400 can mean the key auth worked but payload was rejected — treat as valid.
                Ok(true)
            } else if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
                Err("API key rejected".into())
            } else {
                Err(format!("unexpected status {}", resp.status()))
            }
        }
        other => Err(format!("unknown provider {other}")),
    }
}

pub fn get_changelog() -> String {
    const FALLBACK: &str = include_str!("../../CHANGELOG.md");
    // Prefer a bundled resource beside the executable when present.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("CHANGELOG.md");
            if let Ok(s) = std::fs::read_to_string(candidate) {
                return s;
            }
        }
    }
    FALLBACK.to_string()
}

pub fn get_keybindings() -> KeyBindingsDto {
    let kb = crate::hotkey::current_settings().keybindings;
    KeyBindingsDto {
        cancel: kb.cancel,
        paste_last: kb.paste_last,
        copy_last: kb.copy_last,
        undo_last: kb.undo_last,
    }
}

/// Rough current process RSS in bytes (best-effort).
pub fn current_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;
        unsafe {
            let mut counters = PROCESS_MEMORY_COUNTERS::default();
            let ok = GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            );
            if ok.is_ok() {
                Some(counters.WorkingSetSize as u64)
            } else {
                None
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        // Keep a simple fallback; full task_info wiring can land later.
        None
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        None
    }
}
