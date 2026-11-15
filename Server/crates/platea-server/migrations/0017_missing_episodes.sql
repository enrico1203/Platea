-- Episodi già andati in onda secondo TMDB che non sono in libreria, per le stagioni presenti (speciali escluse).
CREATE TABLE missing_episodes (
    series_id INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    season    INTEGER NOT NULL,
    episode   INTEGER NOT NULL,
    title     TEXT,
    overview  TEXT,
    air_date  TEXT,
    PRIMARY KEY (series_id, season, episode)
);
