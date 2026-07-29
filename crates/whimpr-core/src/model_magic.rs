//! Pre-flight checks for Whisper ggml model files (avoid whisper.cpp segfaults).

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Known ggml / gguf magic prefixes used by whisper.cpp model files.
const MAGIC: &[&[u8; 4]] = &[b"ggml", b"ggmf", b"ggjt", b"gguf"];

/// Minimum plausible model size (~1 MiB). Truncated downloads fail this check.
const MIN_BYTES: u64 = 1_000_000;

pub fn validate_whisper_model(path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("model stat failed: {e}"))?;
    if meta.len() < MIN_BYTES {
        return Err(format!(
            "model file too small ({} bytes); re-download from the Hub",
            meta.len()
        ));
    }
    let mut f = File::open(path).map_err(|e| format!("model open failed: {e}"))?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)
        .map_err(|e| format!("model read failed: {e}"))?;
    if !MAGIC.contains(&&magic) {
        return Err(format!(
            "model magic {:?} is not a known ggml/gguf header; file may be corrupt",
            String::from_utf8_lossy(&magic)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_tiny_or_bad_magic() {
        let dir = std::env::temp_dir().join(format!("whimpr-magic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.bin");
        std::fs::write(&path, b"notg").unwrap();
        assert!(validate_whisper_model(&path).is_err());
        let mut big = vec![0u8; MIN_BYTES as usize];
        big[..4].copy_from_slice(b"ggml");
        std::fs::write(&path, &big).unwrap();
        assert!(validate_whisper_model(&path).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
