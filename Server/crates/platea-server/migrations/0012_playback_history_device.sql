-- Riproduzioni per utente e dispositivo (pannello dei dispositivi di un utente).
CREATE INDEX IF NOT EXISTS playback_history_device ON playback_history (user_id, device_name, client, started_at);
