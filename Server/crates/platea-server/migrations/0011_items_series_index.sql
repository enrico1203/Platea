-- Episodi per serie: conteggi dei non visti, "prossimo episodio", dettaglio della serie. Senza indice ogni
-- serie in un elenco rileggeva tutti gli episodi (1,6 s per 60 serie con 150.000 episodi).
CREATE INDEX IF NOT EXISTS items_series ON items (series_id, kind);
