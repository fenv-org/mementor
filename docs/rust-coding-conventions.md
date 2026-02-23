# Rust Coding Conventions

This document describes the Rust coding conventions for the mementor project.
Follow these rules in all Rust source files.

## Edition and style

- **Edition 2024** -- use all edition 2024 features and idioms.
- Follow standard Rust formatting (`cargo fmt`).
- Use `anyhow::Result` for fallible functions. Use `anyhow::Context` for adding
  context to errors.
- Use `tracing` for logging (`tracing::info!`, `tracing::debug!`, etc.).

## Linting

```bash
cargo clippy -- -D warnings
```

This command **must pass with zero warnings**. All clippy lints at `warn` level
for `all` and `pedantic` groups are enabled in the workspace `Cargo.toml`. The
following lints are explicitly allowed:
- `module_name_repetitions`
- `must_use_candidate`
- `missing_errors_doc`
- `missing_panics_doc`

## Constructors: `new()` vs `Default`

Add `#[derive(Default)]` to every type where zero-argument construction is
possible and infallible. The only exceptions are types whose construction can
fail (`-> Result<Self>`) or that have no meaningful zero-argument form.

### Rules

1. **Derive first.** Use `#[derive(Default)]` whenever the compiler accepts
   it. For enums, mark the default variant with `#[default]`.

2. **Manual impl as fallback.** When `#[derive(Default)]` won't compile (e.g.,
   a field type does not implement `Default`), write a manual `impl Default`.

3. **`new()` delegates to `default()`.** If a type provides a zero-argument
   `pub fn new() -> Self`, its body must be `Self::default()`. Never delegate
   in the reverse direction (`Default::default` calling `new()`).

4. **No `Default` on parameterized/fallible types.** Types that require
   arguments or return `Result` from their constructor should not implement
   `Default`. Use `new(args...)` or named constructors (`from_xxx()`,
   `with_xxx()`, `file()`, `in_memory()`).

### Examples

```rust
// Derive when all fields implement Default.
#[derive(Debug, Default, PartialEq)]
pub struct Turn {
    pub start_line: usize,
    pub end_line: usize,
    pub provisional: bool,
    pub full_text: String,
}

// Manual impl when derive won't compile.
pub struct StdIO {
    stdin: Stdin,   // Stdin does not implement Default
    stdout: Stdout,
    stderr: Stderr,
}

impl Default for StdIO {
    fn default() -> Self {
        Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
            stderr: std::io::stderr(),
        }
    }
}

impl StdIO {
    #[must_use]
    pub fn new() -> Self {
        Self::default()  // always delegate to default()
    }
}

// Enum with a default variant.
#[derive(Debug, Default, PartialEq)]
pub enum MessageRole {
    #[default]
    User,
    Assistant { tool_summary: Vec<String> },
}

// Parameterized/fallible — no Default.
impl Embedder {
    pub fn new(model_cache_dir: &Path) -> anyhow::Result<Self> { ... }
}
```

## Dependency management

Use `cargo add` to add dependencies. **Do not edit `Cargo.toml` dependency
sections directly.** This ensures proper version resolution and formatting.

Example:
```bash
cargo add -p mementor-lib anyhow
cargo add -p mementor-lib --build cc
```
