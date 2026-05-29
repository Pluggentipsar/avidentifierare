//! Swedish NER inference (KB-BERT via ONNX Runtime + HuggingFace tokenizers).
//!
//! The model uses the SUCX non-BIO tag set (e.g. `PER`, `LOC`, `ORG`, `TME`, `EVN`, plus
//! ambiguous compounds like `ORG/PRS`). We map every tag that *could* be a person to
//! [`Category::Person`] for a safety-first de-identification.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

use super::{Category, Source, Span};

/// Token budget per forward pass, leaving room for [CLS] and [SEP] within BERT's 512 limit.
const MAX_TOKENS: usize = 510;

pub struct NerModel {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    id2label: Vec<String>,
    cls_id: i64,
    sep_id: i64,
}

impl NerModel {
    pub fn load(model_path: &Path, tokenizer_path: &Path, labels_path: &Path) -> Result<Self> {
        let session = Session::builder()?
            .commit_from_file(model_path)
            .with_context(|| format!("kunde inte ladda modellen: {}", model_path.display()))?;

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow!("kunde inte ladda tokenizer: {e}"))?;

        let raw: HashMap<String, String> = serde_json::from_slice(
            &std::fs::read(labels_path)
                .with_context(|| format!("kunde inte läsa {}", labels_path.display()))?,
        )?;
        let mut id2label = vec![String::new(); raw.len()];
        for (k, v) in raw {
            let i: usize = k.parse().context("ogiltigt etikett-id i labels.json")?;
            if i < id2label.len() {
                id2label[i] = v;
            }
        }

        let cls_id = tokenizer
            .token_to_id("[CLS]")
            .ok_or_else(|| anyhow!("tokenizer saknar [CLS]"))? as i64;
        let sep_id = tokenizer
            .token_to_id("[SEP]")
            .ok_or_else(|| anyhow!("tokenizer saknar [SEP]"))? as i64;

        Ok(Self { session: Mutex::new(session), tokenizer, id2label, cls_id, sep_id })
    }

    /// Run NER over the full text, chunking past the model's token limit, and return merged spans.
    pub fn detect(&self, text: &str) -> Result<Vec<Span>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let enc = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow!("tokenisering misslyckades: {e}"))?;
        let ids = enc.get_ids();
        let offsets = enc.get_offsets();
        let num_labels = self.id2label.len();

        let mut token_spans: Vec<(Category, usize, usize)> = Vec::new();

        for chunk_start in (0..ids.len()).step_by(MAX_TOKENS) {
            let chunk_end = (chunk_start + MAX_TOKENS).min(ids.len());

            let mut input_ids = Vec::with_capacity(chunk_end - chunk_start + 2);
            input_ids.push(self.cls_id);
            input_ids.extend(ids[chunk_start..chunk_end].iter().map(|&x| x as i64));
            input_ids.push(self.sep_id);
            let len = input_ids.len();
            let attention = vec![1i64; len];
            let token_type = vec![0i64; len];

            let labels = {
                let mut session = self.session.lock().unwrap();
                let outputs = session.run(ort::inputs! {
                    "input_ids" => Tensor::from_array(([1usize, len], input_ids))?,
                    "attention_mask" => Tensor::from_array(([1usize, len], attention))?,
                    "token_type_ids" => Tensor::from_array(([1usize, len], token_type))?,
                })?;
                let (_, data) = outputs["logits"].try_extract_tensor::<f32>()?;
                argmax_per_position(data, len, num_labels)
            };

            // Positions 1..len-1 map to source tokens chunk_start..chunk_end (skip [CLS]/[SEP]).
            for (pos, &label_id) in labels.iter().enumerate() {
                if pos == 0 || pos == len - 1 {
                    continue;
                }
                let tok = chunk_start + (pos - 1);
                let (s, e) = offsets[tok];
                if e <= s {
                    continue;
                }
                if let Some(cat) = map_label(&self.id2label[label_id]) {
                    token_spans.push((cat, s, e));
                }
            }
        }

        Ok(merge_token_spans(text, token_spans))
    }
}

fn argmax_per_position(data: &[f32], positions: usize, num_labels: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(positions);
    for pos in 0..positions {
        let base = pos * num_labels;
        let mut best = 0usize;
        let mut best_val = f32::MIN;
        for l in 0..num_labels {
            let v = data[base + l];
            if v > best_val {
                best_val = v;
                best = l;
            }
        }
        out.push(best);
    }
    out
}

/// Map a SUCX tag to a PII category, prioritising Person for ambiguous compound tags.
fn map_label(label: &str) -> Option<Category> {
    if label == "PER" || label.contains("PRS") {
        return Some(Category::Person);
    }
    if label.contains("LOC") {
        return Some(Category::Plats);
    }
    if label.contains("ORG") {
        return Some(Category::Organisation);
    }
    match label {
        "TME" => Some(Category::Tid),
        "EVN" => Some(Category::Handelse),
        _ => None,
    }
}

/// Merge consecutive same-category tokens (separated only by whitespace) into one span.
fn merge_token_spans(text: &str, tokens: Vec<(Category, usize, usize)>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for (cat, s, e) in tokens {
        if let Some(last) = out.last_mut() {
            if last.category == cat
                && s >= last.end
                && text[last.end..s].chars().all(char::is_whitespace)
            {
                last.end = e;
                last.text = text[last.start..e].to_string();
                continue;
            }
        }
        out.push(Span::new(s, e, &text[s..e], cat, Source::Model, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ambiguous_tags_to_person() {
        assert_eq!(map_label("PER"), Some(Category::Person));
        assert_eq!(map_label("ORG/PRS"), Some(Category::Person));
        assert_eq!(map_label("LOC/PRS"), Some(Category::Person));
        assert_eq!(map_label("LOC"), Some(Category::Plats));
        assert_eq!(map_label("ORG"), Some(Category::Organisation));
        assert_eq!(map_label("O"), None);
        assert_eq!(map_label("MSR"), None);
    }

    /// Loads the real bundled model and runs NER. Run with:
    /// `cargo test --lib smoke_detect_real_model -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn smoke_detect_real_model() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/model");
        let m = NerModel::load(
            &base.join("model.onnx"),
            &base.join("tokenizer.json"),
            &base.join("labels.json"),
        )
        .unwrap();
        let spans = m
            .detect("Anna Svensson bor i Stockholm och arbetar på Volvo sedan 2019.")
            .unwrap();
        println!("SPANS: {spans:#?}");
        assert!(spans.iter().any(|s| s.category == Category::Person));
        assert!(spans.iter().any(|s| s.category == Category::Plats));
    }
}
