# Snapshot-first schema management

## Background

The `schema.rs` migration tests duplicate DDL strings 23 times across 5 test
functions (~80 lines of copy-pasted SQL). When production DDL changes, 1-4 test
copies must be updated in lockstep — forgetting any creates a false positive.

After evaluating diesel, sqlx, sea-orm, refinery, and other frameworks, none
provide compile-time type safety for sqlite-vector/FTS5 queries. The DDL
duplication problem can be solved with a simpler approach.

## Goals

- Eliminate all DDL duplication in production code and tests
- Use `include_str!` to reference `.sql` files directly
- Use `user_version` pragma for version tracking (no external framework)
- Fresh installs execute `schema.sql` directly; upgrades run incremental
  migration files
- Add a `mementor-schema-gen` crate to generate `schema.sql` and validate
  snapshot is up to date
- Document migration test patterns for future contributors

## Design decisions

- **No external migration framework**: `include_str!` + `user_version` is
  sufficient. Remove `rusqlite_migration` dependency.
- **Snapshot-first**: `schema.sql` is the single source of truth for the
  current schema. Fresh databases execute it directly.
- **Migration files**: individual `.sql` files for upgrades, referenced via
  `migration_ddl!` macro.
- **Cascading seed pattern**: each `seed_vN()` calls `seed_v(N-1)()` + applies
  migration N + inserts test data.
- **`mementor-schema-gen` crate**: generates `schema.sql` from migration files
  and validates the snapshot is up to date in CI.

## Results

- Removed `rusqlite_migration` dependency
- Created `crates/mementor-lib/ddl/schema.sql` — complete DDL snapshot
- Created `crates/mementor-lib/ddl/migrations/00001__initial_schema.sql`
- Rewrote `schema.rs`: `include_str!` snapshot + `user_version` pragma
- Created `crates/mementor-lib/tests/schema_snapshot.rs` with functional tests
- Created `crates/mementor-schema-gen/` crate with `migrations_match_snapshot`
  test and schema generation binary
- Added `schema:dump` mise task
- Updated `docs/testing-patterns.md` with Migration Test Patterns section
- Updated `AGENTS.md` with schema management documentation
- All 254 tests pass, clippy clean

### Code simplifications (post-commit)

- Replaced custom `OptionalExt` trait with `rusqlite::OptionalExtension`
- Extracted `format_memories()` helper to deduplicate memory-formatting code
  in `search_context` and `search_file_context`
- Fixed `upsert_session` to include `last_compact_line_index` in INSERT and
  ON CONFLICT clauses (previously silently dropped; workaround raw SQL removed)
- Simplified `embed_batch` closure to method reference

## TODO

- [x] Create worktree and history document
- [x] Remove `rusqlite_migration` dependency, create DDL files
- [x] Rewrite `schema.rs` with `include_str!` and `user_version`
- [x] Create schema functional verification tests
- [x] Create `mementor-schema-gen` crate
- [x] Update `docs/testing-patterns.md` and `AGENTS.md`
- [x] Verify: clippy and tests pass
- [x] Commit and create PR

## Future work

- When V2 migration is needed: create `00002__*.sql`, add `seed_v2()`, add
  `v1_to_v2_preserves_data` test
