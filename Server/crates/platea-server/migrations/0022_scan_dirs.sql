-- Cartelle lette dall'ultima scansione completa: il totale atteso per la percentuale della scansione successiva.
ALTER TABLE libraries ADD COLUMN last_scan_dirs INTEGER;
