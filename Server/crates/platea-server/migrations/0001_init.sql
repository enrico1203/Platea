-- Platea: schema iniziale.
-- Tabelle normalizzate, file fisico separato dall'elemento logico, nessun dato su cui si filtra
-- o si ordina nascosto in una colonna JSON. Tempi in millisecondi Unix.

CREATE TABLE server_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE users (
    id              INTEGER PRIMARY KEY,
    username        TEXT NOT NULL,
    username_lower  TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    is_admin        INTEGER NOT NULL DEFAULT 0,
    disabled        INTEGER NOT NULL DEFAULT 0,
    all_libraries   INTEGER NOT NULL DEFAULT 1,
    allow_remux     INTEGER NOT NULL DEFAULT 1,
    allow_transcode INTEGER NOT NULL DEFAULT 1,
    max_bitrate     INTEGER,
    max_streams     INTEGER,
    max_rating      TEXT,
    allow_remote    INTEGER NOT NULL DEFAULT 1,
    manage_devices  INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    last_login_at   INTEGER
);

CREATE TABLE libraries (
    id                    INTEGER PRIMARY KEY,
    name                  TEXT NOT NULL,
    kind                  TEXT NOT NULL CHECK (kind IN ('movies', 'shows')),
    scan_profile          TEXT NOT NULL DEFAULT 'local' CHECK (scan_profile IN ('local', 'remote')),
    language              TEXT NOT NULL DEFAULT 'it-IT',
    country               TEXT NOT NULL DEFAULT 'IT',
    scan_interval_minutes INTEGER,
    probe_concurrency     INTEGER NOT NULL DEFAULT 2,
    probe_per_minute      INTEGER,
    probe_window_start    INTEGER,
    probe_window_end      INTEGER,
    probe_pause_on_play   INTEGER NOT NULL DEFAULT 0,
    extract_keyframes     INTEGER NOT NULL DEFAULT 1,
    created_at            INTEGER NOT NULL,
    last_scan_at          INTEGER,
    last_scan_status      TEXT,
    last_scan_message     TEXT
);

