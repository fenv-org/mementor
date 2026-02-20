#![allow(
    clippy::cast_precision_loss,
    clippy::print_stdout,
    clippy::use_debug,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::similar_names
)]

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context;
use fastembed::{
    EmbeddingModel, InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles,
    UserDefinedEmbeddingModel,
};

// ---------------------------------------------------------------------------
// Test corpus: realistic turn-format passages and short queries
// ---------------------------------------------------------------------------

/// Realistic turn-format passages representing conversation chunks stored in
/// the mementor DB. Each passage follows the Turn[n] = User + Assistant + User
/// format with mixed EN/KO language, as seen in real mementor conversations.
const PASSAGES: &[&str] = &[
    // 0: SQLite WAL + worktree DB sharing
    "[User] SQLite에서 WAL 모드를 사용할 때 여러 워크트리에서 동시 읽기가 가능한가요? \
여러 개의 git worktree에서 동일한 DB 파일을 공유하려고 합니다.\n\
[Assistant] Yes, SQLite's WAL (Write-Ahead Logging) mode allows concurrent \
read access from multiple processes. Key considerations for worktree setups:\n\
1. WAL mode requires that the database file and its -wal and -shm files \
reside on the same filesystem.\n\
2. Each connection should use PRAGMA journal_mode=WAL on first open.\n\
3. Writes are still serialized — only one writer at a time, but readers \
don't block writers and writers don't block readers.\n\
For your worktree case, since all worktrees share the same .git directory \
ancestor, they can share the same SQLite database file. Use resolve_worktree() \
to find the primary worktree root and derive the DB path from there.\n\
[User] worktree에서 DB 경로를 어떻게 resolve하나요? .git 파일 타입으로 판단하는 건가요?",
    // 1: Vector embedding and similarity search in SQLite
    "[User] How do I implement vector similarity search in SQLite without \
external services? I need to embed conversation transcripts and find \
semantically similar past conversations.\n\
[Assistant] sqlite-vector 확장을 사용하면 SQLite에서 벡터 유사도 검색을 할 수 \
있습니다. Key components:\n\
1. Store embeddings as BLOB columns using vector_as_f32(json_text)\n\
2. Initialize with vector_init('memories', 'embedding', 'type=f32, dimension=384, \
distance=cosine') on each connection.\n\
3. Search using the vector_full_scan virtual table:\n\
SELECT vs.id, vs.distance FROM vector_full_scan('memories', 'embedding', \
?query_json, ?k) vs JOIN memories m ON m.rowid = vs.id\n\
The embedding model runs locally via ONNX Runtime, no external API needed. \
Each turn is chunked with MarkdownSplitter and embedded as 384-dimensional \
vectors with cosine distance for similarity.\n\
[User] 임베딩 모델을 다국어 모델로 바꾸면 dimension이 달라지나요? 마이그레이션은 어떻게 하죠?",
    // 2: CI/CD pipeline with clippy and tests
    "[User] CI 파이프라인에서 cargo clippy와 cargo test를 어떻게 설정하나요? \
PR마다 자동으로 돌리고 싶습니다.\n\
[Assistant] For the CI pipeline, configure it to run both linting and testing \
on every pull request. Here's the recommended setup:\n\
1. Run cargo clippy with -D warnings to treat all warnings as errors. \
This catches code quality issues before they reach the main branch.\n\
2. Run cargo test to execute the full test suite including both unit \
and integration tests.\n\
3. Use NO_COLOR=1 environment variable to prevent ANSI escape codes \
from interfering with output assertions in tests.\n\
The workspace Cargo.toml enables clippy::all and clippy::pedantic at warn \
level. Specific lints like module_name_repetitions and must_use_candidate \
are explicitly allowed to reduce noise.\n\
[User] clippy에서 similar_names warning이 content vs context 때문에 나오는데 어떻게 해결하나요?",
    // 3: Transcript parsing and turn-based chunking
    "[User] How does mementor parse Claude Code transcripts into turns? \
What's the turn grouping logic?\n\
[Assistant] transcript는 JSONL 형식이고 각 라인이 하나의 메시지입니다. \
Turn 그룹핑 로직은 다음과 같습니다:\n\
Turn[n] = User[n] + Assistant[n] + User[n+1]\n\
User[n]은 사용자의 프롬프트, Assistant[n]은 어시스턴트의 응답, \
User[n+1]은 다음 사용자 프롬프트로 forward context를 제공합니다.\n\
이 forward context는 어시스턴트의 응답이 실제로 무엇을 다루고 있었는지에 \
대한 힌트를 제공하여 임베딩 품질을 향상시킵니다.\n\
Turn이 모델의 토큰 제한을 초과하면 MarkdownSplitter로 sub-chunk로 분할하고, \
같은 turn 내의 인접한 sub-chunk는 약 40 토큰(~15%)의 오버랩을 공유합니다.\n\
[User] turn이 너무 길면 어떻게 chunk하나요? overlap은 왜 필요한가요?",
    // 4: Claude Code hooks and lifecycle events
    "[User] Claude Code의 PreCompact hook에서 어떤 작업을 수행하나요?\n\
[Assistant] The PreCompact hook fires right before Claude Code compacts \
the conversation history. In mementor, this hook performs several tasks:\n\
1. Reads the transcript JSONL from the path provided in stdin.\n\
2. Parses all messages since the last compaction boundary.\n\
3. Groups messages into turns using the Turn[n] = User + Assistant + User \
pattern.\n\
4. Chunks long turns using the tokenizer and MarkdownSplitter.\n\
5. Generates embeddings for each chunk using the ONNX embedding model.\n\
6. Stores the chunks and embeddings in the SQLite database.\n\
7. Records the compaction boundary line index for incremental processing.\n\
This ensures that conversation context before compaction is preserved \
in the vector database for future semantic retrieval.\n\
[User] compaction이 끝난 후에는 어떻게 되나요? SessionStart hook이 발동하나요?",
    // 5: Rust async patterns and dependency injection
    "[User] mementor에서 dependency injection을 어떻게 구현했나요? trait으로?\n\
[Assistant] Yes, mementor uses trait-based dependency injection for testability. \
The key traits are:\n\
1. MementorContext — provides project_root, cwd, is_linked_worktree, log_dir. \
Production code creates a real MementorContext from the environment.\n\
2. ConsoleIO<IN, OUT, ERR> — abstracts stdin/stdout/stderr. Tests use \
in-memory buffers instead of real I/O.\n\
The Runtime struct bundles MementorContext + DatabaseDriver. Each CLI command \
receives Runtime and ConsoleIO as parameters, making them fully testable \
without touching the filesystem or real database.\n\
Integration tests use runtime_in_memory() which creates an isolated SQLite \
database with a unique name per test.\n\
[User] 테스트에서 실제 git repo를 만들어서 테스트하나요? mock을 쓰나요?",
    // 6: Embedding model bundling vs disk loading
    "[User] Why is the BGE model bundled with include_bytes! instead of loading \
from disk? Isn't 33MB too large for a binary?\n\
[Assistant] include_bytes!로 모델을 번들링하면 배포가 간단합니다 — 하나의 바이너리만 \
배포하면 됩니다. 하지만 단점이 있습니다:\n\
1. 바이너리 크기가 ~33MB 증가합니다.\n\
2. 컴파일 시간이 길어집니다 (모델 파일을 매번 바이너리에 포함).\n\
3. 모델을 업그레이드하려면 재컴파일이 필요합니다.\n\
For a larger model like multilingual-e5-base (~880MB), bundling is impractical. \
The solution is disk-based loading: download the model to ~/.mementor/models/ \
on first run via 'mementor model download' command. fastembed's try_new() \
with InitOptions::with_cache_dir() handles auto-downloading from HuggingFace.\n\
[User] 디스크 기반으로 바꾸면 첫 실행 시 다운로드 시간은 어떻게 되나요?",
    // 7: Database schema migration strategy
    "[User] How do you handle database schema migrations in mementor? \
What happens when the schema changes between versions?\n\
[Assistant] rusqlite_migration 크레이트를 사용합니다. 마이그레이션은 순서가 있는 \
SQL 스크립트 목록으로 관리됩니다:\n\
1. v1: sessions + memories tables 생성\n\
2. v2: file_mentions table 추가\n\
3. v3: pr_links table + sessions.summary column 추가\n\
4. v4: compaction summary role 지원\n\
Each migration runs in a transaction. rusqlite_migration tracks the current \
version and only applies pending migrations. If the embedding model changes \
(e.g., 384d → 768d), a migration must clear all existing embeddings because \
vectors of different dimensions are incompatible for distance computation.\n\
[User] 임베딩 dimension이 바뀌면 기존 데이터를 전부 다시 생성해야 하나요?",
];

