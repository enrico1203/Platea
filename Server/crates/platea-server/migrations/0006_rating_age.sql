-- Età minima della classificazione, calcolata dal server (ratings.rs); -1 = sigla sconosciuta.
ALTER TABLE items ADD COLUMN rating_age INTEGER;
CREATE TRIGGER items_rating_age_reset AFTER UPDATE OF official_rating ON items
WHEN new.official_rating IS NOT old.official_rating BEGIN
    UPDATE items SET rating_age = NULL WHERE id = new.id;
END;
