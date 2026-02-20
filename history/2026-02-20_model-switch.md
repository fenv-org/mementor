# Phase 1: Embedding Model Switch

Parent: [active-agent-pivot](2026-02-20_active-agent-pivot.md)

## Background

BGE-small-en-v1.5 (384 dimensions, ~33MB) is bundled via `include_bytes!` and
works well for English-only semantic search. However, mementor conversations
are bilingual (Korean + English), and BGE fails catastrophically for
cross-language queries:

| Scenario | BGE distance | E5 distance | Improvement |
|----------|-------------|-------------|-------------|
| Same topic EN↔KO | 0.498 | 0.159 | 3.1x |
| Short query EN↔KO ("search"↔"검색") | 0.383 | 0.078 | 4.9x |
| Long EN passage ← short KO query | random (0%) | 60% correct | usable |
| Code-mixed (EN↔KO) | 0.110 | 0.083 | 1.3x |

E5 distances measured with e5-small (384d). Upgrading to e5-base (768d) adds
~5% retrieval quality and ~6% cross-lingual accuracy per MTEB benchmarks.
Centroid computation doubles from ~5ms to ~10ms -- imperceptible.

## Goal

Replace BGE-small-en-v1.5 with multilingual-e5-base. Switch from bundled model
(`include_bytes!`) to downloaded model loaded from disk.

## Design

### Model loading

Use fastembed's built-in `EmbeddingModel::MultilingualE5Base` instead of
`UserDefinedEmbeddingModel`:

```rust
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

let model = TextEmbedding::try_new(
    InitOptions::new(EmbeddingModel::MultilingualE5Base)
        .with_cache_dir(cache_dir.to_path_buf())
)?;
```

This uses memory-mapped file access (no 880MB heap allocation). If the model
is not cached, fastembed returns an error -- we catch this and direct the user
to run `mementor model download`.

### Asymmetric embedding

E5 models use asymmetric prefixes for queries vs passages. fastembed does NOT
add these automatically -- the `Embedder` wrapper must prepend them.

```rust
#[derive(Debug, Clone, Copy)]
pub enum EmbedMode {
    /// "passage: " prefix -- used during ingest
    Passage,
    /// "query: " prefix -- used during search
    Query,
}
```

### Embedder API

```rust
impl Embedder {
    /// Load model from cache_dir using fastembed built-in MultilingualE5Base.
    /// Fails if model not downloaded.
    pub fn new(cache_dir: &Path) -> anyhow::Result<Self>;

    /// Embed with E5 prefix.
    pub fn embed_batch(
        &mut self,
        texts: &[&str],
        mode: EmbedMode,
    ) -> anyhow::Result<Vec<Vec<f32>>>;

    /// Expose the loaded tokenizer for chunker use.
    pub fn tokenizer(&self) -> &Tokenizer;

    /// Embedding dimension (768 for e5-base).
    pub const fn dimension() -> usize { 768 }
}
```

### Model directory

Default: `~/.mementor/models/`

Override: `MEMENTOR_MODEL_DIR` environment variable.

Structure:
```
~/.mementor/
└── models/
    └── multilingual-e5-base/
        ├── model.onnx           (~880 MB)
        ├── tokenizer.json
        ├── config.json
        ├── special_tokens_map.json
        └── tokenizer_config.json
```

### `mementor model download` subcommand

```
mementor model download [--force]
```

- Uses the same fastembed API (`TextEmbedding::try_new`) to trigger download
- `--force` re-downloads even if cached
- Prints progress and final model path
- Exit code 0 on success, 1 on failure

### Testability

- `Embedder::new(model_dir: &Path)` takes path as parameter
- `MementorContext` gains `model_dir: PathBuf` field
- Production (`main.rs`): resolves via `MEMENTOR_MODEL_DIR` → `~/.mementor/models/`
- Integration tests: resolves via `CARGO_MANIFEST_DIR` → project-local
  `models/multilingual-e5-base/` (downloaded once, gitignored)
- CI: `mise run model:download` as setup step, model cached across runs

### Tokenizer

Currently the chunker uses a separate `load_tokenizer()` with `include_bytes!`
for the BGE tokenizer. After the switch:

- `Embedder` exposes `tokenizer()` method
- Chunker receives the tokenizer from Embedder (no separate loading)
- One model load serves both embedding and tokenization

### Distance thresholds

Current thresholds are calibrated for 384d BGE. After switching to 768d e5-base:

- `EMBEDDING_DIMENSION`: 384 → 768
- `MAX_COSINE_DISTANCE`: needs recalibration (run experiments)
- `FILE_MATCH_DISTANCE`: needs recalibration
- These will be determined experimentally during implementation

## Files to Change

| File | Change |
|------|--------|
| `embedding/embedder.rs` | Remove `include_bytes!`, load from path, add `EmbedMode`, dimension 768 |
| `pipeline/chunker.rs` | Receive tokenizer from Embedder instead of loading separately |
| `config.rs` | `EMBEDDING_DIMENSION` 384→768, update distance thresholds |
| `db/connection.rs` | Update `vector_init` dimension to 768 |
| `context.rs` | Add `model_dir: PathBuf` field to `MementorContext` |
| `cli.rs` | Add `model download` subcommand |
| `commands/` | New `model.rs` command handler |
| `main.rs` | Resolve `model_dir` from env/default |
| `scripts/download-model.sh` | Update for multilingual-e5-base |
| `mise.toml` | Update `model:download` task |
| `CLAUDE.md` | Update model references |
| `.gitignore` | Add `models/multilingual-e5-base/` |

## TODO

- [ ] Download multilingual-e5-base model files
- [ ] Implement `EmbedMode` enum
- [ ] Rewrite `Embedder::new()` for disk-based loading
- [ ] Implement `embed_batch()` with prefix injection
- [ ] Expose `tokenizer()` from Embedder
- [ ] Update chunker to use Embedder's tokenizer
- [ ] Add `model_dir` to `MementorContext`
- [ ] Implement `mementor model download` subcommand
- [ ] Update `EMBEDDING_DIMENSION` to 768
- [ ] Update `vector_init` dimension
- [ ] Calibrate distance thresholds for e5-base
- [ ] Update all call sites for new `embed_batch` signature
- [ ] Update all tests for new Embedder API
- [ ] Update download script and mise task
- [ ] Update CLAUDE.md documentation
- [ ] Remove bundled BGE model files from `models/bge-small-en-v1.5/`
