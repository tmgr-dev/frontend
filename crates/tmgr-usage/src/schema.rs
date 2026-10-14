pub const MIGRATIONS: &[&str] = &[
    "
    CREATE TABLE usage_source (
        id INTEGER PRIMARY KEY,
        tool TEXT NOT NULL,
        account TEXT NOT NULL,
        last_scan_at INTEGER,
        parser_version INTEGER NOT NULL DEFAULT 0,
        UNIQUE (tool, account)
    );
    CREATE TABLE usage_file_state (
        source_id INTEGER NOT NULL REFERENCES usage_source(id) ON DELETE CASCADE,
        file_key TEXT NOT NULL,
        dev INTEGER NOT NULL DEFAULT 0,
        inode INTEGER NOT NULL DEFAULT 0,
        size INTEGER NOT NULL,
        mtime_ns INTEGER NOT NULL,
        byte_offset INTEGER NOT NULL,
        head_len INTEGER NOT NULL DEFAULT 0,
        head_tag TEXT NOT NULL DEFAULT '',
        parser_state TEXT,
        last_event_ts INTEGER,
        PRIMARY KEY (source_id, file_key)
    ) WITHOUT ROWID;
    CREATE TABLE usage_session (
        id INTEGER PRIMARY KEY,
        tool TEXT NOT NULL,
        account TEXT NOT NULL,
        tool_session_id TEXT NOT NULL,
        started_at INTEGER,
        ended_at INTEGER,
        git_branch TEXT,
        cwd TEXT,
        project_name TEXT,
        project_hash TEXT,
        parent_session_id TEXT,
        UNIQUE (tool, account, tool_session_id)
    );
    CREATE TABLE usage_event (
        id INTEGER PRIMARY KEY,
        session_id INTEGER NOT NULL REFERENCES usage_session(id),
        event_key TEXT NOT NULL UNIQUE,
        tool TEXT NOT NULL,
        account TEXT NOT NULL,
        ts INTEGER NOT NULL,
        model TEXT NOT NULL,
        input INTEGER NOT NULL,
        output INTEGER NOT NULL,
        cache_read INTEGER NOT NULL,
        cache_write_5m INTEGER NOT NULL,
        cache_write_1h INTEGER NOT NULL,
        reasoning INTEGER NOT NULL DEFAULT 0,
        speed TEXT,
        service_tier TEXT,
        cost_usd REAL,
        cost_basis TEXT NOT NULL,
        git_branch TEXT,
        cwd TEXT,
        project_name TEXT,
        project_hash TEXT,
        is_sidechain INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX usage_event_ts ON usage_event(ts);
    CREATE INDEX usage_event_session ON usage_event(session_id);
    CREATE TABLE usage_day_rollup (
        day TEXT NOT NULL,
        tool TEXT NOT NULL,
        model TEXT NOT NULL,
        account TEXT NOT NULL,
        project_hash TEXT NOT NULL,
        project_name TEXT NOT NULL,
        input INTEGER NOT NULL,
        output INTEGER NOT NULL,
        cache_read INTEGER NOT NULL,
        cache_write_5m INTEGER NOT NULL,
        cache_write_1h INTEGER NOT NULL,
        reasoning INTEGER NOT NULL,
        cost_usd REAL NOT NULL,
        events INTEGER NOT NULL,
        unpriced_events INTEGER NOT NULL,
        PRIMARY KEY (day, tool, model, account, project_hash)
    ) WITHOUT ROWID;
    CREATE TABLE price_book (
        model_key TEXT PRIMARY KEY,
        entry TEXT NOT NULL,
        source TEXT NOT NULL,
        fetched_at INTEGER NOT NULL
    ) WITHOUT ROWID;
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
    ",
    "
    CREATE TABLE usage_limit_snapshot (
        id INTEGER PRIMARY KEY,
        tool TEXT NOT NULL,
        account TEXT NOT NULL,
        tool_session_id TEXT NOT NULL DEFAULT '',
        window TEXT NOT NULL,
        used_pct REAL NOT NULL,
        resets_at INTEGER,
        plan TEXT,
        observed_at INTEGER NOT NULL,
        source TEXT NOT NULL,
        UNIQUE (tool, account, tool_session_id, window)
    );
    CREATE INDEX usage_limit_observed ON usage_limit_snapshot(observed_at);
    ",
];
