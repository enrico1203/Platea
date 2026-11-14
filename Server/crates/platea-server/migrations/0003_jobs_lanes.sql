-- Prelievo dei lavori per corsia e per libreria: una coda lunga di un tipo non deve affamare le altre.
CREATE INDEX jobs_lane_next ON jobs (state, kind, library_id, priority, run_after);
