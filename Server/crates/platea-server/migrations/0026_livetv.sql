-- Live TV: sorgenti (Dispatcharr, M3U + XMLTV, HDHomeRun, Xtream Codes), canali, guida, preferiti, registrazioni.
-- Le tabelle esistenti non cambiano, tranne due colonne sugli utenti per i permessi.

CREATE TABLE tv_sources (
    id                 INTEGER PRIMARY KEY,
    kind               TEXT NOT NULL CHECK (kind IN ('dispatcharr', 'm3u', 'hdhomerun', 'xtream')),
    name               TEXT NOT NULL,
    -- Dispatcharr, HDHomeRun, Xtream: indirizzo del server. M3U: indirizzo (o percorso) della lista.
    url                TEXT NOT NULL,
    -- M3U: guida XMLTV; per gli altri tipi facoltativa (sostituisce quella della sorgente).
    epg_url            TEXT,
    -- Dispatcharr: chiave API; Xtream: utente e password. Come le altre chiavi del server: mai restituiti dall API.
    api_key            TEXT,
    username           TEXT,
    password           TEXT,
    -- Dispatcharr: profilo di canali e profilo di uscita da chiedere (facoltativi).
    channel_profile    TEXT,
    output_profile     INTEGER,
    -- Dispatcharr: cartella delle registrazioni vista da Platea (corrispondenza con /data/recordings di Dispatcharr).
    recordings_path    TEXT,
    max_streams        INTEGER,
    refresh_hours      INTEGER NOT NULL DEFAULT 6,
    epg_refresh_hours  INTEGER NOT NULL DEFAULT 12,
    epg_days           INTEGER NOT NULL DEFAULT 7,
    enabled            INTEGER NOT NULL DEFAULT 1,
    position           INTEGER NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,
    last_refresh_at    INTEGER,
    last_epg_at        INTEGER,
    last_status        TEXT,
    last_message       TEXT
);

CREATE TABLE tv_channels (
    id            INTEGER PRIMARY KEY,
    source_id     INTEGER NOT NULL REFERENCES tv_sources (id) ON DELETE CASCADE,
    -- Identificativo stabile nella sorgente: UUID di Dispatcharr, tvg-id, GuideNumber, stream_id Xtream.
    remote_id     TEXT NOT NULL,
    -- Chiave della guida (tvg-id / id del canale XMLTV).
    epg_id        TEXT,
    number        REAL,
    name          TEXT NOT NULL,
    group_name    TEXT,
    logo_url      TEXT,
    stream_url    TEXT NOT NULL,
    radio         INTEGER NOT NULL DEFAULT 0,
    catchup_days  INTEGER NOT NULL DEFAULT 0,
    -- Dispatcharr: id numerico del canale (per le registrazioni).
    remote_pk     INTEGER,
    hidden        INTEGER NOT NULL DEFAULT 0,
    present       INTEGER NOT NULL DEFAULT 1,
    -- Codec visti all'ultima apertura (JSON {video, audio, width, height}): per scegliere copia o conversione.
    codecs        TEXT,
    updated_at    INTEGER NOT NULL,
    UNIQUE (source_id, remote_id)
);
CREATE INDEX tv_channels_order ON tv_channels (hidden, number, name);
CREATE INDEX tv_channels_epg ON tv_channels (epg_id);

CREATE TABLE tv_programs (
    id           INTEGER PRIMARY KEY,
    channel_id   INTEGER NOT NULL REFERENCES tv_channels (id) ON DELETE CASCADE,
    start_ms     INTEGER NOT NULL,
    end_ms       INTEGER NOT NULL,
    title        TEXT NOT NULL,
    subtitle     TEXT,
    description  TEXT,
    category     TEXT,
    season       INTEGER,
    episode      INTEGER,
    image_url    TEXT,
    is_new       INTEGER NOT NULL DEFAULT 0,
    rating       TEXT,
    UNIQUE (channel_id, start_ms)
);
CREATE INDEX tv_programs_time ON tv_programs (start_ms, end_ms);

CREATE VIRTUAL TABLE tv_programs_fts USING fts5 (
    title, subtitle,
    content = 'tv_programs', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER tv_programs_fts_ai AFTER INSERT ON tv_programs BEGIN
    INSERT INTO tv_programs_fts (rowid, title, subtitle) VALUES (new.id, new.title, new.subtitle);
END;
CREATE TRIGGER tv_programs_fts_ad AFTER DELETE ON tv_programs BEGIN
    INSERT INTO tv_programs_fts (tv_programs_fts, rowid, title, subtitle) VALUES ('delete', old.id, old.title, old.subtitle);
END;
CREATE TRIGGER tv_programs_fts_au AFTER UPDATE OF title, subtitle ON tv_programs BEGIN
    INSERT INTO tv_programs_fts (tv_programs_fts, rowid, title, subtitle) VALUES ('delete', old.id, old.title, old.subtitle);
    INSERT INTO tv_programs_fts (rowid, title, subtitle) VALUES (new.id, new.title, new.subtitle);
END;

CREATE TABLE user_tv_favorites (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    channel_id INTEGER NOT NULL REFERENCES tv_channels (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, channel_id)
);

CREATE TABLE tv_recordings (
    id             INTEGER PRIMARY KEY,
    user_id        INTEGER REFERENCES users (id) ON DELETE SET NULL,
    channel_id     INTEGER REFERENCES tv_channels (id) ON DELETE SET NULL,
    source_id      INTEGER NOT NULL REFERENCES tv_sources (id) ON DELETE CASCADE,
    -- Identificativo della registrazione (o della regola di serie) in Dispatcharr.
    remote_id      TEXT,
    kind           TEXT NOT NULL DEFAULT 'once' CHECK (kind IN ('once', 'series')),
    title          TEXT NOT NULL,
    subtitle       TEXT,
    description    TEXT,
    start_ms       INTEGER NOT NULL,
    end_ms         INTEGER NOT NULL,
    status         TEXT NOT NULL DEFAULT 'scheduled',
    file_path      TEXT,
    shared         INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL
);
CREATE INDEX tv_recordings_user ON tv_recordings (user_id, start_ms);

-- Permessi: Live TV sì/no e gruppi di canali consentiti (JSON, vuoto = tutti); registrazioni.
ALTER TABLE users ADD COLUMN allow_livetv INTEGER NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN livetv_groups TEXT;
ALTER TABLE users ADD COLUMN allow_recording INTEGER NOT NULL DEFAULT 0;
