-- Loghi TMDB in SVG: il download ora chiede la versione PNG. I lavori falliti per questo ripartono.
UPDATE jobs SET state = 'queued', attempts = 0, error = NULL, run_after = 0
WHERE kind = 'download_image' AND state = 'failed' AND error LIKE 'immagine non leggibile%'
  AND (dedupe_key IS NULL OR NOT EXISTS (
    SELECT 1 FROM jobs j2 WHERE j2.dedupe_key = jobs.dedupe_key AND j2.state IN ('queued', 'running')));
UPDATE images SET state = 'pending' WHERE state = 'failed' AND source = 'tmdb' AND source_url LIKE '%.svg';
