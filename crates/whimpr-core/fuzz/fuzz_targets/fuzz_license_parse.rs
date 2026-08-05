#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the license key parsing path
    // License keys have the format: WF1.<base64url(json)>.<base64url(signature)>

    if let Ok(key_str) = std::str::from_utf8(data) {
        let _ = whimpr_core::verify_license_key(key_str);
        // We don't assert success - we just want to ensure it doesn't panic
        // on malformed input
    }
});