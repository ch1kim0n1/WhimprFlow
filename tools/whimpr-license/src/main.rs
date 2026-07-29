//! Issue WhimprFlow offline license keys (ed25519).
//!
//! Private key sources (first match wins):
//! 1. `WHIMPR_LICENSE_PRIVATE_KEY_HEX` (64 hex chars)
//! 2. `secrets/license-private.hex` relative to cwd / repo root

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use whimpr_core::{LicensePayload, LicenseTier};

#[derive(Parser)]
#[command(name = "whimpr-license", about = "Issue WhimprFlow license keys")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Sign a license for a customer email.
    Issue {
        #[arg(long)]
        email: String,
        #[arg(long, default_value = "pro")]
        tier: String,
        /// Days until expiry. Omit for never-expires.
        #[arg(long)]
        days: Option<u64>,
    },
    /// Verify a license key against the embedded public key.
    Verify {
        #[arg(long)]
        key: String,
    },
}

fn load_signing_key() -> Result<SigningKey> {
    if let Ok(hex) = std::env::var("WHIMPR_LICENSE_PRIVATE_KEY_HEX") {
        return signing_from_hex(hex.trim());
    }
    for candidate in [
        PathBuf::from("secrets/license-private.hex"),
        PathBuf::from("../secrets/license-private.hex"),
        PathBuf::from("../../secrets/license-private.hex"),
    ] {
        if candidate.is_file() {
            let hex = std::fs::read_to_string(&candidate)
                .with_context(|| format!("read {}", candidate.display()))?;
            return signing_from_hex(hex.trim());
        }
    }
    bail!("set WHIMPR_LICENSE_PRIVATE_KEY_HEX or place secrets/license-private.hex")
}

fn signing_from_hex(hex: &str) -> Result<SigningKey> {
    if hex.len() != 64 {
        bail!("private key hex must be 64 chars, got {}", hex.len());
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .with_context(|| format!("hex byte {i}"))?;
    }
    Ok(SigningKey::from_bytes(&bytes))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Issue { email, tier, days } => {
            let tier = match tier.to_ascii_lowercase().as_str() {
                "pro" => LicenseTier::Pro,
                "trial" => LicenseTier::Trial,
                other => bail!("unknown tier {other} (use pro|trial)"),
            };
            let exp = days.map(|d| now_unix().saturating_add(d * 24 * 60 * 60));
            let payload = LicensePayload {
                v: 1,
                email,
                tier,
                exp,
            };
            let body = serde_json::to_vec(&payload)?;
            let signing = load_signing_key()?;
            let sig = signing.sign(&body);
            let key = format!(
                "WF1.{}.{}",
                URL_SAFE_NO_PAD.encode(&body),
                URL_SAFE_NO_PAD.encode(sig.to_bytes())
            );
            println!("{key}");
        }
        Cmd::Verify { key } => {
            let payload = whimpr_core::verify_license_key(&key)?;
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
    }
    Ok(())
}
