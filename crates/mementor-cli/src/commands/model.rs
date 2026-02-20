use std::io::{Read, Write};

use mementor_lib::config::{
    MODEL_CONFIG_FILE, MODEL_ONNX_FILE, MODEL_SPECIAL_TOKENS_FILE, MODEL_SUBDIR,
    MODEL_TOKENIZER_CONFIG_FILE, MODEL_TOKENIZER_FILE,
};
use mementor_lib::output::ConsoleIO;
use mementor_lib::runtime::Runtime;

/// Hugging Face repository for the GTE multilingual base model.
const HF_REPO: &str = "onnx-community/gte-multilingual-base";

/// Mapping of (remote path in HF repo, local file name) for model files.
const MODEL_FILES: &[(&str, &str)] = &[
    ("onnx/model_int8.onnx", MODEL_ONNX_FILE),
    ("tokenizer.json", MODEL_TOKENIZER_FILE),
    ("config.json", MODEL_CONFIG_FILE),
    ("special_tokens_map.json", MODEL_SPECIAL_TOKENS_FILE),
    ("tokenizer_config.json", MODEL_TOKENIZER_CONFIG_FILE),
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

#[cfg(test)]
mod tests {
    use mementor_lib::config::{
        MODEL_CONFIG_FILE, MODEL_ONNX_FILE, MODEL_SPECIAL_TOKENS_FILE, MODEL_SUBDIR,
        MODEL_TOKENIZER_CONFIG_FILE, MODEL_TOKENIZER_FILE,
    };

    use crate::test_util::runtime_in_memory;

    /// Create fake model files in the given model cache directory.
    fn seed_model_files(model_cache_dir: &std::path::Path) {
        let dir = model_cache_dir.join(MODEL_SUBDIR);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            MODEL_ONNX_FILE,
            MODEL_TOKENIZER_FILE,
            MODEL_CONFIG_FILE,
            MODEL_SPECIAL_TOKENS_FILE,
            MODEL_TOKENIZER_CONFIG_FILE,
        ] {
            std::fs::write(dir.join(name), b"fake").unwrap();
        }
    }

    #[test]
    fn try_run_model_download_already_cached() {
        let (tmp, runtime) = runtime_in_memory("model_download_cached");
        let model_cache = tmp.path().join("models");
        seed_model_files(&model_cache);
        let runtime = mementor_lib::runtime::Runtime {
            context: runtime.context.with_model_cache_dir(model_cache.clone()),
            db: runtime.db,
        };

        let mut io = mementor_lib::output::BufferedIO::new();
        crate::try_run(&["mementor", "model", "download"], &runtime, &mut io).unwrap();

        let expected_path = model_cache.join(MODEL_SUBDIR);
        assert_eq!(
            io.stdout_to_string(),
            format!("Model already downloaded at {}\n", expected_path.display()),
        );
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_model_download_force_removes_existing() {
        let (tmp, runtime) = runtime_in_memory("model_download_force");
        let model_cache = tmp.path().join("models");
        seed_model_files(&model_cache);

        // Verify fake files exist before force download.
        let onnx_path = model_cache.join(MODEL_SUBDIR).join(MODEL_ONNX_FILE);
        assert_eq!(std::fs::read(&onnx_path).unwrap(), b"fake");

        let runtime = mementor_lib::runtime::Runtime {
            context: runtime.context.with_model_cache_dir(model_cache.clone()),
            db: runtime.db,
        };

        let mut io = mementor_lib::output::BufferedIO::new();
        // force=true removes existing files and re-downloads from HF cache.
        // The download may succeed (from local HF cache) or fail (no network).
        let _ = crate::try_run(
            &["mementor", "model", "download", "--force"],
            &runtime,
            &mut io,
        );

        // The stderr should start with the removal message.
        assert!(
            io.stderr_to_string()
                .starts_with("Removing existing model files...\n"),
            "expected removal message, got: {}",
            io.stderr_to_string(),
        );
        // The fake "fake" content should be gone (either replaced by real data
        // or the file no longer exists).
        let content = std::fs::read(&onnx_path).unwrap_or_default();
        assert_ne!(content, b"fake");
    }
}
