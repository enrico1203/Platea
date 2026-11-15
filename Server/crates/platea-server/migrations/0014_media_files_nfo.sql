-- Un NFO con lo stesso nome del video (es. episodio.nfo accanto a episodio.mkv), visto durante l'elenco delle cartelle:
-- solo per questi file si legge l'NFO dell'episodio, senza aprire nulla per gli altri.
ALTER TABLE media_files ADD COLUMN has_nfo INTEGER NOT NULL DEFAULT 0;
