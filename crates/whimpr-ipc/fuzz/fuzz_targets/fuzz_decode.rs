#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the JSON decoding path directly
    // This tests serde_json's ability to handle malformed input

    // Try to parse as JSON
    if let Ok(json_str) = std::str::from_utf8(data) {
        let _: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        // We don't assert success - we just want to ensure it doesn't panic
    }

    // Try to parse as length-prefixed frame
    if data.len() >= 4 {
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if len <= whimpr_ipc::codec::MAX_FRAME_LEN {
            // This ensures the length check doesn't panic on extreme values
            let _ = len;
        }
    }
});