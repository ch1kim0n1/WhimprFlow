//! Recommended ASR model catalog + download with SHA-256 verification.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

/// One recommended Whisper model a first-run user can install with one click.
#[derive(Clone, serde::Serialize)]
pub struct ModelOffer {
    pub id: &'static str,
    pub file_name: &'static str,
    pub label: &'static str,
    pub size_bytes: u64,
    pub url: &'static str,
    pub sha256: &'static str,
}

const HF: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub const CATALOG: &[ModelOffer] = &[
    ModelOffer {
        id: "tiny.en",
        file_name: "ggml-tiny.en.bin",
        label: "Whisper tiny (English, fastest)",
        size_bytes: 77_704_715,
        url: concat!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
            "ggml-tiny.en.bin"
        ),
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
    },
    ModelOffer {
        id: "base.en",
        file_name: "ggml-base.en.bin",
        label: "Whisper base (English, recommended)",
        size_bytes: 147_964_211,
        url: concat!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
            "ggml-base.en.bin"
        ),
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    },
    ModelOffer {
        id: "small.en",
        file_name: "ggml-small.en.bin",
        label: "Whisper small (English, higher accuracy)",
        size_bytes: 487_614_201,
        url: concat!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
            "ggml-small.en.bin"
        ),
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
    },
    ModelOffer {
        id: "base",
        file_name: "ggml-base.bin",
        label: "Whisper base (multilingual)",
        size_bytes: 147_951_465,
        url: concat!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
            "ggml-base.bin"
        ),
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    },
];

/// Default English base model: good quality / size balance for first run.
pub fn recommended() -> &'static ModelOffer {
    &CATALOG[1]
}

static DOWNLOAD_LOCK: Mutex<()> = Mutex::new(());

fn support_dir() -> PathBuf {
    crate::logging::support_dir()
}

pub fn models_dir() -> PathBuf {
    support_dir().join("models")
}

pub fn recommended_installed() -> bool {
    models_dir().join(recommended().file_name).is_file()
}

fn offer_by_id(id: &str) -> Option<&'static ModelOffer> {
    CATALOG.iter().find(|o| o.id == id)
}

#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    file_name: String,
    downloaded: u64,
    total: u64,
}

fn hex_sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 1024 * 256];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Download a catalog model (default: recommended base.en). Emits
/// `whimpr://model/progress` on the Hub window while streaming.
pub fn download_recommended(app: AppHandle, model_id: Option<String>) -> Result<String, String> {
    let id = model_id.as_deref().unwrap_or("base.en");
    let offer = offer_by_id(id).ok_or_else(|| format!("unknown model id: {id}"))?;

    let _guard = DOWNLOAD_LOCK
        .lock()
        .map_err(|_| "another model download is already in progress".to_string())?;

    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(offer.file_name);
    if dest.is_file() {
        if let Ok(digest) = hex_sha256_file(&dest) {
            if digest.eq_ignore_ascii_case(offer.sha256) {
                crate::hotkey::reload_asr();
                return Ok(dest.display().to_string());
            }
        }
    }

    let partial = dir.join(format!("{}.partial", offer.file_name));
    let _ = std::fs::remove_file(&partial);

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("WhimprFlow/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(offer.url)
        .send()
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download HTTP error: {e}"))?;

    let total = resp.content_length().unwrap_or(offer.size_bytes);
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut resp, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        let _ = app.emit(
            "whimpr://model/progress",
            ProgressPayload {
                file_name: offer.file_name.to_string(),
                downloaded,
                total,
            },
        );
    }
    drop(file);

    let digest = format!("{:x}", hasher.finalize());
    if !digest.eq_ignore_ascii_case(offer.sha256) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "checksum mismatch for {} (got {digest}, expected {})",
            offer.file_name, offer.sha256
        ));
    }

    std::fs::rename(&partial, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        format!("finalize model file: {e}")
    })?;

    let verify = hex_sha256_file(&dest).map_err(|e| e.to_string())?;
    if !verify.eq_ignore_ascii_case(offer.sha256) {
        let _ = std::fs::remove_file(&dest);
        return Err("post-rename checksum failed".into());
    }

    let _ = HF; // keep const for future mirror switches
    crate::hotkey::reload_asr();
    Ok(dest.display().to_string())
}

pub fn list_offers() -> Vec<ModelOffer> {
    CATALOG.to_vec()
}
