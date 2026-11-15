-- Generi bloccati e consentiti per utente (JSON con i nomi): un controllo parentale più fine del limite d'età.
ALTER TABLE users ADD COLUMN blocked_genres TEXT;
ALTER TABLE users ADD COLUMN allowed_genres TEXT;
