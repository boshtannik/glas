//! On-device LLM inference via `candle` (pure Rust, no bundled C runtime).
//!
//! Loads the selected GGUF model (see [`crate::llm::resolve_llm`]) and runs
//! greedy/ sampled generation. The tokenizer is rebuilt straight from the GGUF
//! metadata (`TokenizerFromGguf`), so no separate `tokenizer.json` is needed.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use candle_core::quantized::gguf_file;
use candle_core::quantized::tokenizer::TokenizerFromGguf;
use candle_core::{DType, Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::{
    quantized_gemma3, quantized_glm4, quantized_lfm2, quantized_llama, quantized_phi,
    quantized_phi3, quantized_qwen2, quantized_qwen3,
};
use tokenizers::Tokenizer;

/// The model architectures we can run, chosen from the GGUF metadata
/// (`general.architecture`). All of them expose the same `forward(&x, pos)`.
enum Model {
    Llama(quantized_llama::ModelWeights),
    Qwen2(quantized_qwen2::ModelWeights),
    Qwen3(quantized_qwen3::ModelWeights),
    Gemma3(quantized_gemma3::ModelWeights),
    Phi(quantized_phi::ModelWeights),
    Phi3(quantized_phi3::ModelWeights),
    Glm4(quantized_glm4::ModelWeights),
    Lfm2(quantized_lfm2::ModelWeights),
}

impl Model {
    fn forward(&mut self, x: &Tensor, pos: usize) -> candle_core::Result<Tensor> {
        match self {
            Model::Llama(m) => m.forward(x, pos),
            Model::Qwen2(m) => m.forward(x, pos),
            Model::Qwen3(m) => m.forward(x, pos),
            Model::Gemma3(m) => m.forward(x, pos),
            Model::Phi(m) => m.forward(x, pos),
            Model::Phi3(m) => m.forward(x, pos),
            Model::Glm4(m) => m.forward(x, pos),
            Model::Lfm2(m) => m.forward(x, pos),
        }
    }
}

/// A loaded model plus its tokenizer, kept resident between calls.
struct Loaded {
    model: Model,
    tokenizer: Tokenizer,
    device: Device,
    eos: Vec<u32>,
    path: PathBuf,
    /// When the model was last used; drives the idle auto-unload.
    last_used: Instant,
}

static MODEL: OnceLock<Mutex<Option<Loaded>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Loaded>> {
    MODEL.get_or_init(|| Mutex::new(None))
}

/// How often the watchdog checks for an idle model.
const WATCHDOG_TICK: Duration = Duration::from_secs(5);

static WATCHDOG: OnceLock<()> = OnceLock::new();

/// Drop the resident model once it has been idle for the configured number of
/// seconds ([`crate::settings::Settings::idle_unload_secs`]; 0 = never unload).
fn start_watchdog() {
    WATCHDOG.get_or_init(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(WATCHDOG_TICK);
            let secs = crate::settings::Settings::load().idle_unload_secs;
            if secs == 0 {
                continue;
            }
            let idle = Duration::from_secs(secs);
            // `try_lock` so we never block behind a long generation.
            if let Ok(mut guard) = slot().try_lock() {
                let stale = guard
                    .as_ref()
                    .map(|l| l.last_used.elapsed() >= idle)
                    .unwrap_or(false);
                if stale {
                    *guard = None;
                    log::info!("infer: unloaded idle model after {secs}s (freeing RAM)");
                }
            }
        });
    });
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
    let arch = match content.metadata.get("general.architecture") {
        Some(gguf_file::Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    let model = match arch.as_str() {
        "llama" => Model::Llama(
            quantized_llama::ModelWeights::from_gguf(content, &mut file, &device)
                .map_err(|e| format!("load weights (llama): {e}"))?,
        ),
        "qwen2" => Model::Qwen2(
            quantized_qwen2::ModelWeights::from_gguf(content, &mut file, &device)
                .map_err(|e| format!("load weights (qwen2): {e}"))?,
        ),
        "qwen3" => Model::Qwen3(
            quantized_qwen3::ModelWeights::from_gguf(content, &mut file, &device)
                .map_err(|e| format!("load weights (qwen3): {e}"))?,
        ),
        "gemma" | "gemma2" | "gemma3" => Model::Gemma3(
            quantized_gemma3::ModelWeights::from_gguf(content, &mut file, &device)
                .map_err(|e| format!("load weights (gemma3): {e}"))?,
        ),
        "phi2" => Model::Phi(
            quantized_phi::ModelWeights::from_gguf(content, &mut file, &device)
                .map_err(|e| format!("load weights (phi2): {e}"))?,
        ),
        "phi3" => Model::Phi3(
            quantized_phi3::ModelWeights::from_gguf(false, content, &mut file, &device)
                .map_err(|e| format!("load weights (phi3): {e}"))?,
        ),
        "glm4" => Model::Glm4(
            quantized_glm4::ModelWeights::from_gguf(content, &mut file, &device, DType::F32)
                .map_err(|e| format!("load weights (glm4): {e}"))?,
        ),
        "lfm2" => Model::Lfm2(
            quantized_lfm2::ModelWeights::from_gguf(content, &mut file, &device)
                .map_err(|e| format!("load weights (lfm2): {e}"))?,
        ),
        other => return Err(format!("unsupported model architecture: {other:?}")),
    };
    Ok(Loaded {
        model,
        tokenizer,
        device,
        eos,
        path: path.to_path_buf(),
        last_used: Instant::now(),
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

        self.last_used = Instant::now();
        tokens.clear();
        self.tokenizer
            .decode(&out, true)
            .map_err(|e| format!("decode: {e}"))
            .map(|s| s.trim().to_string())
    }
}

/// Ensure the selected model is resident in `guard`, loading it if needed.
fn ensure_loaded(guard: &mut Option<Loaded>) -> Result<&mut Loaded, String> {
    let path = crate::llm::resolve_llm().ok_or_else(|| "no LLM selected".to_string())?;
    let stale = guard.as_ref().map(|l| l.path != path).unwrap_or(true);
    if stale {
        *guard = Some(load(&path)?);
    }
    Ok(guard.as_mut().unwrap())
}

/// Run the model on `prompt`, deviating from greedy only when `temperature > 0`.
/// The model is loaded on first use and kept resident (dropped by the idle
/// watchdog) so follow-up calls are fast.
pub fn generate(prompt: &str, max_new: usize, temperature: f64) -> Result<String, String> {
    start_watchdog();
    let mut guard = slot().lock().map_err(|_| "model lock poisoned".to_string())?;
    ensure_loaded(&mut guard)?.generate(prompt, max_new, temperature)
}

/// Run the model once and drop it immediately afterwards, freeing RAM.
/// Used by the "try the model" test form; the idle watchdog does not apply.
pub fn generate_once(prompt: &str, max_new: usize, temperature: f64) -> Result<String, String> {
    let mut guard = slot().lock().map_err(|_| "model lock poisoned".to_string())?;
    let res = ensure_loaded(&mut guard)?.generate(prompt, max_new, temperature);
    *guard = None;
    log::info!("infer: unloaded model after one-shot generation");
    res
}

/// Whether a GGUF model is selected and loadable.
pub fn available() -> bool {
    crate::llm::resolve_llm().is_some()
}

/// Warm the model up (load only); useful to hide the first-token latency.
pub fn warmup() -> Result<(), String> {
    start_watchdog();
    let mut guard = slot().lock().map_err(|_| "model lock poisoned".to_string())?;
    ensure_loaded(&mut guard)?;
    Ok(())
}

/// A tiny smoke test: ask the model to reply with a single word.
pub fn smoke_test() -> Result<String, String> {
    generate("Reply with the single word: OK", 8, 0.0)
}