/// Short query sentences for retrieval evaluation.
/// Each QUERIES[i] should match PASSAGES[i] as the most relevant result.
const QUERIES: &[&str] = &[
    "워크트리에서 SQLite 데이터베이스 공유하는 방법",
    "how to do vector similarity search in sqlite",
    "CI에서 clippy warning 설정하는 방법",
    "transcript를 turn으로 파싱하는 로직",
    "PreCompact hook에서 하는 작업",
    "dependency injection 구현 방식",
    "embedding model bundling vs disk loading",
    "database schema migration 전략",
];

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    1.0 - dot / (norm_a * norm_b)
}

struct ModelEval {
    name: String,
    dimension: usize,
    load_time_ms: f64,
    embed_speed_ms: f64,
    /// distances[q][p] = cosine distance from query q to passage p
    distances: Vec<Vec<f32>>,
}

impl ModelEval {
    /// Precision@1: fraction of queries where the closest passage is correct.
    fn p_at_1(&self) -> f64 {
        let correct = self
            .distances
            .iter()
            .enumerate()
            .filter(|(q, dists)| {
                let min_idx = dists
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.total_cmp(b))
                    .map_or(usize::MAX, |(i, _)| i);
                min_idx == *q
            })
            .count();
        correct as f64 / self.distances.len() as f64
    }

    /// Mean Reciprocal Rank.
    fn mrr(&self) -> f64 {
        let sum: f64 = self
            .distances
            .iter()
            .enumerate()
            .map(|(q, dists)| {
                let mut indexed: Vec<(usize, f32)> = dists.iter().copied().enumerate().collect();
                indexed.sort_by(|(_, a), (_, b)| a.total_cmp(b));
                let rank = indexed
                    .iter()
                    .position(|(i, _)| *i == q)
                    .unwrap_or(dists.len())
                    + 1;
                1.0 / rank as f64
            })
            .sum();
        sum / self.distances.len() as f64
    }

    /// Mean distance from each query to its correct passage.
    fn mean_relevant_distance(&self) -> f32 {
        let sum: f32 = self
            .distances
            .iter()
            .enumerate()
            .map(|(q, dists)| dists[q])
            .sum();
        sum / self.distances.len() as f32
    }

    /// Mean distance from each query to non-matching passages.
    fn mean_irrelevant_distance(&self) -> f32 {
        let mut sum = 0.0_f32;
        let mut count = 0_usize;
        for (q, dists) in self.distances.iter().enumerate() {
            for (p, d) in dists.iter().enumerate() {
                if p != q {
                    sum += d;
                    count += 1;
                }
            }
        }
        sum / count as f32
    }
}

