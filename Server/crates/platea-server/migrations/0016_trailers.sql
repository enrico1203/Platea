-- Trailer (JSON: [{"key": "<id YouTube>", "name": "...", "lang": "it"}]) da TMDB o dal tag <trailer> degli NFO.
ALTER TABLE items ADD COLUMN trailers TEXT;
