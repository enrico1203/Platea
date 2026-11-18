-- Audiolibri: tipo di libreria 'audiobooks', elementi 'audiobook', autori e lettori tra le persone.
-- I vincoli CHECK si allargano riscrivendo il testo dello schema (procedura documentata da SQLite per le modifiche
-- che non toccano i dati su disco): nessuna copia delle tabelle, che su un database grande richiederebbe minuti.
-- Il server riapre le connessioni dopo le migrazioni, così tutte leggono lo schema nuovo.
PRAGMA writable_schema = ON;
UPDATE sqlite_master
   SET sql = replace(sql, 'CHECK (kind IN (''movies'', ''shows''))', 'CHECK (kind IN (''movies'', ''shows'', ''audiobooks''))')
 WHERE type = 'table' AND name = 'libraries';
UPDATE sqlite_master
   SET sql = replace(sql, 'CHECK (kind IN (''movie'', ''series'', ''season'', ''episode'', ''collection''))',
                          'CHECK (kind IN (''movie'', ''series'', ''season'', ''episode'', ''collection'', ''audiobook''))')
 WHERE type = 'table' AND name = 'items';
UPDATE sqlite_master
   SET sql = replace(sql, 'CHECK (role IN (''actor'', ''director'', ''writer'', ''creator''))',
                          'CHECK (role IN (''actor'', ''director'', ''writer'', ''creator'', ''author'', ''narrator''))')
 WHERE type = 'table' AND name = 'item_people';
PRAGMA writable_schema = OFF;

-- Tag del file (titolo, album, artista, compositore, genere, descrizione…) letti dall'analisi dei file audio:
-- JSON {chiave in minuscolo: valore}. NULL per i video e per i file analizzati prima.
ALTER TABLE media_files ADD COLUMN tags TEXT;
