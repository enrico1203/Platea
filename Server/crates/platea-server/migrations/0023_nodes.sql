-- Nodi: altre macchine a cui il server affida transcodifiche (ruolo transcode) e da cui legge i file delle cartelle
-- che hanno in copia (ruolo cdn). La riga is_local = 1 è questo server: stessi campi di bilanciamento, non si cancella.
CREATE TABLE nodes (
    id              INTEGER PRIMARY KEY,
    name            TEXT NOT NULL,
    is_local        INTEGER NOT NULL DEFAULT 0,
    url             TEXT NOT NULL DEFAULT '',
    key             TEXT NOT NULL DEFAULT '',
    enabled         INTEGER NOT NULL DEFAULT 1,
    draining        INTEGER NOT NULL DEFAULT 0,
    role_transcode  INTEGER NOT NULL DEFAULT 1,
    role_cdn        INTEGER NOT NULL DEFAULT 0,
    tc_priority     INTEGER NOT NULL DEFAULT 1,
    tc_weight       INTEGER NOT NULL DEFAULT 100,
    tc_max_sessions INTEGER NOT NULL DEFAULT 0,
    rules           TEXT NOT NULL DEFAULT '{}',
    cdn_priority    INTEGER NOT NULL DEFAULT 1,
    cdn_weight      INTEGER NOT NULL DEFAULT 100,
    cdn_max_streams INTEGER NOT NULL DEFAULT 0,
    config          TEXT NOT NULL DEFAULT '{}',
    public_url      TEXT NOT NULL DEFAULT '',
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX nodes_one_local ON nodes (is_local) WHERE is_local = 1;
INSERT INTO nodes (name, is_local, role_transcode, role_cdn, created_at, updated_at)
VALUES ('', 1, 1, 1, CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000);

-- Cartella di una libreria (library_paths) e la sua copia su un nodo.
CREATE TABLE node_paths (
    node_id         INTEGER NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    library_path_id INTEGER NOT NULL REFERENCES library_paths (id) ON DELETE CASCADE,
    node_path       TEXT NOT NULL,
    verified_at     INTEGER,
    verify_error    TEXT,
    PRIMARY KEY (node_id, library_path_id)
);

-- Chi ha eseguito la riproduzione (transcodifica) o da quale copia è stata letta.
ALTER TABLE playback_history ADD COLUMN node TEXT;
