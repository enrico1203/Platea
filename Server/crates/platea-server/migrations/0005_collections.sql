-- Collezioni come elementi della libreria (kind = 'collection'): ricerca, immagini e navigazione già funzionano.
-- Le vecchie tabelle non erano mai state usate.
DROP TABLE collection_items;
DROP TABLE collections;
CREATE TABLE collection_members (
    collection_id INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    item_id       INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, item_id)
);
CREATE INDEX collection_members_item ON collection_members (item_id);