fn evaluate_model(
    name: &str,
    model: &mut TextEmbedding,
    passage_prefix: &str,
    query_prefix: &str,
    load_time_ms: f64,
) -> anyhow::Result<ModelEval> {
    let passages: Vec<String> = PASSAGES
        .iter()
        .map(|t| format!("{passage_prefix}{t}"))
        .collect();
    let queries: Vec<String> = QUERIES
        .iter()
        .map(|t| format!("{query_prefix}{t}"))
        .collect();

    // Speed benchmark: embed all passages + queries, 3 runs, take median
    let all_texts: Vec<String> = passages.iter().chain(queries.iter()).cloned().collect();
    let text_count = all_texts.len();
    let mut times = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let _ = model
            .embed(all_texts.clone(), None)
            .context("embed failed")?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let embed_speed_ms = times[1]; // median

    println!("  Embed speed: {embed_speed_ms:.0}ms for {text_count} texts (median of 3)");

    // Embed passages and queries separately for distance computation
    let passage_embs = model.embed(passages, None).context("embed passages")?;
    let query_embs = model.embed(queries, None).context("embed queries")?;

    let dimension = passage_embs[0].len();

    // Compute full distance matrix: distances[query_idx][passage_idx]
    let distances: Vec<Vec<f32>> = query_embs
        .iter()
        .map(|qe| {
            passage_embs
                .iter()
                .map(|pe| cosine_distance(qe, pe))
                .collect()
        })
        .collect();

    Ok(ModelEval {
        name: name.to_string(),
        dimension,
        load_time_ms,
        embed_speed_ms,
        distances,
    })
}

// ---------------------------------------------------------------------------
// GTE model helpers
// ---------------------------------------------------------------------------

