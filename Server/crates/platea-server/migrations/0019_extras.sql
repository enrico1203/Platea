-- Contenuti extra (trailer locali, dietro le quinte, scene eliminate…): file senza elemento proprio (item_id NULL),
-- collegati al film o alla serie a cui appartengono. Le query sui file di un elemento li escludono da sole.
ALTER TABLE media_files ADD COLUMN extra_of INTEGER REFERENCES items (id) ON DELETE SET NULL;
ALTER TABLE media_files ADD COLUMN extra_type TEXT;
CREATE INDEX media_files_extra_of ON media_files (extra_of) WHERE extra_of IS NOT NULL;
