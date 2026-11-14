-- Protezione dalle sparizioni di massa (mount a metà): la rimozione aspetta la conferma dell'amministratore.
ALTER TABLE libraries ADD COLUMN missing_held INTEGER NOT NULL DEFAULT 0;
ALTER TABLE libraries ADD COLUMN confirm_missing INTEGER NOT NULL DEFAULT 0;
