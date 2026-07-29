//! Shared HTTP client policy for cloud cleanup providers.

use std::net::IpAddr;
use std::time::Duration;

use url::Url;

/// Build the hardened blocking client used by every cloud provider.
pub fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        // Never follow redirects: a malicious base_url could bounce the Bearer
        // token to an attacker-controlled host.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("WhimprFlow/", env!("CARGO_PKG_VERSION")))
        // http is allowed only after validate_base_url permits loopback.
        .build()
        .expect("failed to build HTTP client")
}

/// Validate a user-supplied OpenAI-compatible API root before we ever send a
/// bearer token to it. Rejects non-HTTPS (except loopback HTTP for local
/// servers), credentials-in-URL, and private / link-local hosts that are not
/// loopback.
pub fn validate_base_url(base_url: &str) -> anyhow::Result<()> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let parsed = Url::parse(trimmed).map_err(|e| anyhow::anyhow!("invalid base_url: {e}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("base_url must include a host"))?;

    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("base_url must not embed credentials");
    }

    let is_loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);

    match parsed.scheme() {
        "https" => {}
        "http" if is_loopback => {}
        other => {
            anyhow::bail!("base_url scheme must be https (or http to localhost only), got {other}")
        }
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if !ip.is_loopback() && is_private_or_link_local(ip) {
            anyhow::bail!(
                "base_url host resolves to a private or link-local address ({ip}); \
                 use a public HTTPS endpoint, or localhost for a local server"
            );
        }
    } else if looks_like_private_hostname(host) {
        anyhow::bail!(
            "base_url host looks like a private hostname ({host}); \
             use a public HTTPS endpoint, or localhost for a local server"
        );
    }

    Ok(())
}

fn is_private_or_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            // Unique local (fc00::/7) and link-local (fe80::/10).
            (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

fn looks_like_private_hostname(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h.ends_with(".local")
        || h.ends_with(".internal")
        || h.ends_with(".lan")
        || h == "metadata.google.internal"
}

/// Simple sliding-window rate limit shared by cloud cleanup calls in this
/// process. Caps runaway spend if a hotkey gets stuck.
pub mod rate {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    static WINDOW: Mutex<Window> = Mutex::new(Window {
        start: None,
        count: 0,
    });

    struct Window {
        start: Option<Instant>,
        count: u32,
    }

    /// Max cloud cleanup / command-edit calls per rolling minute.
    pub const MAX_PER_MINUTE: u32 = 60;

    pub fn check() -> anyhow::Result<()> {
        let mut w = WINDOW.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        match w.start {
            Some(start) if now.duration_since(start) < Duration::from_secs(60) => {
                if w.count >= MAX_PER_MINUTE {
                    anyhow::bail!(
                        "cloud cleanup rate limit exceeded ({MAX_PER_MINUTE}/min); \
                         wait a moment or switch Cleanup Engine to Raw / Local"
                    );
                }
                w.count += 1;
            }
            _ => {
                w.start = Some(now);
                w.count = 1;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn reset_for_tests() {
        let mut w = WINDOW.lock().unwrap_or_else(|e| e.into_inner());
        w.start = None;
        w.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_public() {
        validate_base_url("https://openrouter.ai/api/v1").unwrap();
    }

    #[test]
    fn accepts_localhost_http() {
        validate_base_url("http://127.0.0.1:11434/v1").unwrap();
        validate_base_url("http://localhost:8080/v1").unwrap();
    }

    #[test]
    fn rejects_private_lan() {
        assert!(validate_base_url("https://192.168.1.10/v1").is_err());
        assert!(validate_base_url("https://10.0.0.5/v1").is_err());
        assert!(validate_base_url("http://192.168.0.1/v1").is_err());
    }

    #[test]
    fn rejects_embedded_credentials() {
        assert!(validate_base_url("https://user:pass@evil.example/v1").is_err());
    }
}
