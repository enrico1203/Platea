-- Miniature per la barra di avanzamento: opzione per libreria e griglie JPEG generate in cache/trickplay/<file_id>/.
ALTER TABLE libraries ADD COLUMN trickplay INTEGER NOT NULL DEFAULT 0;

CREATE TABLE trickplay (
    file_id     INTEGER PRIMARY KEY REFERENCES media_files (id) ON DELETE CASCADE,
    interval_ms INTEGER NOT NULL,
    width       INTEGER NOT NULL,
    height      INTEGER NOT NULL,
    cols        INTEGER NOT NULL,
    rows        INTEGER NOT NULL,
    count       INTEGER NOT NULL,
    sheets      INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);
