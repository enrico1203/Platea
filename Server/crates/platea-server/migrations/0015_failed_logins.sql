-- Tentativi di accesso falliti di fila: con il blocco attivo (Impostazioni › Generali) l'account si disattiva alla soglia.
ALTER TABLE users ADD COLUMN failed_logins INTEGER NOT NULL DEFAULT 0;
