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

## PoC: Model and Quantization Evaluation

Before finalizing the model choice, Phase 1 includes a practical evaluation
comparing models and quantization levels.

### Candidate models

| Model | Dimensions | Max tokens | Prefix | fastembed built-in | ONNX size |
|-------|-----------|-----------|--------|-------------------|-----------|
| multilingual-e5-base (default) | 768 | 512 | `passage: ` / `query: ` | Yes (`MultilingualE5Base`) | ~880 MB |
| GTE multilingual base (Alibaba) | 768 (elastic 128-768) | **8,192** | None | No (`UserDefinedEmbeddingModel`) | ~1.2 GB |

**GTE advantages**:
- 8,192 token context → turns can be embedded whole without chunking
- No prefix management → simpler code (no `EmbedMode` enum)
- Elastic dimensions → 128d option with <2% accuracy loss, 6x storage savings

**GTE risks**:
- Not in fastembed's built-in model list → `UserDefinedEmbeddingModel` required
- ONNX export via [onnx-community](https://huggingface.co/onnx-community/gte-multilingual-base),
  not official Optimum → compatibility risk with fastembed's ONNX runtime version
- Larger model size (~1.2 GB vs ~880 MB)

### Quantization comparison: f32 vs int8

For the chosen model, compare f32 (default) and int8 quantized variants:

| Metric | f32 | int8 |
|--------|-----|------|
| Model size | ~880 MB (e5) / ~1.2 GB (GTE) | ~220 MB (e5) / ~300 MB (GTE) |
| Embedding speed | baseline | expected 2-4x faster |
| Accuracy | baseline | expected <2% degradation |
| Memory usage | baseline | ~4x reduction |

### Evaluation protocol

1. **Prepare test corpus**: 20 representative turns from existing mementor
   transcripts (mix of EN, KO, code-heavy, discussion-heavy)
2. **Embed with all variants**: e5-f32, e5-int8, GTE-f32, GTE-int8
3. **Measure speed**: batch embed 100 turns, record wall time (3 runs, median)
4. **Measure accuracy**: compute pairwise cosine distances for known-relevant
   and known-irrelevant pairs, calculate recall@5 and MRR
5. **Cross-language test**: EN query → KO passage, KO query → EN passage
6. **Decision criteria**:
   - If GTE loads successfully via fastembed and accuracy is comparable → prefer
     GTE for 8K context
   - If int8 accuracy loss < 3% and speed gain > 2x → use int8 for production
   - If GTE has compatibility issues → stay with e5-base

### Decision matrix

| Outcome | Model | Quantization | Dimension |
|---------|-------|-------------|-----------|
| GTE works + int8 accurate | GTE multilingual base | int8 | 768 |
| GTE works + int8 inaccurate | GTE multilingual base | f32 | 768 |
| GTE fails + int8 accurate | multilingual-e5-base | int8 | 768 |
| GTE fails + int8 inaccurate | multilingual-e5-base | f32 | 768 |

If GTE is chosen, the `EmbedMode` enum and prefix logic are removed.
If e5 is chosen, `EmbedMode` stays as designed above.

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

### PoC: Model and quantization evaluation
- [x] Download multilingual-e5-base (f32 + int8)
- [x] Download GTE multilingual base ONNX from onnx-community (f32 + int8)
- [x] Verify GTE loads via fastembed `UserDefinedEmbeddingModel`
- [x] Prepare test corpus (20 turns: EN, KO, code, discussion)
- [x] Benchmark embedding speed (e5-f32, e5-int8, GTE-f32, GTE-int8)
- [x] Measure retrieval accuracy (recall@5, MRR, cross-language)
- [x] Document results and finalize model + quantization choice → GTE int8

### Implementation (GTE multilingual base int8)
- [x] ~~Implement `EmbedMode` enum~~ (skipped: GTE needs no prefix)
- [x] Rewrite `Embedder::new()` for disk-based loading
- [x] ~~Implement `embed_batch()` with prefix injection~~ (skipped: GTE needs no prefix)
- [x] Expose `tokenizer()` from Embedder
- [x] Update chunker to use Embedder's tokenizer
- [x] Add `model_dir` to `MementorContext`
- [x] Implement `mementor model download` subcommand
- [x] Update `EMBEDDING_DIMENSION` to 768
- [x] Update `vector_init` dimension
- [x] Calibrate distance thresholds (`FILE_MATCH_DISTANCE` 0.40 → 0.35)
- [x] Update all call sites for new `embed_batch` signature
- [x] Update all tests for new Embedder API
- [x] Update download script and mise task
- [x] Update CLAUDE.md documentation
- [x] Remove bundled BGE model files from `models/bge-small-en-v1.5/`

### CI fix
- [x] Update CI workflow for new model path
  - Cache path: `models/bge-small-en-v1.5/model.onnx` → `~/.mementor/models/gte-multilingual-base`
  - Cache key: `onnx-model-bge-small-en-v1.5-v1` → `onnx-model-gte-multilingual-base-v1`
  - Move model download step after Setup ONNX Runtime (needs cargo build)
