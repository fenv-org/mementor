CREATE TABLE file_mentions (
    session_id   TEXT NOT NULL REFERENCES sessions(session_id),
    line_index   INTEGER NOT NULL,
    file_path    TEXT NOT NULL,
    tool_name    TEXT NOT NULL,
    UNIQUE(session_id, line_index, file_path, tool_name)
);

CREATE TABLE memories (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id       TEXT NOT NULL REFERENCES sessions(session_id),
    line_index       INTEGER NOT NULL,
    chunk_index      INTEGER NOT NULL,
    role             TEXT NOT NULL,
    content          TEXT NOT NULL,
    embedding        BLOB,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(session_id, line_index, chunk_index)
);

CREATE TABLE pr_links (
    session_id    TEXT NOT NULL REFERENCES sessions(session_id),
    pr_number     INTEGER NOT NULL,
    pr_url        TEXT NOT NULL,
    pr_repository TEXT NOT NULL,
    timestamp     TEXT NOT NULL,
    UNIQUE(session_id, pr_number)
);

CREATE TABLE sessions (
    session_id                TEXT PRIMARY KEY,
    transcript_path           TEXT NOT NULL,
    project_dir               TEXT NOT NULL,
    last_line_index           INTEGER NOT NULL DEFAULT 0,
    provisional_turn_start    INTEGER,
    last_compact_line_index   INTEGER,
    created_at                TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_file_mentions_path ON file_mentions(file_path);

CREATE INDEX idx_memories_session ON memories(session_id, line_index);
