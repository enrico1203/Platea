-- Dischi Blu-ray copiati come cartella (BDMV): finora ogni flusso .m2ts era un "film" a sé (00021, 00050…).
-- Da ora il film è solo il flusso più grande, con il nome della cartella del disco. Si tolgono i file già importati
-- (e i loro elementi rimasti senza altri file): la prossima scansione li rilegge nel modo giusto.
DELETE FROM items
WHERE kind = 'movie'
  AND id IN (SELECT item_id FROM media_files WHERE item_id IS NOT NULL AND ('/' || relative_path) LIKE '%/BDMV/%')
  AND NOT EXISTS (SELECT 1 FROM media_files f WHERE f.item_id = items.id AND ('/' || f.relative_path) NOT LIKE '%/BDMV/%');
DELETE FROM media_files WHERE ('/' || relative_path) LIKE '%/BDMV/%';
