//! Local-LLM cleanup worker.
//!
//! Loads a GGUF instruction model once, then serves one request per line of stdin:
//! `{"system": "...", "user": "..."}` → `{"text": "..."}` on stdout. The WhimprFlow
//! app spawns this and keeps it warm so cleanup is fast and fully offline.
//!
//! Usage: `whimpr-llm-worker <model.gguf> [--n_ctx N] [--n-predict N]`
//! (or WHIMPR_LLM_MODEL env var for the model path).

use std::io::{BufRead, Write};
use std::num::NonZeroU32;

use anyhow::Context as _;
use encoding_rs::UTF_8;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Msg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct Request {
    /// Full multi-turn message list (system + few-shot + user). Preferred.
    #[serde(default)]
    messages: Vec<Msg>,
    /// Back-compat single-turn form, used only when `messages` is empty.
    #[serde(default)]
    system: String,
    #[serde(default)]
    user: String,
    #[serde(default = "default_max")]
    max_tokens: i32,
}
fn default_max() -> i32 {
    400
}

#[derive(Serialize)]
struct Response {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct WorkerArgs {
    model_path: String,
    n_ctx: u32,
    n_predict: i32,
}

fn parse_args() -> anyhow::Result<WorkerArgs> {
    let mut model_path: Option<String> = None;
    let mut n_ctx: u32 = 4096;
    let mut n_predict: i32 = 512;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--n_ctx" | "--n-ctx" => {
                let v = args
                    .next()
                    .context("--n_ctx requires a value")?
                    .parse::<u32>()
                    .context("bad --n_ctx")?;
                n_ctx = v.max(512);
            }
            "--n-predict" | "--n_predict" => {
                let v = args
                    .next()
                    .context("--n-predict requires a value")?
                    .parse::<i32>()
                    .context("bad --n-predict")?;
                n_predict = v.max(1);
            }
            other if other.starts_with('-') => {
                anyhow::bail!("unknown flag: {other}");
            }
            other => {
                if model_path.is_some() {
                    anyhow::bail!("unexpected argument: {other}");
                }
                model_path = Some(other.to_string());
            }
        }
    }
    let model_path = model_path
        .or_else(|| std::env::var("WHIMPR_LLM_MODEL").ok())
        .context("model path required (argv or WHIMPR_LLM_MODEL)")?;
    Ok(WorkerArgs {
        model_path,
        n_ctx,
        n_predict,
    })
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    eprintln!(
        "[llm-worker] n_ctx={} n_predict={}",
        args.n_ctx, args.n_predict
    );

    let backend = LlamaBackend::init()?;
    // Offload everything to the Apple GPU (Metal)  -  capped by what fits.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(999);
    let model = LlamaModel::load_from_file(&backend, &args.model_path, &model_params)
        .with_context(|| format!("failed to load model {}", args.model_path))?;
    eprintln!("[llm-worker] model loaded, ready");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(mut req) => {
                req.max_tokens = req.max_tokens.min(args.n_predict);
                match generate(&backend, &model, &req, args.n_ctx) {
                    Ok(text) => Response { text, error: None },
                    Err(e) => Response {
                        text: String::new(),
                        error: Some(e.to_string()),
                    },
                }
            }
            Err(e) => Response {
                text: String::new(),
                error: Some(format!("bad request: {e}")),
            },
        };
        serde_json::to_writer(&mut stdout, &resp)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

fn generate(
    backend: &LlamaBackend,
    model: &LlamaModel,
    req: &Request,
    n_ctx: u32,
) -> anyhow::Result<String> {
    // Qwen2.5 ChatML template. Prefer the full multi-turn message list (few-shot
    // demonstrations drive the newline/list/self-correction behavior); fall back
    // to the legacy single system+user pair.
    let mut prompt = String::new();
    if req.messages.is_empty() {
        prompt.push_str(&format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n",
            req.system, req.user
        ));
    } else {
        for m in &req.messages {
            prompt.push_str(&format!(
                "<|im_start|>{}\n{}<|im_end|>\n",
                m.role, m.content
            ));
        }
    }
    prompt.push_str("<|im_start|>assistant\n");

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(n_ctx));
    let mut ctx = model.new_context(backend, ctx_params)?;

    let tokens = model.str_to_token(&prompt, AddBos::Always)?;
    let n_prompt = tokens.len() as i32;

    let mut batch = LlamaBatch::new(n_ctx as usize, 1);
    let last = tokens.len() - 1;
    for (i, tok) in tokens.iter().enumerate() {
        batch.add(*tok, i as i32, &[0], i == last)?;
    }
    ctx.decode(&mut batch)?;

    let mut sampler = LlamaSampler::greedy();
    let mut decoder = UTF_8.new_decoder();
    let mut n_cur = batch.n_tokens();
    let mut out = String::new();
    let limit = n_prompt + req.max_tokens;

    while n_cur <= limit {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }
        out.push_str(&model.token_to_piece(token, &mut decoder, true, None)?);
        batch.clear();
        batch.add(token, n_cur, &[0], true)?;
        n_cur += 1;
        ctx.decode(&mut batch)?;
    }
    Ok(out.trim().to_string())
}
