# Platea

Open source media server: server Rust (`Server/`) + web UI React (`WebApp/`), avviati da `docker-compose.yml` nella root.

- Piano: `docs/piano-di-progetto.md`. Brand: `docs/brand-identity.md`.
- **Stato dei lavori e indicazioni dell'utente: `PROGRESS.md`. Leggerlo per primo e aggiornarlo a ogni passo.**
  Le note private sull'installazione dell'utente stanno in `PROGRESS.local.md` (non versionato): mai nei file versionati,
  perché la repository è pubblica.
- Tutto si compila e si testa dentro Docker. Commit e push su `main` della repo GitHub `Platea` (pubblica).

## Regole della UI (non negoziabili)

- **iOS non deve mai fare zoom quando si tocca un campo.** Ogni `input`/`select`/`textarea` su touch e sotto i 900px ha
  testo di almeno 16px (regola globale in `WebApp/src/styles/app.css`, dettagli in `docs/brand-identity.md` › TextField).
  Un campo nuovo non la deve aggirare (niente `font-size` inline sotto 16px, niente `!important` contrari). Mai bloccare
  lo zoom con `maximum-scale`/`user-scalable=no`.

