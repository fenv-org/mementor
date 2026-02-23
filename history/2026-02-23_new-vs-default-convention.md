# Establish `new()` vs `Default` Convention

## Background

The codebase had no established rule for `Xxxx::new()` vs `Xxxx::default()`.
`StdIO` and `BufferedIO` both had both methods with inconsistent delegation
directions. Many value-object types that could derive `Default` didn't, missing
out on `..Default::default()` ergonomics in tests.

## Goals

1. Define and document a clear convention for constructors and `Default`.
2. Extract Rust coding conventions from `AGENTS.md` into a standalone
   `docs/rust-coding-conventions.md` (matching the Deno conventions pattern).
3. Add `#[derive(Default)]` to all types where it compiles.
4. Fix delegation direction in `StdIO` and `BufferedIO`.

## Design Decisions

**Convention:**
- Add `#[derive(Default)]` whenever possible. Exceptions: fallible
  construction or zero-arg construction is impossible.
- `new()` always delegates to `Self::default()`, never the reverse.
- Manual `impl Default` only when derive won't compile.

**Scope:** 12 types modified (2 fixed, 10 new derives). 15 types correctly
have no Default (require args or are fallible).

## TODO

- [x] Create feature branch
- [x] Create history document
- [x] Create `docs/rust-coding-conventions.md`
- [x] Update `AGENTS.md` with link
- [x] Refactor `StdIO` and `BufferedIO`
- [x] Add `#[derive(Default)]` to 10 types
- [x] Verify with clippy and tests (195 tests pass, clippy clean)
- [ ] Commit
