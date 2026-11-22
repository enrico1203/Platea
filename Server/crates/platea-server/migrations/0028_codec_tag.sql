-- Etichetta del codec video nel contenitore (`hvc1`, `hev1`, `avc1`): AVPlayer apre l'HEVC in MP4 solo come
-- `hvc1`/`dvh1`. Nell'MKV non c'è, quindi si rianalizzano solo gli HEVC negli altri contenitori.
ALTER TABLE media_streams ADD COLUMN codec_tag TEXT;

UPDATE media_files SET probe_state = 'pending', probed_at = NULL
WHERE probe_state = 'done' AND COALESCE(container, '') <> 'mkv'
  AND id IN (SELECT file_id FROM media_streams WHERE kind = 'video' AND codec = 'hevc' AND is_external = 0);

INSERT OR IGNORE INTO jobs (kind, payload, library_id, priority, state, run_after, dedupe_key, created_at)
SELECT 'probe_file', json_object('kind', 'probe_file', 'file_id', f.id), p.library_id, 40, 'queued',
       CAST(strftime('%s', 'now') AS INTEGER) * 1000, 'probe:' || f.id, CAST(strftime('%s', 'now') AS INTEGER) * 1000
FROM media_files f JOIN library_paths p ON p.id = f.library_path_id
WHERE f.probe_state = 'pending' AND f.state = 'present' AND COALESCE(f.container, '') <> 'mkv'
  AND f.id IN (SELECT file_id FROM media_streams WHERE kind = 'video' AND codec = 'hevc' AND is_external = 0);
