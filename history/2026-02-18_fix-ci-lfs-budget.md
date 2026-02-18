# Fix CI LFS Budget Exceeded

## Background

GitHub Actions CI fails at `git lfs fetch` with "This repository exceeded its
LFS budget." The only LFS-tracked file is `models/bge-small-en-v1.5/model.onnx`
(127 MB). Each CI run downloads it twice (aarch64 + x86_64), burning through
the free 1 GB/month LFS bandwidth in ~4 runs.

The model is embedded at compile time via `include_bytes!` in `embedder.rs`, so
the file must exist on disk before `cargo build`.

## Goals

1. Skip LFS fetch in CI by setting `lfs: false` in checkout
2. Download the ONNX model from Hugging Face Hub via a reusable script
3. Cache the downloaded model in CI to avoid repeated downloads
4. Register the script as a mise task (`model:download`)
5. Document usage in CLAUDE.md and README.md

## Design Decisions

- Reusable `scripts/download-model.sh` follows the same pattern as
  `scripts/update-sqlite-vector.sh`
- Script uses relative path from its own location to find the repo root
- Script skips download if model already exists (> 1MB check detects LFS
  pointer files vs actual model)
- CI uses `mise run model:download` for consistency
- `.gitattributes` LFS tracking kept for local dev clones

## TODO

- [x] Create worktree and history document
- [x] Create `scripts/download-model.sh`
- [x] Register mise task in `mise.toml`
- [x] Update `ci.yml`: disable LFS, add model cache + download
- [x] Update `CLAUDE.md` Build section
- [x] Update `README.md` with Development section
- [ ] Commit via `/commit`
- [ ] Create PR
