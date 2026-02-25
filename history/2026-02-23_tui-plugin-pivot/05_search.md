# Phase 4: Search + Polish

Parent: [00_overview.md](00_overview.md)
Depends on: [04_detail-transcript-diff.md](04_detail-transcript-diff.md)

## Goal

Implement cross-transcript search overlay, file history filter, and branch
filter.

## views/search.rs — Cross-Transcript Search

```
+--[ Search ]----------------------------------------------------------+
| Query: authentication middleware_                                     |
+-----------------------------------------------------------------------+
|                                                                       |
|  [1] d5bd4941cf95 — redesign schema (2h ago)                         |
|      "...JWT authentication middleware for the API..."                |
|                                                                       |
|  [2] 7f11f9dc0ce6 — add entire integration (5h ago)                  |
|      "...authenticate with the entire.io service..."                  |
|                                                                       |
|  [3] bed382d — switch embedding model (1d ago)                        |
|      "...no authentication needed for local model..."                 |
|                                                                       |
+-----------------------------------------------------------------------+
| Enter Open match  Tab Toggle scope (all/branch)  Esc Close            |
+-----------------------------------------------------------------------+
```

### Search Implementation

No vector DB — search is done by:
1. Load transcript for each checkpoint (lazy, cached)
2. Case-insensitive substring search across all text entries
3. Rank by recency (most recent first) or by match density
4. Display matching line with surrounding context

For large histories, search runs in a background thread to keep the UI
responsive. Results stream in as checkpoints are scanned.

### Key Bindings

| Key | Action |
|-----|--------|
| `Enter` | Open matching checkpoint |
| `Tab` | Toggle scope: all branches / current branch |
| `j` / `k` | Navigate results |
| `Esc` | Close search |

## TODO

- [ ] Implement `views/search.rs` — search overlay UI
- [ ] Implement background search thread with streaming results
- [ ] Implement result ranking (recency + match density)
- [ ] Implement scope toggle (all / current branch)
- [ ] Implement search from checkpoint list (`/` key)
- [ ] Implement file history filter ("which sessions touched this file?")
- [ ] Polish: loading indicators, empty states, error messages
