-- Capitoli del file in JSON: [{"start_ms":0,"title":"…"}]. NULL = non ancora letti (file analizzati prima), "[]" = nessuno.
ALTER TABLE media_files ADD COLUMN chapters TEXT;
