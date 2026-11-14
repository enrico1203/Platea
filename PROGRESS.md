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

### Prova sulla libreria reale dell'utente (2026-11-14)

Mount rclone (Jottacloud crypt) dell'utente su `/mnt/jotta2`: 8.517 cartelle, 8.495 MKV quasi tutti con `.nfo` Kodi.
Montato SOLO in lettura nel container (`docker-compose.override.yml` locale, non versionato:
`/mnt/jotta2:/media/jotta2:ro,rslave` + `extra_hosts host.docker.internal:host-gateway` per l'rc di rclone su :6470).
**Regola dell'utente: mai scrivere nelle cartelle dei media; copertine e metadati solo nel DB o in cache locale.**
Libreria "Film cloud" (profilo Remoto) creata nel compose locale.

- [x] Scansione in parallelo (`list_tree` a livelli con N thread, 8 per le remote; niente stat su `.plateaignore`, niente stat
  sui file non video): 8.502 file in 8.520 cartelle in 39 s, 0 non riconosciuti (il vecchio elenco sequenziale non finiva in 10 min:
  la cache cartelle di rclone scade dopo 5 min e ogni cartella è una richiesta al provider).
- [x] rclone rc: `rclone_mount_path` traduce la cartella della libreria nel percorso del mount; `vfs/refresh` ricorsivo prima della scansione.
- [x] Modulo NFO (`metadata/nfo.rs`, roxmltree): film `<nome>.nfo`/`movie.nfo`, `tvshow.nfo`; NFO con solo URL; ID tmdb/imdb/tvdb;
  senza chiave TMDB applica titolo, originale, trama, voto, classificazione, generi, studi, cast con foto, regia, locandina, sfondo.
  Risultato: 8.194 film su 8.496 associati senza TMDB, 8.190 locandine in cache. Impostazione `metadata.read_nfo`.
- [x] Modulo `.plexmatch` (`metadata/plexmatch.rs`): title, year, tmdbid, imdbid, tvdbid, guid; vince sull'NFO; un NFO in
  contrasto non viene applicato. Righe `ep:` non ancora usate. Impostazione `metadata.read_plexmatch`. Provato end-to-end.
- [x] Keyframe dai Cues MKV (`mkv.rs`, parser EBML minimo): poche letture invece del file intero; estratti già durante l'analisi.
  Verificati identici a ffprobe su file reali. Le librerie remote non leggono mai tutto il file (senza Cues → transcodifica).
- [x] CORRETTO BUG GRAVE: il muxer segment misura `segment_times` dal primo pacchetto, non in tempo assoluto. Con `-copyts`
  dopo un salto i tagli non scattavano mai (un unico segmento con tutto il resto del film), sia in remux sia in transcodifica;
  anche dall'inizio i tagli potevano slittare di un keyframe. Ora: tempi relativi all'inizio + `segment_time_delta` (copia: fino a
  300 ms, mai oltre metà distanza tra keyframe; transcodifica 100 ms); nella copia `-ss` punta 200 ms dopo il keyframe perché
  FFmpeg arretra la ricerca di ~130 ms con i B-frame. Verificato: salto al segmento 300 dal cloud in 0,6 s, confini esatti.
- [x] Coda lavori: il prelievo era "i primi 64 per priorità" e migliaia di associazioni affamavano immagini e analisi.
  Ora per corsia e, per le analisi, per libreria (migrazione 0003, indice `jobs_lane_next`).
- [x] Protezione del mount (richiesta dell'utente): radice vuota/illeggibile → stop (già c'era); cartelle illeggibili → i loro file
  non contano come spariti; sparizione di massa (≥ 20% e ≥ 10 file, configurabile) → nulla viene rimosso, stato `missing_held`,
  avviso in panoramica, pulsante "Li ho cancellati io" (POST `/libraries/{id}/confirm-missing`, vale per una scansione).
  Cancellazioni normali: file segnati mancanti, visibili come non disponibili per il periodo di grazia, poi rimossi.
  Migrazione 0004. Provato end-to-end su libreria di prova locale (non sul mount dell'utente).
- [x] Avvisi della panoramica tradotti per codice; con NFO attivi la chiave TMDB mancante è solo informativa.

### Collezioni, limite per età, ricerca (2026-11-14)

- [x] Collezioni automatiche (migrazione 0005, `metadata/collections.rs`): elementi `kind = 'collection'` + `collection_members`;
  da `<set>` degli NFO e `belongs_to_collection` di TMDB (con le sue immagini; senza, la locandina del primo film).
  Elenco e ricerca mostrano solo collezioni con almeno 2 film. Pagina collezione in ordine di uscita, "Fa parte di" sul film,
  filtro "Collezioni" nella libreria. Sulla libreria cloud: 896 collezioni (James Bond 10, Fast & Furious 10, Saw 10…).
- [x] POST `/libraries/{id}/refresh-metadata` + pulsante "Aggiorna metadati"; senza chiave TMDB il rinnovo rilegge NFO/.plexmatch.
  8.496 elementi riletti dal cloud in ~13 minuti.
- [x] Limite di classificazione per utente applicato (migrazione 0006 `items.rating_age`, `ratings.rs`): sigle di ogni paese
  convertite in età (T, VM14, PG-13, TV-MA, FSK 16, 14+, Kids+13…), trigger che la azzera al cambio, ricalcolo ogni minuto.
  Filtro in `library_filter` (liste, ricerca, home, figli, strato Infuse) e `can_see_item` (dettaglio, riproduzione, stream).
  Episodi e stagioni usano la classificazione della serie; i titoli senza classificazione restano visibili.
  Provato: utente con limite 6 anni vede 7.996 film su 8.496, VM18 → 404 su dettaglio e riproduzione, Infuse coerente.
- [x] Ricerca: `?q=` nell'indirizzo (condivisibile, "indietro" ritrova i risultati).
- [x] Notifiche sui dischi locali (`watch.rs`, crate notify/inotify): eventi in coda limitata, quiete di 10 s, scansione della
  sola cartella di primo livello (voce nella radice → libreria intera, così una cartella cancellata viene tolta); coda piena →
  scansione completa; watcher ricostruito quando cambiano le librerie. Solo profilo Locale. Provato: film aggiunto visibile
  dopo 10 s (scansione 6 ms), file tolto segnato mancante. Impostazioni `scanner.watch_local`, `watch_debounce_seconds`.
- [x] Sparizione di massa: la percentuale ora si misura sull'intera libreria anche nelle scansioni mirate.
- [x] Infuse: collezioni come BoxSet (IncludeItemTypes=BoxSet, ParentId=collezione → film, ChildCount, locandina del primo film).
- [x] CORRETTO: nello strato Infuse `SortBy=PremiereDate` dava errore SQL (l'ordinamento veniva spezzato sulle virgole).

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

0. Libreria cloud: 302 film senza NFO restano "in attesa" finché non c'è una chiave TMDB (chiederla all'utente).
1. Provare Infuse reale contro il server (serve un dispositivo Apple: chiedere all'utente) e correggere le differenze.
2. Fase 3: verificare QSV/VA-API/NVENC su hardware reale (qui non c'è GPU: solo software testato).
3. Fase 4: fatto (log in tempo reale, chiavi API, notify per dischi locali).
4. Fase 5: test di carico, libreria sintetica da 100.000 file, CI GitHub Actions.

## Debito tecnico noto (da fare dopo)

- Segmenti HLS in MPEG-TS (non fMP4): HEVC in remux solo se il client lo dichiara in `hls_video_codecs`.
- Keyframe per MP4 remoti: leggere `stss` dal box moov (ora su remoto senza Cues si transcodifica).
- `.plexmatch`: righe `ep:` per episodi con nomi non standard. NFO degli episodi (`<episodio>.nfo`) non letti.
- Chiavi API con ambito limitato: tabella presente, endpoint da fare.
- Log in tempo reale in dashboard da fare.

## Registro

- 2026-11-14 — Notifiche sui dischi locali, collezioni in Infuse (BoxSet), corretto l'ordinamento per data in Infuse.
- 2026-11-14 — Collezioni automatiche, limite per età applicato ovunque, rinnovo metadati per libreria, ricerca nell'indirizzo.
- 2026-11-14 — Prova sulla libreria cloud reale: NFO, .plexmatch, Cues MKV, scansione parallela, rclone rc, protezione del mount,
  corretto il taglio dei segmenti dopo un salto, coda lavori per corsia.
- 2026-11-14 — Avvio del progetto. Server, WebApp, compose, README. Fasi 0 e 1 complete, fase 2 quasi (manca Infuse).
