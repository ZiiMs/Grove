
CREATE TABLE sessions (
    id             TEXT PRIMARY KEY,
    harness         TEXT NOT NULL,
    cwd            TEXT NOT NULL,
    model          TEXT,
    title          TEXT NOT NULL,
    status         TEXT NOT NULL,
    adapter_config TEXT NOT NULL DEFAULT '{}',
    resume_token   TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE TABLE events (
    seq        INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    at         TEXT NOT NULL,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL
);

CREATE INDEX events_by_session ON events(session_id, seq);

-- ---- 8< ----

CREATE TABLE plans (
    session_id  TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    markdown    TEXT NOT NULL,
    version     INTEGER NOT NULL,
    mirror_hash TEXT,
    updated_at  TEXT NOT NULL
);

-- ---- 8< ----

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- ---- 8< ----

CREATE TABLE pins (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    spec_id    TEXT NOT NULL,
    PRIMARY KEY (session_id, spec_id)
);

-- ---- 8< ----

CREATE TABLE projects (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL UNIQUE,
    path           TEXT NOT NULL UNIQUE,
    remote         TEXT,
    default_branch TEXT,
    is_git         INTEGER NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

ALTER TABLE sessions ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE SET NULL;
ALTER TABLE sessions ADD COLUMN awaiting INTEGER NOT NULL DEFAULT 0;

CREATE INDEX sessions_by_project ON sessions(project_id);