struct GteModelPaths {
    tokenizer_json: PathBuf,
    config_json: PathBuf,
    special_tokens_map_json: PathBuf,
    tokenizer_config_json: PathBuf,
    onnx_f32: PathBuf,
    onnx_int8: PathBuf,
}

fn download_gte_files() -> anyhow::Result<GteModelPaths> {
    println!("Downloading GTE multilingual base model files from HuggingFace...");
    let api = hf_hub::api::sync::Api::new().context("create HF API")?;
    let repo = api.model("onnx-community/gte-multilingual-base".to_string());

    println!("  tokenizer files...");
    let tokenizer_json = repo.get("tokenizer.json").context("tokenizer.json")?;
    let config_json = repo.get("config.json").context("config.json")?;
    let special_tokens_map_json = repo
        .get("special_tokens_map.json")
        .context("special_tokens_map.json")?;
    let tokenizer_config_json = repo
        .get("tokenizer_config.json")
        .context("tokenizer_config.json")?;

    println!("  onnx/model.onnx (f32, ~1.2GB)...");
    let onnx_f32 = repo.get("onnx/model.onnx").context("onnx/model.onnx")?;

    println!("  onnx/model_int8.onnx (~340MB)...");
    let onnx_int8 = repo
        .get("onnx/model_int8.onnx")
        .context("onnx/model_int8.onnx")?;

    println!("  Downloads complete.\n");

    Ok(GteModelPaths {
        tokenizer_json,
        config_json,
        special_tokens_map_json,
        tokenizer_config_json,
        onnx_f32,
        onnx_int8,
    })
}

fn load_gte_model(paths: &GteModelPaths, use_int8: bool) -> anyhow::Result<TextEmbedding> {
    let onnx_path = if use_int8 {
        &paths.onnx_int8
    } else {
        &paths.onnx_f32
    };
    let onnx_bytes =
        fs::read(onnx_path).with_context(|| format!("read {}", onnx_path.display()))?;

    let tokenizer_files = TokenizerFiles {
        tokenizer_file: fs::read(&paths.tokenizer_json)?,
        config_file: fs::read(&paths.config_json)?,
        special_tokens_map_file: fs::read(&paths.special_tokens_map_json)?,
        tokenizer_config_file: fs::read(&paths.tokenizer_config_json)?,
    };

    let user_model =
        UserDefinedEmbeddingModel::new(onnx_bytes, tokenizer_files).with_pooling(Pooling::Cls);

    TextEmbedding::try_new_from_user_defined(user_model, InitOptionsUserDefined::default())
        .context("load GTE model")
}

// ---------------------------------------------------------------------------
// Results display
// ---------------------------------------------------------------------------

fn print_results(results: &[ModelEval]) {
    let sep = "=".repeat(90);
    println!("\n{sep}");
    println!("EMBEDDING MODEL EVALUATION RESULTS");
    println!("{sep}\n");

    // Summary table: speed and size
    println!("## Speed and Size\n");
    println!(
        "{:<35} {:>5} {:>12} {:>15}",
        "Model", "Dims", "Load (ms)", "Embed 16 (ms)"
    );
    println!("{}", "-".repeat(72));
    for r in results {
        println!(
            "{:<35} {:>5} {:>12.0} {:>15.0}",
            r.name, r.dimension, r.load_time_ms, r.embed_speed_ms
        );
    }

    // Retrieval accuracy
    println!("\n## Retrieval Accuracy (8 queries → 8 passages)\n");
    println!(
        "{:<35} {:>6} {:>6} {:>10} {:>12} {:>10}",
        "Model", "P@1", "MRR", "Rel.Dist", "Irrel.Dist", "Separation"
    );
    println!("{}", "-".repeat(84));
    for r in results {
        let rel = r.mean_relevant_distance();
        let irrel = r.mean_irrelevant_distance();
        println!(
            "{:<35} {:>5.0}% {:>5.1}% {:>10.4} {:>12.4} {:>10.4}",
            r.name,
            r.p_at_1() * 100.0,
            r.mrr() * 100.0,
            rel,
            irrel,
            irrel - rel,
        );
    }

    // Detailed distance matrix per model
    for r in results {
        println!("\n### {} — Distance Matrix\n", r.name);
        // Header
        print!("{:<6}", "");
        for p in 0..PASSAGES.len() {
            print!(" P{p:<6}");
        }
        println!("  Best");
        println!("{}", "-".repeat(6 + PASSAGES.len() * 8 + 6));

        for (q, dists) in r.distances.iter().enumerate() {
            print!("Q{q:<5}");
            let min_idx = dists
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                .map_or(0, |(i, _)| i);
            for (p, d) in dists.iter().enumerate() {
                if p == q {
                    print!("*{d:<6.4} ");
                } else if p == min_idx {
                    print!(">{d:<6.4} ");
                } else {
                    print!(" {d:<6.4} ");
                }
            }
            let mark = if min_idx == q { "OK" } else { "MISS" };
            println!("  {mark}");
        }
        println!("(* = correct passage, > = closest match)");
    }
}

