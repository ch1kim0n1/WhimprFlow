//! Cloud cleanup providers. The OpenAI provider (default cloud) sends the shared
//! WhimprFlow system prompt plus the assembled context and returns cleaned text.
//! On any failure the caller falls back to the raw transcript — cleanup is an
//! enhancement, never a gate.

use std::time::Duration;

use whimpr_core::cleanup::{build_messages, CleanupContext, CleanupProvider, ProviderId};

/// Cleanup via the OpenAI Chat Completions API.
pub struct OpenAiProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            api_key,
            model: model.into(),
        }
    }
}

impl CleanupProvider for OpenAiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    fn cleanup(&self, raw: &str, ctx: &CleanupContext) -> anyhow::Result<String> {
        // System prompt + few-shot demonstration turns + the real transcript.
        let messages: Vec<serde_json::Value> = build_messages(raw, ctx)
            .into_iter()
            .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
            .collect();
        let body = serde_json::json!({
            "model": self.model,
            "temperature": 0.2,
            "max_tokens": 512,
            "messages": messages,
        });

        let resp = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()?;

        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().unwrap_or_default();
            anyhow::bail!("OpenAI HTTP {status}: {detail}");
        }

        let v: serde_json::Value = resp.json()?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            anyhow::bail!("OpenAI returned empty content");
        }
        Ok(text)
    }
}

/// Cleanup via the Anthropic Messages API. Same shared system prompt; the only
/// difference from OpenAI is the wire envelope (top-level `system`, `x-api-key`).
pub struct AnthropicProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            api_key,
            model: model.into(),
        }
    }
}

impl CleanupProvider for AnthropicProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn cleanup(&self, raw: &str, ctx: &CleanupContext) -> anyhow::Result<String> {
        // Anthropic takes the system prompt top-level; the few-shot turns and the
        // real transcript go in `messages` as user/assistant turns.
        let mut system = String::new();
        let mut messages: Vec<serde_json::Value> = Vec::new();
        for m in build_messages(raw, ctx) {
            if m.role == "system" {
                system = m.content;
            } else {
                messages.push(serde_json::json!({ "role": m.role, "content": m.content }));
            }
        }
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 512,
            "temperature": 0.2,
            "system": system,
            "messages": messages,
        });

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()?;

        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().unwrap_or_default();
            anyhow::bail!("Anthropic HTTP {status}: {detail}");
        }

        let v: serde_json::Value = resp.json()?;
        let text = v["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            anyhow::bail!("Anthropic returned empty content");
        }
        Ok(text)
    }
}
