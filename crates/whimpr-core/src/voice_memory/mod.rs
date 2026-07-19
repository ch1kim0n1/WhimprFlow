//! Voice Memory: a local, auditable log of what WhimprFlow has learned about
//! the user's speech  -  every correction picked up by autolearn plus manual
//! dictionary edits. Encrypted at rest with AES-256-GCM (the key lives in the
//! OS keychain, owned by the shell); exportable as a plain-JSON bundle together
//! with the dictionary, snippets, and style profile.
//!
//! ponytail: v1 ceiling is corrections + vocab only  -  no acoustic adaptation.
//! Upgrade path: per-user fine-tuning / bias lists fed into the ASR engine.

use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use serde::{Deserialize, Serialize};

use crate::dictionary::DictionaryStore;
use crate::settings::StyleProfile;
use crate::snippets::SnippetStore;

/// AES-GCM standard nonce length; the nonce is prefixed to the ciphertext.
const NONCE_LEN: usize = 12;

/// One learned correction: `from` was heard, `to` is what the user meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionEvent {
    /// Seconds since the Unix epoch (UTC) when the correction was learned.
    pub ts_unix: u64,
    /// What the recognizer produced.
    pub from: String,
    /// What the user corrected it to.
    pub to: String,
    /// Where the correction came from, e.g. "autolearn" | "manual".
    pub source: String,
}

/// The full memory log, persisted encrypted (see [`VoiceMemory::save_encrypted`]).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceMemory {
    #[serde(default)]
    pub corrections: Vec<CorrectionEvent>,
}

impl VoiceMemory {
    /// Load and decrypt from `path` with a 32-byte key. Any failure  -  missing
    /// file, truncated data, wrong key, tampered ciphertext, bad JSON  -  returns
    /// an empty default rather than an error: memory is an enhancement, never a
    /// reason the app fails to start.
    pub fn load_encrypted(path: &Path, key: &[u8; 32]) -> Self {
        let Ok(blob) = std::fs::read(path) else {
            return Self::default();
        };
        if blob.len() <= NONCE_LEN {
            return Self::default();
        }
        let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .ok()
            .and_then(|plain| serde_json::from_slice(&plain).ok())
            .unwrap_or_default()
    }

    /// Encrypt (AES-256-GCM, fresh random 12-byte nonce prefixed to the
    /// ciphertext) and write to `path`, creating parent dirs.
    pub fn save_encrypted(&self, path: &Path, key: &[u8; 32]) -> anyhow::Result<()> {
        let plain = serde_json::to_vec(self)?;
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce)
            .map_err(|e| anyhow::anyhow!("nonce randomness unavailable: {e}"))?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_slice())
            .map_err(|e| anyhow::anyhow!("voice memory encryption failed: {e}"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        std::fs::write(path, blob)?;
        Ok(())
    }

    /// Append one learned correction.
    pub fn record(&mut self, from: String, to: String, source: String, ts_unix: u64) {
        self.corrections.push(CorrectionEvent {
            ts_unix,
            from,
            to,
            source,
        });
    }

    /// The exportable plain-JSON bundle: everything WhimprFlow has learned  -
    /// corrections, dictionary, snippets, style  -  in one portable document the
    /// user can inspect, back up, or move to another machine.
    pub fn export_bundle(
        &self,
        dict: &DictionaryStore,
        snippets: &SnippetStore,
        style: &StyleProfile,
    ) -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "corrections": self.corrections,
            "dictionary": dict,
            "snippets": snippets,
            "style": style,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8; 32] = b"0123456789abcdef0123456789abcdef";

    fn memory() -> VoiceMemory {
        let mut m = VoiceMemory::default();
        m.record("monvi".into(), "Manvi".into(), "autolearn".into(), 1_000);
        m.record("wimper".into(), "Whimpr".into(), "manual".into(), 2_000);
        m
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("whimpr-vm-test-{name}-{}", std::process::id()))
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let path = temp_path("roundtrip");
        let m = memory();
        m.save_encrypted(&path, KEY).expect("save should succeed");

        // The bytes on disk are not the plaintext JSON.
        let blob = std::fs::read(&path).unwrap();
        assert!(blob.len() > NONCE_LEN);
        let on_disk = String::from_utf8_lossy(&blob);
        assert!(!on_disk.contains("Manvi"), "plaintext must not hit disk");

        let back = VoiceMemory::load_encrypted(&path, KEY);
        assert_eq!(back.corrections.len(), 2);
        assert_eq!(back.corrections[0].from, "monvi");
        assert_eq!(back.corrections[0].to, "Manvi");
        assert_eq!(back.corrections[1].source, "manual");
        assert_eq!(back.corrections[1].ts_unix, 2_000);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wrong_key_or_garbage_falls_back_to_default() {
        let path = temp_path("wrongkey");
        memory().save_encrypted(&path, KEY).expect("save");

        let wrong: [u8; 32] = *b"ffffffffffffffffffffffffffffffff";
        let m = VoiceMemory::load_encrypted(&path, &wrong);
        assert!(m.corrections.is_empty(), "wrong key must yield default");

        // Truncated / garbage file also falls back to default.
        std::fs::write(&path, b"not encrypted").unwrap();
        let m2 = VoiceMemory::load_encrypted(&path, KEY);
        assert!(m2.corrections.is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_loads_default() {
        let m = VoiceMemory::load_encrypted(Path::new("/nonexistent/whimpr-vm.bin"), KEY);
        assert!(m.corrections.is_empty());
    }

    #[test]
    fn export_bundle_is_plain_json_with_all_sections() {
        let mut dict = DictionaryStore::default();
        dict.add(
            "Manvi",
            vec!["monvi".to_string()],
            crate::dictionary::DictSource::Manual,
        );
        let mut snippets = SnippetStore::default();
        snippets.add("my email".into(), "user@example.com".into());
        let style = StyleProfile::default();

        let bundle = memory().export_bundle(&dict, &snippets, &style);
        assert_eq!(bundle["version"], 1);
        assert_eq!(bundle["corrections"].as_array().unwrap().len(), 2);
        assert_eq!(bundle["corrections"][0]["to"], "Manvi");
        assert_eq!(bundle["dictionary"]["entries"][0]["correct"], "Manvi");
        assert_eq!(bundle["snippets"]["entries"][0]["trigger"], "my email");
        assert!(bundle["style"].is_object());
    }
}