CREATE TABLE library_paths (
    id         INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL REFERENCES libraries (id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    UNIQUE (library_id, path)
);

CREATE TABLE user_library_access (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    library_id INTEGER NOT NULL REFERENCES libraries (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, library_id)
);

CREATE TABLE tokens (
    id             INTEGER PRIMARY KEY,
    token_hash     TEXT NOT NULL UNIQUE,
    user_id        INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    device_id      TEXT NOT NULL,
    device_name    TEXT NOT NULL,
    client         TEXT NOT NULL,
    client_version TEXT NOT NULL,
    created_at     INTEGER NOT NULL,
    last_used_at   INTEGER NOT NULL,
    last_ip        TEXT
);
CREATE INDEX tokens_user ON tokens (user_id);

CREATE TABLE api_keys (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    key_hash     TEXT NOT NULL UNIQUE,
    scopes       TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER
);

CREATE TABLE items (
    id                    INTEGER PRIMARY KEY,
    library_id            INTEGER NOT NULL REFERENCES libraries (id) ON DELETE CASCADE,
    kind                  TEXT NOT NULL CHECK (kind IN ('movie', 'series', 'season', 'episode', 'collection')),
    parent_id             INTEGER REFERENCES items (id) ON DELETE CASCADE,
    series_id             INTEGER REFERENCES items (id) ON DELETE CASCADE,
    title                 TEXT NOT NULL,
    sort_title            TEXT NOT NULL,
    original_title        TEXT,
    year                  INTEGER,
    index_number          INTEGER,
    index_end             INTEGER,
    parent_index_number   INTEGER,
    overview              TEXT,
    tagline               TEXT,
    runtime_ms            INTEGER,
    community_rating      REAL,
    official_rating       TEXT,
    premiere_date         TEXT,
    end_date              TEXT,
    status                TEXT,
    match_key             TEXT,
    match_state           TEXT NOT NULL DEFAULT 'pending'
                          CHECK (match_state IN ('pending', 'matched', 'review', 'unmatched', 'manual')),
    match_score           REAL,
    locked_fields         TEXT NOT NULL DEFAULT '',
    available             INTEGER NOT NULL DEFAULT 1,
    added_at              INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    metadata_refreshed_at INTEGER
);
CREATE INDEX items_browse ON items (library_id, kind, sort_title);
CREATE INDEX items_children ON items (parent_id, index_number);
CREATE INDEX items_recent ON items (library_id, added_at);
CREATE INDEX items_match_key ON items (library_id, kind, match_key);
CREATE INDEX items_match_state ON items (match_state);

CREATE TABLE media_files (
    id              INTEGER PRIMARY KEY,
    item_id         INTEGER REFERENCES items (id) ON DELETE SET NULL,
    library_path_id INTEGER NOT NULL REFERENCES library_paths (id) ON DELETE CASCADE,
    relative_path   TEXT NOT NULL,
    size            INTEGER NOT NULL,
    mtime           INTEGER NOT NULL,
    container       TEXT,
    duration_ms     INTEGER,
    bitrate         INTEGER,
    version_label   TEXT,
    state           TEXT NOT NULL DEFAULT 'present' CHECK (state IN ('present', 'missing')),
    missing_since   INTEGER,
    probe_state     TEXT NOT NULL DEFAULT 'pending' CHECK (probe_state IN ('pending', 'done', 'failed')),
    probe_error     TEXT,
    probed_at       INTEGER,
    added_at        INTEGER NOT NULL
);
CREATE UNIQUE INDEX media_files_path ON media_files (library_path_id, relative_path);
CREATE INDEX media_files_item ON media_files (item_id);
CREATE INDEX media_files_identity ON media_files (size, mtime);
CREATE INDEX media_files_probe ON media_files (probe_state);

CREATE TABLE media_streams (
    id             INTEGER PRIMARY KEY,
    file_id        INTEGER NOT NULL REFERENCES media_files (id) ON DELETE CASCADE,
    stream_index   INTEGER NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('video', 'audio', 'subtitle', 'attachment', 'data')),
    codec          TEXT NOT NULL,
    profile        TEXT,
    level          INTEGER,
    bit_depth      INTEGER,
    width          INTEGER,
    height         INTEGER,
    frame_rate     REAL,
    hdr            TEXT,
    channels       INTEGER,
    channel_layout TEXT,
    sample_rate    INTEGER,
    bitrate        INTEGER,
    language       TEXT,
    title          TEXT,
    is_default     INTEGER NOT NULL DEFAULT 0,
    is_forced      INTEGER NOT NULL DEFAULT 0,
    is_external    INTEGER NOT NULL DEFAULT 0,
    external_path  TEXT
);
CREATE INDEX media_streams_file ON media_streams (file_id, kind);

CREATE TABLE keyframes (
    file_id      INTEGER PRIMARY KEY REFERENCES media_files (id) ON DELETE CASCADE,
    times_ms     BLOB NOT NULL,
    extracted_at INTEGER NOT NULL
);

CREATE TABLE external_ids (
    item_id  INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    value    TEXT NOT NULL,
    PRIMARY KEY (item_id, provider)
);
CREATE INDEX external_ids_lookup ON external_ids (provider, value);

CREATE TABLE genres (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
CREATE TABLE item_genres (
    item_id  INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    genre_id INTEGER NOT NULL REFERENCES genres (id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, genre_id)
);
CREATE INDEX item_genres_genre ON item_genres (genre_id);

CREATE TABLE people (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    tmdb_id      INTEGER UNIQUE,
    profile_path TEXT
);
CREATE TABLE item_people (
    item_id    INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    person_id  INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
    role       TEXT NOT NULL CHECK (role IN ('actor', 'director', 'writer', 'creator')),
    character  TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (item_id, person_id, role)
);
CREATE INDEX item_people_person ON item_people (person_id);

CREATE TABLE studios (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
CREATE TABLE item_studios (
    item_id   INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    studio_id INTEGER NOT NULL REFERENCES studios (id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, studio_id)
);

CREATE TABLE images (
    id                INTEGER PRIMARY KEY,
    item_id           INTEGER REFERENCES items (id) ON DELETE CASCADE,
    person_id         INTEGER REFERENCES people (id) ON DELETE CASCADE,
    kind              TEXT NOT NULL CHECK (kind IN ('poster', 'backdrop', 'logo', 'still', 'profile', 'thumb')),
    source            TEXT NOT NULL DEFAULT 'tmdb',
    source_url        TEXT NOT NULL,
    local_path        TEXT,
    width             INTEGER,
    height            INTEGER,
    placeholder_color TEXT,
    state             TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'ready', 'failed'))
);
CREATE INDEX images_item ON images (item_id, kind);
CREATE INDEX images_person ON images (person_id);

