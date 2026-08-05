#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the license payload JSON parsing path
    // This tests the inner JSON deserialization

    if let Ok(json_str) = std::str::from_utf8(data) {
        let _: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        // We don't assert success - we just want to ensure it doesn't panic

        // Try to parse as LicensePayload specifically
        let _: Result<whimpr_core::LicensePayload, _> = serde_json::from_str(json_str);
    }
});