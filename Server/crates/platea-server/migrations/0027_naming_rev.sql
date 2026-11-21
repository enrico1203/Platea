-- Versione delle regole di lettura dei nomi con cui sono stati riconosciuti i file della libreria.
-- Quando le regole cambiano (scanner::NAMING_REV), la scansione successiva rilegge i file già noti
-- e sposta quelli che ora risultano un altro episodio. 0 = mai riletta.
ALTER TABLE libraries ADD COLUMN naming_rev INTEGER NOT NULL DEFAULT 0;
