/// Embedding dimension for GTE multilingual base.
pub const EMBEDDING_DIMENSION: usize = 768;

/// Subdirectory name under the model cache for GTE multilingual base files.
pub const MODEL_SUBDIR: &str = "gte-multilingual-base";

/// ONNX model file name (int8 quantized).
pub const MODEL_ONNX_FILE: &str = "model_int8.onnx";

/// Tokenizer file name.
pub const MODEL_TOKENIZER_FILE: &str = "tokenizer.json";

/// Model config file name.
pub const MODEL_CONFIG_FILE: &str = "config.json";

/// Special tokens map file name.
pub const MODEL_SPECIAL_TOKENS_FILE: &str = "special_tokens_map.json";

/// Tokenizer config file name.
pub const MODEL_TOKENIZER_CONFIG_FILE: &str = "tokenizer_config.json";

/// Target chunk size in tokens for markdown-aware sub-chunking.
pub const CHUNK_TARGET_TOKENS: usize = 256;

/// Number of overlap tokens between adjacent sub-chunks within a turn.
pub const CHUNK_OVERLAP_TOKENS: usize = 40;

/// Prefix that identifies compaction summary messages.
///
/// User messages starting with this prefix are post-compaction summaries
/// injected by Claude Code after context window compaction.
pub const COMPACTION_SUMMARY_PREFIX: &str =
    "This session is being continued from a previous conversation";