fn resolve_cache_dir() -> PathBuf {
    std::env::var("MEMENTOR_MODEL_DIR").map_or_else(
        |_| {
            dirs::home_dir()
                .expect("Could not determine home directory")
                .join(".mementor")
                .join("models")
        },
        PathBuf::from,
    )
}

fn main() -> anyhow::Result<()> {
    let cache_dir = resolve_cache_dir();
    println!("Model cache directory: {}", cache_dir.display());
    println!("Passages: {} (turn-format, mixed EN/KO)", PASSAGES.len());
    println!("Queries:  {} (short sentences)\n", QUERIES.len());

    let mut results = Vec::new();

    // --- Model 1: E5-small f32 (built-in, 384d) ---
    println!("Loading multilingual-e5-small (384d, f32)...");
    let start = Instant::now();
    let mut e5_small = TextEmbedding::try_new(
        fastembed::InitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_cache_dir(cache_dir.clone())
            .with_show_download_progress(true),
    )
    .context("Failed to load multilingual-e5-small")?;
    let load_time = start.elapsed().as_secs_f64() * 1000.0;
    println!("  Loaded in {load_time:.0}ms");

    let eval = evaluate_model(
        "e5-small f32 (384d)",
        &mut e5_small,
        "passage: ",
        "query: ",
        load_time,
    )?;
    results.push(eval);
    drop(e5_small);

    // --- Model 2: E5-base f32 (built-in, 768d) ---
    println!("\nLoading multilingual-e5-base (768d, f32)...");
    let start = Instant::now();
    let mut e5_base = TextEmbedding::try_new(
        fastembed::InitOptions::new(EmbeddingModel::MultilingualE5Base)
            .with_cache_dir(cache_dir.clone())
            .with_show_download_progress(true),
    )
    .context("Failed to load multilingual-e5-base")?;
    let load_time = start.elapsed().as_secs_f64() * 1000.0;
    println!("  Loaded in {load_time:.0}ms");

    let eval = evaluate_model(
        "e5-base f32 (768d)",
        &mut e5_base,
        "passage: ",
        "query: ",
        load_time,
    )?;
    results.push(eval);
    drop(e5_base);

    // --- Model 3 & 4: GTE multilingual base (f32 + int8) ---
    println!("\nPreparing GTE multilingual base models...");
    match download_gte_files() {
        Ok(gte_paths) => {
            // GTE f32
            println!("Loading GTE multilingual base (768d, f32)...");
            let start = Instant::now();
            match load_gte_model(&gte_paths, false) {
                Ok(mut gte_f32) => {
                    let load_time = start.elapsed().as_secs_f64() * 1000.0;
                    println!("  Loaded in {load_time:.0}ms");

                    let eval =
                        evaluate_model("GTE-multi f32 (768d)", &mut gte_f32, "", "", load_time)?;
                    results.push(eval);
                    drop(gte_f32);
                }
                Err(e) => {
                    println!("  FAILED: {e}");
                    println!("  Skipping GTE f32.\n");
                }
            }

            // GTE int8
            println!("\nLoading GTE multilingual base (768d, int8)...");
            let start = Instant::now();
            match load_gte_model(&gte_paths, true) {
                Ok(mut gte_int8) => {
                    let load_time = start.elapsed().as_secs_f64() * 1000.0;
                    println!("  Loaded in {load_time:.0}ms");

                    let eval =
                        evaluate_model("GTE-multi int8 (768d)", &mut gte_int8, "", "", load_time)?;
                    results.push(eval);
                    drop(gte_int8);
                }
                Err(e) => {
                    println!("  FAILED: {e}");
                    println!("  Skipping GTE int8.\n");
                }
            }
        }
        Err(e) => {
            println!("  FAILED to download GTE files: {e}");
            println!("  Skipping all GTE models.\n");
        }
    }

    // --- Print results ---
    print_results(&results);

    Ok(())
}
