//! Optional semantic de-identification layer: a small local LLM (quantized GGUF via candle)
//! that proposes additional verbatim substrings to mask. Its output is treated as just another
//! candidate source that flows through the normal human review — never auto-trusted.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_qwen2::ModelWeights as Qwen2;
use tokenizers::Tokenizer;

const SYSTEM_PROMPT: &str = "Du är ett verktyg för avidentifiering av svensk text. \
Identifiera ALLA direkta och indirekta personuppgifter: namn, platser, organisationer, \
datum, ålder, yrkesroller, kontaktuppgifter, samt indirekta ledtrådar som i kombination \
kan identifiera en person. Svara ENBART med en JSON-array av exakta, ordagranna textutdrag \
ur texten. Inga förklaringar, ingen annan text.";

pub struct LlmDetector {
    model: Mutex<Qwen2>,
    tokenizer: Tokenizer,
    eos: u32,
    device: Device,
}

impl LlmDetector {
    pub fn load(gguf_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let device = Device::Cpu;
        let mut file = std::fs::File::open(gguf_path)
            .map_err(|e| anyhow!("kunde inte öppna {}: {e}", gguf_path.display()))?;
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| anyhow!("kunde inte läsa GGUF: {e}"))?;
        let model = Qwen2::from_gguf(content, &mut file, &device)?;

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow!("kunde inte ladda tokenizer: {e}"))?;
        let eos = *tokenizer
            .get_vocab(true)
            .get("<|im_end|>")
            .ok_or_else(|| anyhow!("tokenizer saknar <|im_end|>"))?;

        Ok(Self { model: Mutex::new(model), tokenizer, eos, device })
    }

    /// Ask the model for verbatim substrings that should be masked.
    pub fn propose(&self, text: &str) -> Result<Vec<String>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let prompt = format!(
            "<|im_start|>system\n{SYSTEM_PROMPT}<|im_end|>\n<|im_start|>user\n{text}<|im_end|>\n<|im_start|>assistant\n"
        );
        let output = self.generate(&prompt, 512)?;
        Ok(parse_json_strings(&output))
    }

    fn generate(&self, prompt: &str, max_new: usize) -> Result<String> {
        let enc = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| anyhow!("tokenisering misslyckades: {e}"))?;
        let tokens = enc.get_ids().to_vec();

        let mut model = self.model.lock().unwrap();
        let mut lp = LogitsProcessor::new(42, None, None); // greedy = deterministisk

        let input = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
        let logits = model.forward(&input, 0)?.squeeze(0)?;
        let mut next = lp.sample(&logits)?;

        let mut generated = vec![next];
        for i in 0..max_new {
            if next == self.eos {
                break;
            }
            let input = Tensor::new(&[next], &self.device)?.unsqueeze(0)?;
            let logits = model.forward(&input, tokens.len() + i)?.squeeze(0)?;
            next = lp.sample(&logits)?;
            generated.push(next);
        }
        if generated.last() == Some(&self.eos) {
            generated.pop();
        }
        self.tokenizer.decode(&generated, true).map_err(|e| anyhow!("avkodning misslyckades: {e}"))
    }
}

/// Leniently pull a JSON array of strings (or objects with a `text` field) from the model output.
fn parse_json_strings(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(a), Some(b)) = (s.find('['), s.rfind(']')) else {
        return out;
    };
    if b <= a {
        return out;
    }
    if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&s[a..=b]) {
        for v in arr {
            if let Some(t) = v.as_str() {
                out.push(t.to_string());
            } else if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
                out.push(t.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loads the bundled LLM and runs it on the sample text. Run with:
    /// `cargo test --lib ai::tests::smoke_llm -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn smoke_llm() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/llm");
        let d = LlmDetector::load(&base.join("model.gguf"), &base.join("tokenizer.json")).unwrap();
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../samples/exempel-journal.txt"),
        )
        .unwrap();
        let proposals = d.propose(&text).unwrap();
        println!("AI-FÖRSLAG ({}):\n{proposals:#?}", proposals.len());
        assert!(!proposals.is_empty());
    }

    #[test]
    fn parses_json_array() {
        let s = "Här: [\"Anna Svensson\", \"Lund\"] klart";
        assert_eq!(parse_json_strings(s), vec!["Anna Svensson", "Lund"]);
    }
}
