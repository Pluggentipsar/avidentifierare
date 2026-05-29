mod ai;
mod docio;
mod engine;
mod pii;

use std::path::{Path, PathBuf};

use engine::{AnalyzeResult, Engine, ModelPaths};
use pii::Category;
use tauri::{Manager, State};

#[tauri::command]
fn analyze_text(
    engine: State<Engine>,
    text: String,
    enabled: Vec<Category>,
    terms: Vec<String>,
    use_ai: bool,
) -> Result<AnalyzeResult, String> {
    engine.analyze_text(text, enabled, terms, use_ai).map_err(|e| e.to_string())
}

#[tauri::command]
fn analyze_file(
    engine: State<Engine>,
    path: String,
    enabled: Vec<Category>,
    terms: Vec<String>,
    use_ai: bool,
) -> Result<AnalyzeResult, String> {
    engine.analyze_file(PathBuf::from(path), enabled, terms, use_ai).map_err(|e| e.to_string())
}

#[tauri::command]
fn export(engine: State<Engine>, path: String, rejected: Vec<usize>) -> Result<(), String> {
    engine.export(PathBuf::from(path), rejected).map_err(|e| e.to_string())
}

#[tauri::command]
fn anonymized_text(engine: State<Engine>, rejected: Vec<usize>) -> Result<String, String> {
    engine.anonymized_text(rejected).map_err(|e| e.to_string())
}

#[tauri::command]
fn suggested_output_name(engine: State<Engine>) -> Option<String> {
    engine.suggested_output_name()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let paths = resolve_model_paths(app);
            app.manage(Engine::new(paths));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            analyze_text,
            analyze_file,
            export,
            anonymized_text,
            suggested_output_name
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Resolve the bundled model files, falling back to the source tree during development.
fn resolve_model_paths(app: &tauri::App) -> ModelPaths {
    let resource_dir = app.path().resource_dir().ok();
    let pick = |rel: &str| -> PathBuf {
        if let Some(rd) = &resource_dir {
            let p = rd.join("resources").join("model").join(rel);
            if p.exists() {
                return p;
            }
        }
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("model").join(rel)
    };
    let pick_llm = |rel: &str| -> PathBuf {
        if let Some(rd) = &resource_dir {
            let p = rd.join("resources").join("llm").join(rel);
            if p.exists() {
                return p;
            }
        }
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("llm").join(rel)
    };
    ModelPaths {
        model: pick("model.onnx"),
        tokenizer: pick("tokenizer.json"),
        labels: pick("labels.json"),
        llm_model: pick_llm("model.gguf"),
        llm_tokenizer: pick_llm("tokenizer.json"),
    }
}
