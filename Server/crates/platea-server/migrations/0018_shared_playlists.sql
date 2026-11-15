-- Playlist condivise: visibili in sola lettura a tutti gli utenti (ognuno vede solo le voci permesse a lui).
ALTER TABLE playlists ADD COLUMN shared INTEGER NOT NULL DEFAULT 0;
