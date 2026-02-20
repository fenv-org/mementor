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

/// Default number of top-k results for vector similarity search.
pub const DEFAULT_TOP_K: usize = 5;

/// Multiplier for over-fetching candidates from vector search.
///
/// The search fetches `k * OVER_FETCH_MULTIPLIER` candidates, then applies
/// post-filters (in-context removal, distance threshold, turn dedup) to
/// produce the final `k` results.
pub const OVER_FETCH_MULTIPLIER: usize = 4;

/// Maximum cosine distance for a search result to be considered relevant.
///
/// Results with distance above this threshold are discarded.
/// GTE multilingual base int8: relevant ≈ 0.19-0.36, irrelevant ≈ 0.38-0.58.
pub const MAX_COSINE_DISTANCE: f64 = 0.45;

/// Synthetic distance assigned to file-path-only matches in hybrid search.
///
/// Set below `MAX_COSINE_DISTANCE` (0.45) so file matches survive the distance
/// threshold, but above typical strong semantic matches (~0.26) so they rank
/// lower than genuine vector similarity hits.
pub const FILE_MATCH_DISTANCE: f64 = 0.35;

/// Minimum number of information units for a prompt to be considered searchable.
///
/// Prompts with fewer units are classified as trivial and skip recall.
/// An "information unit" is one whitespace-delimited word for alphabetic scripts,
/// or one character for logographic scripts (CJK ideographs, kana).
pub const MIN_QUERY_UNITS: usize = 3;

/// Prefix that identifies compaction summary messages.
///
/// User messages starting with this prefix are post-compaction summaries
/// injected by Claude Code after context window compaction.
pub const COMPACTION_SUMMARY_PREFIX: &str =
    "This session is being continued from a previous conversation";
