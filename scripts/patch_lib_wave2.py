#!/usr/bin/env python3
"""Patch src-tauri/src/lib.rs for wave-2 commands + logging init."""
from pathlib import Path

path = Path("src-tauri/src/lib.rs")
text = path.read_text(encoding="utf-8")

if "mod logging;" not in text:
    text = text.replace("mod licensing;\n", "mod licensing;\nmod logging;\n")

if "fn export_diagnostics" not in text:
    insert = '''
#[tauri::command]
fn export_diagnostics() -> Result<String, String> {
    logging::export_diagnostics()
}

#[tauri::command]
fn get_last_cold_start_ms() -> Option<u64> {
    logging::last_cold_start_ms()
}

#[tauri::command]
fn hub_ready() {
    logging::mark_hub_ready();
    let _ = crate::watchdog::clear_launch_sentinel();
}

#[derive(serde::Serialize)]
struct BuildInfo {
    version: &'static str,
    git_hash: &'static str,
}

#[tauri::command]
fn get_build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        git_hash: env!("WHIMPR_GIT_HASH"),
    }
}

#[tauri::command]
fn wipe_all_data(delete_models: bool) -> Result<(), String> {
    crate::data_wipe::wipe_all(delete_models)
}

#[tauri::command]
fn mic_self_test() -> Result<f32, String> {
    crate::mic_test::peak_rms_2s()
}

'''
    text = text.replace("pub fn run() {", insert + "pub fn run() {")

# Ensure Result on set_settings
old_set = """#[tauri::command]
fn set_settings(mut settings: whimpr_core::Settings) {
    // Cloud cleanup requires an active license or trial.
    if matches!(
        settings.cleanup_mode,
        whimpr_core::CleanupMode::OpenAi | whimpr_core::CleanupMode::Anthropic
    ) && !licensing::cloud_cleanup_allowed()
    {
        settings.cleanup_mode = whimpr_core::CleanupMode::Local;
    }
    hotkey::update_settings(settings);
}"""
new_set = """#[tauri::command]
fn set_settings(mut settings: whimpr_core::Settings) -> Result<whimpr_core::Settings, String> {
    // Cloud cleanup requires an active license or trial.
    if matches!(
        settings.cleanup_mode,
        whimpr_core::CleanupMode::OpenAi | whimpr_core::CleanupMode::Anthropic
    ) && !licensing::cloud_cleanup_allowed()
    {
        settings.cleanup_mode = whimpr_core::CleanupMode::Local;
    }
    hotkey::update_settings(settings.clone());
    Ok(settings)
}"""
if "fn set_settings(mut settings: whimpr_core::Settings) {" in text:
    text = text.replace(old_set, new_set)

if "logging::init()" not in text:
    text = text.replace(
        "pub fn run() {\n    tauri::Builder::default()",
        "pub fn run() {\n    logging::mark_process_start();\n    logging::init();\n    let _ = crate::watchdog::note_launch();\n    tauri::Builder::default()",
    )

# Add new commands to handler list
needle = "            start_trial\n        ])"
if needle in text and "export_diagnostics" not in text.split("generate_handler")[1][:800]:
    text = text.replace(
        needle,
        """            start_trial,
            export_diagnostics,
            get_last_cold_start_ms,
            hub_ready,
            get_build_info,
            wipe_all_data,
            mic_self_test
        ])""",
    )

# Ensure plugins still present (wave1)
if "tauri_plugin_updater" not in text:
    text = text.replace(
        "tauri::Builder::default()\n        .invoke_handler",
        "tauri::Builder::default()\n        .plugin(tauri_plugin_updater::Builder::new().build())\n        .plugin(tauri_plugin_process::init())\n        .invoke_handler",
    )

path.write_text(text, encoding="utf-8", newline="\n")
print("patched lib.rs")
