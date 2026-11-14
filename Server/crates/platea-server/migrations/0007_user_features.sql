-- Permessi e personalizzazione degli utenti.
ALTER TABLE users ADD COLUMN allow_download INTEGER NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN block_unrated INTEGER NOT NULL DEFAULT 0;
-- Fasce orarie di accesso in JSON: [{"days":[1,2,3,4,5],"start":960,"end":1320}] (minuti dalla mezzanotte, ora locale).
-- NULL = sempre.
ALTER TABLE users ADD COLUMN access_schedule TEXT;
-- Preferenze scelte dall'utente (lingue, sottotitoli, home…), JSON validato dal server.
ALTER TABLE users ADD COLUMN preferences TEXT NOT NULL DEFAULT '{}';
-- Versione dell'immagine del profilo (ms), NULL = nessuna.
ALTER TABLE users ADD COLUMN avatar_version INTEGER;
