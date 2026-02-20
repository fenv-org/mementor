# Phase 1: Embedding Model Switch

Parent: [active-agent-pivot](2026-02-20_active-agent-pivot.md)

## Background

BGE-small-en-v1.5 (384d, ~33MB) is bundled via `include_bytes!` and works well
for English-only semantic search. However, mementor conversations are bilingual
(Korean + English), and BGE fails catastrophically for cross-language queries
(cosine distance 0.37-0.56, random-level similarity).

The active-agent-pivot design (PR #33) requires switching to a multilingual
model loaded from disk at runtime.

## Goal

1. Evaluate candidate embedding models (e5-base, GTE multilingual) with f32
   and int8 quantization variants
2. Replace BGE-small-en-v1.5 with the chosen model
3. Switch from bundled model (`include_bytes!`) to disk-based loading

## Part A: PoC Model Evaluation

### Candidates

| # | Model | Dims | fastembed | ONNX size |
|---|-------|------|----------|-----------|
| 1 | multilingual-e5-base f32 | 768 | Built-in `MultilingualE5Base` | ~880 MB |
| 2 | GTE multilingual base f32 | 768 | `UserDefinedEmbeddingModel` | ~1.2 GB |
| 3 | int8 variants | 768 | If quantized ONNX available | ~220-300 MB |

### Test corpus

8 realistic turn-format passages (User + Assistant + User, mixed EN/KO) and
8 short query sentences. Each query[i] maps to passage[i] as ground truth.
Passages cover: SQLite WAL + worktree DB sharing, vector search, CI/CD,
transcript parsing, Claude Code hooks, dependency injection, model bundling
vs disk loading, and DB schema migration.

### Metrics

- Embedding speed (3 runs, median for batch of 16 texts)
- Retrieval accuracy (P@1, MRR)
- Cosine distance separation (relevant vs irrelevant)
- Model load time

### Results (x86_64-apple-darwin, Intel Mac)

| Model | Dims | ONNX Size | Load | Embed 16 | P@1 | MRR | Rel.Dist | Irrel.Dist | Separation |
|-------|------|-----------|------|----------|-----|-----|----------|------------|------------|
| e5-small f32 | 384 | ~130 MB | 3.6s | 1,448ms | 100% | 100% | 0.1100 | 0.1787 | 0.0687 |
| e5-base f32 | 768 | ~880 MB | 4.5s | 3,494ms | 100% | 100% | 0.1223 | 0.1958 | 0.0735 |
| GTE-multi f32 | 768 | ~1.2 GB | 4.7s | 4,058ms | 100% | 100% | 0.2511 | 0.5070 | 0.2559 |
| GTE-multi int8 | 768 | ~340 MB | 3.0s | 3,236ms | 100% | 100% | 0.2604 | 0.4925 | 0.2321 |

#### Key observations

1. **All models achieve 100% P@1** on this 8-query benchmark.
2. **GTE has far better separation** (0.23-0.26 vs 0.07) — irrelevant passages
   are pushed much farther away, making threshold-based filtering more robust.
3. **E5 clusters everything close** — relevant and irrelevant passages are only
   0.07 apart, which makes cosine distance thresholds fragile.
4. **GTE int8 vs f32**: Negligible quality difference (0.26 vs 0.25 relevant
   distance, same 100% P@1), but int8 is ~20% faster and loads faster.
5. **Speed**: E5-small fastest (1.4s), GTE-int8 comparable to E5-base (~3.2s).

#### GTE model details

- Source: `onnx-community/gte-multilingual-base` on HuggingFace
- Loaded via `fastembed::UserDefinedEmbeddingModel` with `Pooling::Cls`
- No prefix needed for passages or queries (unlike E5's "passage: " / "query: ")
- fastembed 5.9.0 supports `UserDefinedEmbeddingModel::new()` builder
- Tokenizer: XLMRobertaTokenizer (17.1 MB tokenizer.json)
- Max sequence length: 8,192 tokens

### Decision

**GTE multilingual base int8** — best fit for mementor:
- Superior separation for robust retrieval in larger databases
- Smallest model file (~340 MB vs ~880 MB e5-base)
- No asymmetric prefix needed (simpler integration, no `EmbedMode`)
- Comparable speed to e5-base
- 768 dimensions (same as e5-base)

## Part B: Implementation

### TODO

#### PoC evaluation
- [x] Add `crates/mementor-poc-embedding/` to workspace
- [x] Implement PoC binary with realistic turn-format test corpus
- [x] Evaluate multilingual-e5-small f32 and e5-base f32
- [x] Evaluate GTE multilingual base f32 and int8
- [x] Document results and finalize model choice → GTE multilingual base int8

#### Implementation (GTE multilingual base int8)
- [x] Rewrite `Embedder` for disk-based loading via `UserDefinedEmbeddingModel`
- [x] Expose `tokenizer()` from Embedder
- [x] Add `model_cache_dir` to `MementorContext`
- [x] Remove `load_tokenizer()` from chunker
- [x] Update `EMBEDDING_DIMENSION` to 768
- [x] Add v5 DB migration (delete all data for clean start)
- [x] Update all Embedder call sites (7 production, all tests)
- [x] Add `mementor model download` CLI command
- [x] Update `main.rs` for model dir resolution (handled by MementorContext defaults)
- [x] Recalibrate distance thresholds (`FILE_MATCH_DISTANCE` 0.40 → 0.35)
- [x] Remove bundled BGE model files (`models/bge-small-en-v1.5/`)
- [x] Update scripts, mise.toml, CLAUDE.md
- [x] Remove PoC crate from workspace

#### Post-implementation simplification
- [x] Consolidate 4 duplicated `model_dir()` test helpers into `mementor-test-util`
- [x] Move `MODEL_SUBDIR` constant to `config.rs` (was duplicated in embedder + downloader)
- [x] Extract model file name constants to `config.rs` (shared by embedder + downloader)
- [x] Add `Embedder::load_tokenizer()` for lightweight tokenizer-only loading in tests
- [x] Add `model_cache_dir()` accessor test in `context.rs`
- [x] Add `MementorContext::with_model_cache_dir()` builder method
- [x] Add integration tests for `mementor model download` (cached + force paths)
