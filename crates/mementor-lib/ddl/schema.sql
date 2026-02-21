CREATE TABLE chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    turn_id     INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    embedding   BLOB,
    UNIQUE(turn_id, chunk_index)
);

CREATE TABLE entries (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    line_index   INTEGER NOT NULL,
    entry_type   TEXT NOT NULL,
    content      TEXT NOT NULL DEFAULT '',
    tool_summary TEXT NOT NULL DEFAULT '',
    timestamp    TEXT,
    UNIQUE(session_id, line_index)
);

CREATE TABLE file_mentions (
    turn_id   INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    UNIQUE(turn_id, file_path, tool_name)
);

CREATE TABLE pr_links (
    session_id    TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    pr_number     INTEGER NOT NULL,
    pr_url        TEXT NOT NULL,
    pr_repository TEXT NOT NULL,
    timestamp     TEXT NOT NULL,
    UNIQUE(session_id, pr_number)
);

CREATE TABLE resource_embeddings (
    resource  TEXT PRIMARY KEY,
    embedding BLOB NOT NULL
);

CREATE TABLE session_access_patterns (
    session_id     TEXT PRIMARY KEY REFERENCES sessions(session_id) ON DELETE CASCADE,
    centroid       BLOB NOT NULL,
    resource_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE sessions (
    session_id              TEXT PRIMARY KEY,
    transcript_path         TEXT NOT NULL,
    project_dir             TEXT NOT NULL,
    started_at              TEXT,
    last_line_index         INTEGER NOT NULL DEFAULT 0,
    provisional_turn_start  INTEGER,
    last_compact_line_index INTEGER,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE subagent_sessions (
    session_id             TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    agent_id               TEXT NOT NULL,
    last_line_index        INTEGER NOT NULL DEFAULT 0,
    provisional_turn_start INTEGER,
    PRIMARY KEY (session_id, agent_id)
);

CREATE TABLE turn_access_patterns (
    turn_id        INTEGER PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE,
    centroid       BLOB NOT NULL,
    resource_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE turns (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    start_line   INTEGER NOT NULL,
    end_line     INTEGER NOT NULL,
    provisional  INTEGER NOT NULL DEFAULT 0,
    full_text    TEXT NOT NULL DEFAULT '',
    UNIQUE(session_id, start_line)
);

CREATE VIRTUAL TABLE turns_fts USING fts5(
    full_text,
    content='turns',
    content_rowid='id',
    tokenize='trigram'
);

CREATE INDEX idx_entries_session ON entries(session_id, line_index);

CREATE INDEX idx_file_mentions_path ON file_mentions(file_path);

CREATE INDEX idx_turns_session ON turns(session_id, start_line);

CREATE TRIGGER turns_fts_ad AFTER DELETE ON turns BEGIN
    INSERT INTO turns_fts(turns_fts, rowid, full_text)
    VALUES('delete', old.id, old.full_text);
END;

CREATE TRIGGER turns_fts_ai AFTER INSERT ON turns BEGIN
    INSERT INTO turns_fts(rowid, full_text) VALUES (new.id, new.full_text);
END;

CREATE TRIGGER turns_fts_au AFTER UPDATE OF full_text ON turns BEGIN
    INSERT INTO turns_fts(turns_fts, rowid, full_text)
    VALUES('delete', old.id, old.full_text);
    INSERT INTO turns_fts(rowid, full_text) VALUES (new.id, new.full_text);
END;
