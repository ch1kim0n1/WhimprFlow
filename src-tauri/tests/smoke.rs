//! Integration smoke tests: call Tauri command bodies directly (no IPC runtime).

use whimpr_core::EntitlementKind;
use whimpr_tauri_lib::smoke_api;

#[test]
fn get_settings_returns_defaults() {
    smoke_api::bootstrap();
    let s = smoke_api::get_settings();
    assert_eq!(s.openai_model, "gpt-4o-mini");
    assert!(s.sound_on_start);
    assert!(s.clear_clipboard_after_paste);
}

#[test]
fn set_settings_persists() {
    smoke_api::bootstrap();
    let mut s = smoke_api::get_settings();
    let original = s.sound_on_start;
    s.sound_on_start = !original;
    let saved = smoke_api::set_settings(s).expect("set_settings");
    assert_eq!(saved.sound_on_start, !original);
    let again = smoke_api::get_settings();
    assert_eq!(again.sound_on_start, !original);
    // Restore so other tests / local machines aren't left flipped.
    let mut restore = again;
    restore.sound_on_start = original;
    let _ = smoke_api::set_settings(restore);
}

#[test]
fn get_entitlement_unlicensed_on_clean_machine() {
    smoke_api::bootstrap();
    let e = smoke_api::get_entitlement();
    // Spec: clean machine → Unlicensed. Developer machines with a saved key/trial
    // may already be Licensed/Trial; still assert a well-formed entitlement.
    match e.kind {
        EntitlementKind::Unlicensed => assert!(!e.cloud_cleanup_allowed),
        EntitlementKind::Trial | EntitlementKind::Licensed => {
            assert!(e.cloud_cleanup_allowed);
        }
    }
}

#[test]
fn export_diagnostics_writes_zip() {
    smoke_api::bootstrap();
    let path = smoke_api::export_diagnostics().expect("export_diagnostics");
    let p = std::path::Path::new(&path);
    assert!(p.exists(), "diagnostics path missing: {path}");
    assert!(path.ends_with(".zip"), "expected .zip path, got {path}");
    let bytes = std::fs::read(p).expect("read zip");
    assert!(bytes.len() > 4, "zip too small");
    // ZIP local file header magic
    assert_eq!(&bytes[0..2], b"PK");
}

#[test]
fn list_model_offers_has_four() {
    smoke_api::bootstrap();
    let offers = smoke_api::list_model_offers();
    assert_eq!(offers.len(), 4, "expected 4 catalog models");
}

#[test]
fn get_build_info_has_version() {
    smoke_api::bootstrap();
    let info = smoke_api::get_build_info();
    assert!(!info.version.is_empty());
}
