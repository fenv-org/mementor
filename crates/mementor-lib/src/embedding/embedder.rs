use std::fs;
use std::path::Path;

use anyhow::Context;
use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use tokenizers::Tokenizer;

use crate::config::EMBEDDING_DIMENSION;

/// Subdirectory name under `model_cache_dir` for GTE multilingual base files.
const MODEL_SUBDIR: &str = "gte-multilingual-base";

/// Wrapper around fastembed's `TextEmbedding` model.
/// Uses GTE multilingual base int8 loaded from disk.
pub struct Embedder {
    model: TextEmbedding,
}

impl Embedder {
    /// Create a new embedder by loading GTE multilingual base int8 from disk.
    ///
    /// Expected files in `model_cache_dir/gte-multilingual-base/`:
    ///   - `model_int8.onnx`
    ///   - `tokenizer.json`
    ///   - `config.json`
    ///   - `special_tokens_map.json`
    ///   - `tokenizer_config.json`
    ///
    /// If the model files are not found, returns an error instructing the user
    /// to run `mementor model download`.
    pub fn new(model_cache_dir: &Path) -> anyhow::Result<Self> {
        let base = model_cache_dir.join(MODEL_SUBDIR);

        let onnx_bytes = fs::read(base.join("model_int8.onnx")).with_context(|| {
            format!(
                "Model not found at {}. Run 'mementor model download' first.",
                base.display()
            )
        })?;

        let tokenizer_files = TokenizerFiles {
            tokenizer_file: fs::read(base.join("tokenizer.json"))
                .context("Missing tokenizer.json")?,
            config_file: fs::read(base.join("config.json")).context("Missing config.json")?,
            special_tokens_map_file: fs::read(base.join("special_tokens_map.json"))
                .context("Missing special_tokens_map.json")?,
            tokenizer_config_file: fs::read(base.join("tokenizer_config.json"))
                .context("Missing tokenizer_config.json")?,
        };

        let user_model =
            UserDefinedEmbeddingModel::new(onnx_bytes, tokenizer_files).with_pooling(Pooling::Cls);

        let model =
            TextEmbedding::try_new_from_user_defined(user_model, InitOptionsUserDefined::default())
                .context("Failed to initialize GTE multilingual base int8 model")?;

        Ok(Self { model })
    }

    /// Access the tokenizer used by the embedding model.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.model.tokenizer
    }

    /// Embed a batch of text strings and return their vector representations.
    pub fn embed_batch(&mut self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let owned: Vec<String> = texts.iter().map(|s| (*s).to_string()).collect();
        let embeddings = self
            .model
            .embed(owned, None)
            .context("Failed to embed texts")?;
        Ok(embeddings)
    }

    /// Return the embedding dimension (768 for GTE multilingual base).
    #[must_use]
    pub const fn dimension() -> usize {
        EMBEDDING_DIMENSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_dir() -> std::path::PathBuf {
        std::env::var("MEMENTOR_MODEL_DIR").map_or_else(
            |_| {
                dirs::home_dir()
                    .expect("home dir")
                    .join(".mementor")
                    .join("models")
            },
            std::path::PathBuf::from,
        )
    }

    #[test]
    fn embedding_dimension_is_768() {
        assert_eq!(Embedder::dimension(), 768);
    }

    #[test]
    fn embed_batch_returns_correct_count() {
        let mut embedder = Embedder::new(&model_dir()).unwrap();
        let texts = &["hello world", "how are you"];
        let embeddings = embedder.embed_batch(texts).unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 768);
        assert_eq!(embeddings[1].len(), 768);
    }

    #[test]
    fn embed_single_text() {
        let mut embedder = Embedder::new(&model_dir()).unwrap();
        let embeddings = embedder.embed_batch(&["test"]).unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), 768);
    }

    #[test]
    fn tokenizer_is_accessible() {
        let embedder = Embedder::new(&model_dir()).unwrap();
        let tokenizer = embedder.tokenizer();
        let encoding = tokenizer.encode("hello world", false).unwrap();
        assert!(!encoding.get_ids().is_empty());
    }
}
