use std::io::{Read, Write};

use mementor_lib::output::ConsoleIO;
use mementor_lib::runtime::Runtime;

/// Hugging Face repository for the GTE multilingual base model.
const HF_REPO: &str = "onnx-community/gte-multilingual-base";

/// Subdirectory name under `model_cache_dir` for GTE multilingual base files.
const MODEL_SUBDIR: &str = "gte-multilingual-base";

/// Files to download from the Hugging Face repository.
const MODEL_FILES: &[(&str, &str)] = &[
    ("onnx/model_int8.onnx", "model_int8.onnx"),
    ("tokenizer.json", "tokenizer.json"),
    ("config.json", "config.json"),
    ("special_tokens_map.json", "special_tokens_map.json"),
    ("tokenizer_config.json", "tokenizer_config.json"),
];

/// Run the `mementor model download` command.
pub fn run_model_download<IN, OUT, ERR>(
    force: bool,
    runtime: &Runtime,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    let model_dir = runtime.context.model_cache_dir().join(MODEL_SUBDIR);

    if force && model_dir.exists() {
        writeln!(io.stderr(), "Removing existing model files...")?;
        std::fs::remove_dir_all(&model_dir)?;
    }

    // Check if all files already exist
    if !force
        && MODEL_FILES
            .iter()
            .all(|(_, local)| model_dir.join(local).exists())
    {
        writeln!(
            io.stdout(),
            "Model already downloaded at {}",
            model_dir.display()
        )?;
        return Ok(());
    }

    std::fs::create_dir_all(&model_dir)?;

    writeln!(io.stderr(), "Downloading GTE multilingual base int8...")?;
    let api = hf_hub::api::sync::Api::new()?;
    let repo = api.model(HF_REPO.to_string());

    for (remote_path, local_name) in MODEL_FILES {
        let dest = model_dir.join(local_name);
        if dest.exists() {
            writeln!(io.stderr(), "  {local_name} (cached)")?;
            continue;
        }
        writeln!(io.stderr(), "  {local_name}...")?;
        let cached = repo.get(remote_path)?;
        std::fs::copy(&cached, &dest)?;
    }

    writeln!(io.stdout(), "Model downloaded to {}", model_dir.display())?;
    Ok(())
}
