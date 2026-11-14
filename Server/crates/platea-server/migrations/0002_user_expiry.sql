-- Scadenza dell'account: dopo questa data l'utente sfoglia la libreria ma non riproduce né scarica.
ALTER TABLE users ADD COLUMN expires_at INTEGER;
