//! On-device LLM inference via `candle` (pure Rust, no bundled C runtime).
//!
//! Loads the selected GGUF model (see [`crate::llm::resolve_llm`]) and runs
//! greedy/ sampled generation. The tokenizer is rebuilt straight from the GGUF
//! metadata (`TokenizerFromGguf`), so no separate `tokenizer.json` is needed.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use candle_core::quantized::gguf_file;
use candle_core::quantized::tokenizer::TokenizerFromGguf;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_llama::ModelWeights;
use tokenizers::Tokenizer;

/// A loaded model plus its tokenizer, kept resident between calls.
struct Loaded {
    model: ModelWeights,
    tokenizer: Tokenizer,
    device: Device,
    eos: Vec<u32>,
    path: PathBuf,
}

static MODEL: OnceLock<Mutex<Option<Loaded>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Loaded>> {
    MODEL.get_or_init(|| Mutex::new(None))
}

/// Stop tokens: the GGUF `eos_token_id` plus well-known end-of-turn markers.
fn stop_tokens(content: &gguf_file::Content, tokenizer: &Tokenizer) -> Vec<u32> {
    let mut ids = Vec::new();
    if let Some(v) = content.metadata.get("tokenizer.ggml.eos_token_id") {
        if let gguf_file::Value::U32(id) = v {
            ids.push(*id);
        }
    }
    let vocab = tokenizer.get_vocab(true);
    for s in [
        "<|im_end|>",
        "<|endoftext|>",
        "<|end_of_text|>",
        "<|eot_id|>",
        "</s>",
    ] {
        if let Some(id) = vocab.get(s) {
            ids.push(*id);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn load(path: &Path) -> Result<Loaded, String> {
    let device = Device::Cpu;
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let content = gguf_file::Content::read(&mut file).map_err(|e| format!("read gguf: {e}"))?;
    let tokenizer =
        Tokenizer::from_gguf(&content).map_err(|e| format!("build tokenizer: {e}"))?;
    let eos = stop_tokens(&content, &tokenizer);
    let model = ModelWeights::from_gguf(content, &mut file, &device)
        .map_err(|e| format!("load weights: {e}"))?;
    Ok(Loaded {
        model,
        tokenizer,
        device,
        eos,
        path: path.to_path_buf(),
    })
}

impl Loaded {
    /// Format an instruct prompt using ChatML when the vocab supports it.
    fn format_prompt(&self, user: &str) -> String {
        if self.tokenizer.get_vocab(true).contains_key("<|im_start|>") {
            format!("<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n")
        } else {
            user.to_string()
        }
    }

    fn generate(&mut self, user: &str, max_new: usize, temperature: f64) -> Result<String, String> {
        let prompt = self.format_prompt(user);
        let encoding = self
            .tokenizer
            .encode(prompt, false)
            .map_err(|e| format!("encode: {e}"))?;
        let mut tokens = encoding.get_ids().to_vec();
        if tokens.is_empty() {
            return Err("empty prompt".into());
        }

        let sampling = if temperature <= 0.0 {
            Sampling::ArgMax
        } else {
            Sampling::All { temperature }
        };
        let mut sampler = LogitsProcessor::from_sampling(42, sampling);

        let input = Tensor::new(tokens.as_slice(), &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| format!("input tensor: {e}"))?;
        let mut logits = self
            .model
            .forward(&input, 0)
            .and_then(|l| l.squeeze(0))
            .map_err(|e| format!("forward: {e}"))?;
        let mut pos = tokens.len();
        let mut next = sampler
            .sample(&logits)
            .map_err(|e| format!("sample: {e}"))?;

        let mut out: Vec<u32> = Vec::new();
        while out.len() < max_new && !self.eos.contains(&next) {
            out.push(next);
            let input = Tensor::new(&[next], &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| format!("step tensor: {e}"))?;
            logits = self
                .model
                .forward(&input, pos)
                .and_then(|l| l.squeeze(0))
                .map_err(|e| format!("forward: {e}"))?;
            next = sampler.sample(&logits).map_err(|e| format!("sample: {e}"))?;
            pos += 1;
        }

        tokens.clear();
        self.tokenizer
            .decode(&out, true)
            .map_err(|e| format!("decode: {e}"))
            .map(|s| s.trim().to_string())
    }
}

/// Run the model on `prompt`, deviating from greedy only when `temperature > 0`.
/// The model is loaded on first use and kept resident for later calls.
pub fn generate(prompt: &str, max_new: usize, temperature: f64) -> Result<String, String> {
    let path = crate::llm::resolve_llm().ok_or_else(|| "no LLM selected".to_string())?;
    let mut guard = slot().lock().map_err(|_| "model lock poisoned".to_string())?;
    let stale = guard.as_ref().map(|l| l.path != path).unwrap_or(true);
    if stale {
        *guard = Some(load(&path)?);
    }
    guard.as_mut().unwrap().generate(prompt, max_new, temperature)
}

/// Drop the resident model, freeing its memory.
pub fn unload() {
    if let Ok(mut guard) = slot().lock() {
        *guard = None;
    }
}

/// Whether a GGUF model is selected and loadable.
pub fn available() -> bool {
    crate::llm::resolve_llm().is_some()
}

/// Warm the model up (load only); useful to hide the first-token latency.
pub fn warmup() -> Result<(), String> {
    let path = crate::llm::resolve_llm().ok_or_else(|| "no LLM selected".to_string())?;
    let mut guard = slot().lock().map_err(|_| "model lock poisoned".to_string())?;
    if guard.as_ref().map(|l| l.path != path).unwrap_or(true) {
        *guard = Some(load(&path)?);
    }
    Ok(())
}

/// A tiny smoke test: ask the model to reply with a single word.
pub fn smoke_test() -> Result<String, String> {
    generate("Reply with the single word: OK", 8, 0.0)
}
