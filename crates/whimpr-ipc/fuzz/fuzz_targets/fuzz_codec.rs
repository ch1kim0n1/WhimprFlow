#![no_main]
use libfuzzer_sys::fuzz_target;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TestMessage {
    field1: String,
    field2: u32,
    field3: Vec<u8>,
    field4: Option<String>,
}

fuzz_target!(|data: &[u8]| {
    // Fuzz the frame encoding/decoding path
    // We try to decode the data as a frame, and if successful, re-encode it
    // and decode again to ensure round-trip consistency

    use std::io::Cursor;

    // Try to decode as a frame
    let mut cursor = Cursor::new(data);
    match whimpr_ipc::codec::read_frame::<_, TestMessage>(&mut cursor) {
        Ok(Some(msg)) => {
            // Successfully decoded, now re-encode
            let mut encoded = Vec::new();
            if whimpr_ipc::codec::write_frame(&mut encoded, &msg).is_ok() {
                // Try to decode the re-encoded data
                let mut cursor2 = Cursor::new(encoded);
                if let Ok(Some(msg2)) = whimpr_ipc::codec::read_frame::<_, TestMessage>(&mut cursor2) {
                    // Ensure round-trip consistency
                    assert_eq!(msg.field1, msg2.field1);
                    assert_eq!(msg.field2, msg2.field2);
                    assert_eq!(msg.field3, msg2.field3);
                    assert_eq!(msg.field4, msg2.field4);
                }
            }
        }
        Ok(None) => {
            // Clean EOF - this is fine
        }
        Err(_) => {
            // Decode error - this is expected for random fuzz data
            // The codec should handle malformed data gracefully
        }
    }
});