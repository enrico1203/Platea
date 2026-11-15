-- File .strm: l'indirizzo http(s) che contengono, letto durante l'analisi. FFmpeg legge da lì; direct play e download
-- rimandano lì. NULL per i file normali.
ALTER TABLE media_files ADD COLUMN stream_url TEXT;