CREATE TABLE user_item_data (
    user_id        INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    item_id        INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    position_ms    INTEGER NOT NULL DEFAULT 0,
    played         INTEGER NOT NULL DEFAULT 0,
    play_count     INTEGER NOT NULL DEFAULT 0,
    last_played_at INTEGER,
    favorite       INTEGER NOT NULL DEFAULT 0,
    audio_index    INTEGER,
    subtitle_index INTEGER,
    PRIMARY KEY (user_id, item_id)
);
CREATE INDEX user_item_data_recent ON user_item_data (user_id, last_played_at);

CREATE TABLE collections (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE collection_items (
    collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    item_id       INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, item_id)
);
CREATE TABLE playlists (
    id         INTEGER PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE playlist_items (
    id          INTEGER PRIMARY KEY,
    playlist_id INTEGER NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    item_id     INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    sort_order  INTEGER NOT NULL
);
CREATE INDEX playlist_items_order ON playlist_items (playlist_id, sort_order);

CREATE TABLE jobs (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,
    payload      TEXT NOT NULL,
    library_id   INTEGER,
    priority     INTEGER NOT NULL,
    state        TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'running', 'done', 'failed')),
    attempts     INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    run_after    INTEGER NOT NULL,
    dedupe_key   TEXT,
    created_at   INTEGER NOT NULL,
    started_at   INTEGER,
    finished_at  INTEGER,
    error        TEXT
);
CREATE INDEX jobs_next ON jobs (state, priority, run_after);
CREATE UNIQUE INDEX jobs_dedupe ON jobs (dedupe_key) WHERE state IN ('queued', 'running') AND dedupe_key IS NOT NULL;
CREATE INDEX jobs_finished ON jobs (finished_at);

CREATE TABLE settings (
    grp        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE playback_history (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER REFERENCES users (id) ON DELETE SET NULL,
    item_id     INTEGER REFERENCES items (id) ON DELETE SET NULL,
    file_id     INTEGER REFERENCES media_files (id) ON DELETE SET NULL,
    client      TEXT,
    device_name TEXT,
    method      TEXT NOT NULL,
    reason      TEXT,
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER,
    watched_ms  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX playback_history_time ON playback_history (started_at);

CREATE TABLE activity_log (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    user_id INTEGER,
    kind    TEXT NOT NULL,
    message TEXT NOT NULL,
    ip      TEXT
);
CREATE INDEX activity_log_time ON activity_log (at);

CREATE TABLE metadata_cache (
    provider   TEXT NOT NULL,
    key        TEXT NOT NULL,
    language   TEXT NOT NULL,
    response   TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (provider, key, language)
);

-- Candidati per l'elenco "da verificare" e associazioni rifiutate.
CREATE TABLE match_candidates (
    item_id     INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL,
    title       TEXT NOT NULL,
    year        INTEGER,
    score       REAL NOT NULL,
    overview    TEXT,
    poster_path TEXT,
    PRIMARY KEY (item_id, provider_id)
);
CREATE TABLE rejected_matches (
    item_id     INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL,
    PRIMARY KEY (item_id, provider_id)
);

-- Ricerca a testo pieno allineata a items.
CREATE VIRTUAL TABLE items_fts USING fts5 (
    title, original_title,
    content = 'items', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER items_fts_ai AFTER INSERT ON items BEGIN
    INSERT INTO items_fts (rowid, title, original_title) VALUES (new.id, new.title, new.original_title);
END;
CREATE TRIGGER items_fts_ad AFTER DELETE ON items BEGIN
    INSERT INTO items_fts (items_fts, rowid, title, original_title) VALUES ('delete', old.id, old.title, old.original_title);
END;
CREATE TRIGGER items_fts_au AFTER UPDATE OF title, original_title ON items BEGIN
    INSERT INTO items_fts (items_fts, rowid, title, original_title) VALUES ('delete', old.id, old.title, old.original_title);
    INSERT INTO items_fts (rowid, title, original_title) VALUES (new.id, new.title, new.original_title);
END;
