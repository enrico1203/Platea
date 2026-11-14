# Platea — Stato dei lavori

Questo file è la memoria del progetto tra una sessione e l'altra. Va aggiornato a ogni passo concluso.
Se la sessione si interrompe, alla ripresa ("continua") si riparte da qui: leggere **Dove sono arrivato** e **Prossimi passi**.

## Indicazioni di lavoro (dall'utente)

- Leggere `docs/piano-di-progetto.md` e `docs/brand-identity.md` e sviluppare tutto, senza fermarsi.
- Usare sempre container Docker (niente toolchain installate sull'host: Rust, Node, FFmpeg girano nei container).
- Un `docker-compose.yml` nella root che avvia sia `Server/` sia `WebApp/`.
- Repository GitHub privata `Platea`; fare sempre commit e push su `main`.
- Testare da soli le cose (build, test unitari, prove end-to-end con curl/browser nei container).
- Tenere aggiornato il README.
- Scaricare da soli FFmpeg (jellyfin-ffmpeg) e le altre dipendenze.
- Fare domande solo se indispensabile, cercando di non fermarsi mai.
- Scrivere sempre in questo file cosa è stato fatto e dove si è arrivati.
- Nel README una sezione "App compatibili": Infuse (Apple) OK, WebApp OK, TODO Android nativa, TODO iOS nativa.
- Immagini Docker leggere: l'obiettivo è l'efficienza estrema. Il server non deve bloccarsi mai
  (nessun I/O bloccante sul runtime async, timeout ovunque, semafori su ogni risorsa scarsa).
  Scelte: runtime server `debian:trixie-slim` + `jellyfin-ffmpeg8` senza pacchetti consigliati
  (serve glibc per i driver GPU); web UI su `nginx:alpine` con soli file statici.
- WebUI multilingua: per ora italiano e inglese, con un sistema di traduzione scalabile
  (`WebApp/src/i18n/`: un file per lingua, chiavi tipizzate sull'italiano, `Intl` per numeri e date;
  aggiungere una lingua = aggiungere un file e registrarlo in `locales.ts`). Aggiunto al piano.

## Note sull'ambiente

- Funzione richiesta: scadenza dell'account utente. Dopo la data l'utente resta attivo e vede tutto,
  ma non può riprodurre né scaricare. (Fatta: vedi sotto.)
- Host condiviso con altri progetti Docker: le porte 8096 (server) e 8080 (web) erano libere all'inizio.
- Nessun cargo/node sull'host: build solo via Docker.

## Dove sono arrivato

- [x] Letti i documenti, spostati in `docs/`
- [x] Repository GitHub privata creata: github.com/enrico1203/Platea
- [x] `Server/` workspace Rust: crate `platea-naming` (nomi), `platea-playback` (decisione), `platea-server` (eseguibile)
- [x] Server compila; 39 test unitari verdi (`scripts/cargo.sh test`)
- Moduli scritti nel server: boot, db (WAL, scrittore unico + pool lettura, migrazioni con backup), settings (schema JSON),
  auth (Argon2id, token hash, cache, limite login), jobs (coda SQLite, priorità, semafori per libreria),
  scanner (3 fasi, zero file aperti, mount assente, spostamenti, sidecar), probe (ffprobe, keyframe),
  metadata (TMDB, punteggio, da verificare, immagini), transcode (comandi FFmpeg, HW detect, HLS con riavvio/throttle),
  sessions, stream (Range), API REST completa + OpenAPI generato dalla tabella delle route + WebSocket eventi
- [x] Dockerfile Server (rust:1-trixie → debian:trixie-slim + jellyfin-ffmpeg8 8.1.3), healthcheck senza curl
- [x] Test end-to-end manuale (2026-11-14): setup con codice, librerie, scansione (5 ms), analisi, direct play 206,
  remux MKV+AC3 → HLS con AAC, transcodifica HEVC 10 bit → H.264 con riavvio sul salto, timestamp segmenti allineati,
  WebVTT da .srt esterno, stato di visione e "prossimi episodi", ricerca FTS5, WebSocket 101. RAM server ~60 MB.
- [x] WebApp React + Vite + TS (`WebApp/`): i18n it/en scalabile, brand completo (token, componenti pl-*, marchio SVG 16:9,
  icone PWA), procedura guidata, home, librerie, dettaglio, ricerca, player (hls.js/nativo, doppio tocco, tracce, qualità,
  ripresa, prossimo episodio, ripiego automatico), account, informazioni (attribuzione TMDB), dashboard admin in pacchetti
  separati (panoramica, librerie, utenti, lavori, da verificare, transcodifica, impostazioni da schema, attività, sistema)
- [x] Dockerfile WebApp (node:22-alpine → nginx:1-alpine, proxy /api senza buffering)
- [x] docker-compose.yml root (server 8096, web 8080), `.env.example`, README con "App compatibili"
- [x] Test nel browser (Playwright + Google Chrome in Docker, script in scratchpad non versionato): procedura guidata,
  direct play, remux, transcodifica con salto: tutti riproducono; zero errori in console; telefono e inglese verificati.
  Nota: il Chromium di Playwright non ha H.264/AAC, serve `channel: "chrome"`.
- [x] Strato compatibile Jellyfin per Infuse (`api/jellyfin.rs`): System/Info, AuthenticateByName (header X-Emby-Authorization),
  Views, Items (filtri, ordinamento, paginazione), Shows/Seasons/Episodes/NextUp, Resume, Latest, Search/Hints, immagini,
  PlaybackInfo, /Videos/{id}/stream con Range, sottotitoli esterni, Sessions/Playing|Progress|Stopped, Played/Favorite.
  ID GUID a 32 cifre con prefisso di tipo; percorsi senza distinzione maiuscole/minuscole e prefisso /emby.
  Verificato con richieste simulate (curl) come le fa Infuse. NON ancora provato con l'app Infuse reale.

- [x] Metadati verificati contro un TMDB finto (`scripts/tmdb-mock/mock.py`, server con `PLATEA_TMDB_BASE` e
  `PLATEA_TMDB_IMAGE_BASE`): associazione automatica, "da verificare" con candidati e punteggi, Identifica manuale,
  stagioni ed episodi, cast, generi, studi, classificazione, immagini con colore segnaposto, foto attori su richiesta.
  Non c'è una chiave TMDB vera in questo ambiente: va provata dall'utente.
- [x] CI GitHub Actions (`.github/workflows/ci.yml`): fmt, clippy -D warnings, test, typecheck e build web, build immagini.

- [x] Scadenza account (migrazione 0002 `users.expires_at`): `require_playback()` su playback/info, stream diretto,
  HLS, sottotitoli, strato Infuse (Policy.EnableMediaPlayback=false, 403); sessioni in corso chiuse alla scadenza;
  campo data nel dialogo utente, badge "Scaduto"/"scade il", avviso all'utente. Gli amministratori non scadono.
  Testato end-to-end (scaduto → bloccato, rinnovo → riproduce subito).
- [x] Corretto: `null` nelle PATCH non azzerava i campi opzionali (serde Option<Option>): `util::double_option`.
- [x] Log in tempo reale (`logs.rs`, pagina Amministrazione → Log, righe via WebSocket `log.line`).
- [x] Chiavi API con ambito `scan` (header `X-Platea-Key`) per Sonarr/Radarr/script; gestione in Sistema.

## Benchmark (2026-11-14, 8 CPU, disco locale, libreria sintetica da 100.000 file vuoti)

| Misura | Obiettivo del piano | Misurato |
| --- | --- | --- |
| Prima scansione 40.000 film + 60.000 episodi (inserimento e identificazione) | — | 22 s |
| Scansione senza modifiche di 100.000 file | < 60 s | 0,9–1,5 s, zero file aperti (solo read_dir + stat) |
| Nomi non riconosciuti | — | 0 su 100.000 |
| Pagina di libreria da 60 elementi | < 50 ms | 20 ms |
| Ricerca FTS5 | — | 19 ms |
| Home | — | 4 ms |
| Avvio del server con 100.000 elementi | < 2 s | 38 ms |
| Memoria a riposo con 100.000 elementi | < 100 MB | 44 MB all'avvio, 66 MB dopo una scansione |
| Memoria con libreria piccola | — | 24 MB |

Ottimizzazioni che hanno portato la memoria da 315 MB a 66 MB: mimalloc, cache SQLite 8 MB per connessione,
lettori inattivi chiusi dopo 60 s, mmap 64 MB, accodamento dei lavori a lotti.
Script del benchmark: `scripts/bench_library.py` (genera la libreria sintetica).

## Prossimi passi

1. Provare Infuse reale contro il server (serve un dispositivo Apple: chiedere all'utente) e correggere le differenze.
2. Fase 3: verificare QSV/VA-API/NVENC su hardware reale (qui non c'è GPU: solo software testato).
3. Fase 4: log in tempo reale, chiavi API con ambito, notify per dischi locali.
4. Fase 5: test di carico, libreria sintetica da 100.000 file, CI GitHub Actions.

## Debito tecnico noto (da fare dopo)

- Segmenti HLS in MPEG-TS (non fMP4): HEVC in remux solo se il client lo dichiara in `hls_video_codecs`.
- Keyframe estratti con ffprobe sui pacchetti (legge tutto il file): da sostituire con lettura dei Cues MKV.
- `max_rating` per utente salvato ma non ancora applicato ai filtri.
- Chiavi API con ambito limitato: tabella presente, endpoint da fare.
- Log in tempo reale in dashboard da fare.

## Registro

- 2026-11-14 — Avvio del progetto. Server, WebApp, compose, README. Fasi 0 e 1 complete, fase 2 quasi (manca Infuse).
