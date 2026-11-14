-- Tracce scelte dall'utente per un film o per tutta una serie (in lingue, non in indici: cambiano tra gli episodi).
CREATE TABLE track_choices (
    user_id           INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    item_id           INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    audio_language    TEXT,
    subtitle_language TEXT,
    subtitle_off      INTEGER NOT NULL DEFAULT 0,
    updated_at        INTEGER NOT NULL,
    PRIMARY KEY (user_id, item_id)
);
