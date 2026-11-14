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
- 14/11/2026: l'installazione locale è stata azzerata su richiesta dell'utente (data, cache, transcode):
  l'onboarding lo fa l'utente. Niente account o librerie di prova sul database dell'utente.
  Cartelle dei media dell'utente in `docker-compose.override.yml` (NON versionato): `/mnt/{jotta2,ortiad,john,
  audiolibri,film,serie}` → `/media/<nome>:ro,rslave` (mount rclone dell'host). rc di rclone: jotta2 :6470,
  film :8573, serie :8574, john :8575, ortiad :8578, audiolibri :8579 — in Impostazioni → Scansione → "Mount rclone"
  (`/media/<nome>=http://host.docker.internal:<porta>`), uno per mount; ogni cartella di libreria usa il suo.
  Tunnel Cloudflare attivo (profilo `tunnel`, token in `.env`). Password dimenticata:
  `docker compose exec server platea reset-password <utente>`.
- 15/11/2026 notte: Infuse completo (serie comprese: Fields in tutti gli elenchi), AirPlay e Google Cast nel player,
  pausa dei lavori, dispositivi per utente, soggetto leggibile dei lavori, manutenzione programmata del DB (PRAGMA
  optimize ogni ora, ANALYZE/WAL/VACUUM la notte), indice items(series_id), eventi raggruppati (client e server).
  Sicurezza: job Trivy in CI (fallisce su HIGH/CRITICAL correggibili), container NON root (server uid 1000:
  data/cache/transcode dell'host sono `chown 1000:1000`; nginx utente nginx). Restano HIGH non corrette da Debian
  (util-linux, libxml2, expat…): elencate in CI, non bloccanti.
  Prima di ogni commit: `scripts/cargo.sh fmt --all` e `scripts/cargo.sh clippy --all-targets --locked -- -D warnings`.
- 14/11/2026 sera: menu laterale fisso e comprimibile, pannello riproduzioni con dati dal vivo, ricerca nella
  libreria, "torna su", associazione TMDB migliorata (da 214 incerti a 68), loghi SVG, API Jellyfin in più.
  Infuse 8.5 su iPhone FUNZIONA (confermato dall'utente) dopo: /UserViews/GroupingOptions, niente null nelle
  risposte JSON, Fields negli elenchi (MediaSources, Path, ProviderIds, Genres). Per diagnosticare un client:
  `PLATEA_LOG=info,tower_http=debug,platea::api::jellyfin=debug` in .env (percorsi e parametri di elenco nel log,
  i "percorso non gestito" sempre a livello info). Ora di nuovo `PLATEA_LOG=info`.

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

### Test di carico (2026-11-14, `scripts/loadtest/loadtest.py`, client e server sulla stessa macchina a 8 CPU)

300 player in direct play (richieste Range da 512 KB, 10% salti casuali, pause 0,2–1 s) + 20 navigatori sull'API
(liste della libreria da 8.500 film, home, ricerca), 3 processi client da 100 player:

| Misura | Risultato |
| --- | --- |
| Richieste Range | ~490/s, 2 Gb/s, 0 errori |
| Latenza Range | p50 1,0 ms · p95 2,9 ms · p99 12–43 ms · max < 100 ms |
| Latenza API durante il carico | p50 7 ms · p95 17 ms · p99 22–29 ms |
| 900 sessioni accumulate (3 giri) | 0 errori, 33 descrittori aperti |
| Memoria dopo il carico | ~230–290 MB anonimi (trattenuti dall'allocatore, stabili tra i giri: nessuna perdita) |

Trovato e corretto durante il test:
- Limite di file aperti del container (1024) raggiunto con ~900 sessioni → 409 "File non leggibile". Ora il server alza il
  proprio limite soft al massimo (524.288 nel container) e i descrittori sono condivisi per file tra sessioni e Infuse
  (`stream::open_library_file`, aperti finché usati + 10 min).
- mmap di SQLite disattivato: ogni connessione mappava gli stessi 64 MB e la RSS mostrava 600+ MB che erano solo cache.
- Copie di sicurezza delle migrazioni: si tengono solo le ultime 2 (erano 5, 330 MB).
### Personalizzazione degli utenti e grafica (2026-11-14, richiesta dell'utente: "guarda Jellyfin e fai le funzioni mancanti")

- [x] `docs/confronto-jellyfin.md`: permessi, preferenze, navigazione, riproduzione, server, con stato e priorità.
- [x] Permessi (migrazione 0007): download del file originale (`allow_download`), fasce orarie di riproduzione
  (`access_schedule`, ora locale del server con libc `localtime_r`, fasce oltre la mezzanotte), blocco dei titoli senza
  classificazione. Download: POST `/items/{id}/download` → link firmato 6 h, riprendibile (Range), permessi ricontrollati a
  ogni richiesta; Infuse `/Items/{id}/Download` e `Policy.EnableContentDownloading`.
- [x] Preferenze (`prefs.rs`, GET/PUT `/me/preferences`): lingua audio (scelta della traccia sul server), lingua e modalità
  dei sottotitoli (predefiniti/intelligenti/sempre/solo forzati/mai; solo tracce di testo), episodio successivo con conto
  alla rovescia (anche playlist), salti, dimensione e fondo dei sottotitoli, sezioni e librerie della home, già visti nascosti,
  tema. Esposte anche a Infuse (Configuration).
- [x] Immagine del profilo (256×256 PNG in `data/avatars`, mai l'originale), iniziali colorate se manca.
- [x] Pagina della persona con filmografia in libreria; titoli simili (generi in comune, la saga prima).
- [x] "Scollega tutti gli altri dispositivi"; elenco dispositivi limitato ai 10 più recenti.
- [x] Grafica: "in scena" in cima alla home (sfondo a tutta larghezza che sfuma nel fondo, logo, un solo Play blu, riga
  sotto senza doppioni), locandine che reagiscono al passaggio, segnaposto di caricamento, focus visibile.
- [x] CORRETTI: gli "aggiunti di recente" della home ignoravano il limite d'età; avatar inesistente (`?v=0`) per gli utenti
  senza immagine (colonna nullable letta come 0).

### Priorità B: filtri, capitoli, immagini (2026-11-14)

- [x] Filtri della libreria per decennio e qualità video (4K, 1080p, 720p, SD, HDR): un'unica passata sulle tracce video,
  ~25 ms su 8.500 film (la prima versione con sottoquery per elemento impiegava fino a 9 s).
- [x] Capitoli (migrazione 0008 `media_files.chapters`): letti con `-show_chapters` durante l'analisi; per i file analizzati
  prima, letti dall'intestazione alla prima richiesta e salvati. Titoli generici o orari (mkvmerge) scartati. Player: tacche
  sulla barra e pannello per saltare.
- [x] Locandina, sfondo e logo caricati dall'amministratore (dialogo Modifica), campo bloccato; "Ripristina" torna ai metadati.
- [x] Tracce ricordate (migrazione 0009 `track_choices`): le scelte nel player si salvano come lingue sul film o sulla
  serie e vincono sulle preferenze generali alla riproduzione successiva; "sottotitoli spenti" compreso. Preferenza
  `remember_tracks`.
- [x] CORRETTO: un'immagine cambiata (anche da TMDB) teneva lo stesso identificativo, o SQLite lo riusava dopo una
  cancellazione: con la cache "immutable" i browser mostravano la vecchia. Ora riga nuova inserita prima di togliere la vecchia.

### Playlist (2026-11-14)

- [x] API `/api/v1/playlists` (elenco, crea con voci, dettaglio, rinomina, cancella, aggiungi, togli, sposta): una riga per voce;
  serie/stagioni/collezioni si espandono nei loro episodi/film; voci filtrate dai permessi correnti (libreria, età);
  playlist visibili solo al proprietario (404 agli altri). Web UI: pagina Playlist, dettaglio con riordino, "Aggiungi a
  playlist" sul dettaglio, "Riproduci tutto" e player in sequenza (`?pl=…&i=…`). Provato: James Bond → 10 voci.

### HLS in fMP4: HEVC copiato nel browser (2026-11-14)

La libreria cloud è quasi tutta HEVC + AC3 (6.827 film su 7.088): nel browser ogni film andava in transcodifica completa,
perché hls.js non legge HEVC in MPEG-TS. Ora:
- [x] Segmenti fMP4 quando il video copiato (o l'uscita) è HEVC o AV1; MPEG-TS resta per H.264 e transcodifica
  (impostazione `playback.fmp4_always` per usare fMP4 anche con H.264).
- [x] FFmpeg con muxer `hls` in fMP4, un frammento per GOP (`-hls_time 0.001`), `movflags=+frag_discont` (tfdt assoluto:
  senza, il tfdt parte da 0 e lo spostamento finisce in un'edit list diversa a ogni avvio), `-tag:v hvc1` per Apple.
  Il server legge la playlist dei frammenti dell'avvio, assegna ogni frammento al segmento pianificato in base al suo
  istante d'inizio (tolleranza 50 ms), e serve il segmento concatenando i frammenti; `init.mp4` identico tra gli avvii.
- [x] Web UI: HEVC/AV1 dichiarati in `hls_video_codecs` se MediaSource li accetta (o con HLS nativo).
- Verificato: Goldfinger HEVC dal cloud → "HEVC copiato · AC3 5.1 → AAC", tfdt dei segmenti 0, 1, 150, 300, 301 uguali
  ai tempi previsti dalla playlist (es. 1436,352 s), segmento dopo un salto in 0,5–1,4 s. In Chrome (H.264 in fMP4, perché
  il Chrome di prova non ha HEVC): avvio, salti a 3000 s e 1200 s, riproduzione corretta, zero errori in console.
  Da provare dall'utente: un browser con HEVC hardware (Safari, Chrome/Edge su Mac o Windows).

### Backup e ripristino (2026-11-14)

- [x] `backup.rs`: copie con VACUUM INTO in `data/backups` (automatiche ogni 24 h, ultime 7; manuali), elenco, scaricamento in
  streaming (prima il file passava tutto dalla memoria), cancellazione. Ripristino da una copia o da un file caricato (fuori
  dal limite di 16 MB, scritto su disco a blocchi): verifica (intestazione SQLite, tabelle di Platea, `quick_check`),
  `restore-pending.db`, uscita del processo; all'avvio, a database chiuso, il vecchio diventa `platea.db.before-restore-<ms>`
  (con WAL). Provato: copia 106 MB in 0,5 s, ripristino da elenco e da file, utente creato dopo la copia sparito, RSS 60 MB
  durante lo scaricamento. Le copie delle migrazioni si potano a ogni avvio (ultime 2). Pagina Sistema con l'elenco.
- [x] Spuntate nel piano (`docs/piano-di-progetto.md`) le voci completate delle fasi 0–5.

### Storage lento e mount bloccato (2026-11-14, `scripts/stuck-mount-test.sh`)

Mount rclone di prova (separato da quello dell'utente) a 300 KB/s, poi congelato con SIGSTOP:
- Storage lento: analisi completate (l'MKV si legge solo nell'intestazione), API max 12 ms.
- Mount bloccato: API max 11 ms su 94 richieste; scansione fermata con "mount bloccato?" (nessuna cancellazione);
  direct play dal mount bloccato → 504 StorageTimeout dopo 30 s (`fs_timeout_seconds`), le altre richieste non ne risentono.
- Durante il test di carico prima della correzione dei descrittori, una scansione pianificata aveva trovato "Too many open
  files": fermata come mount assente, nessun file tolto (la protezione ha funzionato).

Nota: un solo processo client Python satura sopra ~500 Range/s; con un solo processo il p99 misurato sale a 60–120 ms
per colpa del client, non del server.

Ottimizzazioni che hanno portato la memoria da 315 MB a 66 MB: mimalloc, cache SQLite 8 MB per connessione,
lettori inattivi chiusi dopo 60 s, mmap 64 MB, accodamento dei lavori a lotti.
Script del benchmark: `scripts/bench_library.py` (genera la libreria sintetica).

## Prossimi passi

0. Libreria cloud: 302 film senza NFO restano "in attesa" finché non c'è una chiave TMDB (chiederla all'utente).
1. Provare Infuse reale contro il server (serve un dispositivo Apple: chiedere all'utente) e correggere le differenze.
2. Fase 3: verificare QSV/VA-API/NVENC su hardware reale (qui non c'è GPU: solo software testato).
3. Fase 4: fatto (log in tempo reale, chiavi API, notify per dischi locali).
4. Fase 5: fatti test di carico, libreria sintetica, CI, storage lento e mount bloccato.

## Debito tecnico noto (da fare dopo)

- Keyframe per MP4 remoti: leggere `stss` dal box moov (ora su remoto senza Cues si transcodifica).
- `.plexmatch`: righe `ep:` per episodi con nomi non standard. NFO degli episodi (`<episodio>.nfo`) non letti.
- Chiavi API con ambito limitato: tabella presente, endpoint da fare.
- Log in tempo reale in dashboard da fare.

## Registro

- 2026-11-14 — Filtri per decennio e qualità, capitoli nel player, immagini caricate a mano; corretto il riuso degli ID immagine.
- 2026-11-14 — Personalizzazione utenti (download, fasce orarie, preferenze, avatar), persona e simili, home "in scena".
- 2026-11-14 — Playlist personali.
- 2026-11-14 — HLS in fMP4: HEVC e AV1 copiati nel browser invece di transcodificati.
- 2026-11-14 — Backup automatici e ripristino dalla dashboard; voci del piano spuntate.
- 2026-11-14 — Prova di storage lento e mount bloccato: server reattivo, nessuna cancellazione.
- 2026-11-14 — Test di carico (300 player): corretti limite dei file aperti e descrittori per sessione, mmap SQLite, copie di sicurezza.
- 2026-11-14 — Notifiche sui dischi locali, collezioni in Infuse (BoxSet), corretto l'ordinamento per data in Infuse.
- 2026-11-14 — Collezioni automatiche, limite per età applicato ovunque, rinnovo metadati per libreria, ricerca nell'indirizzo.
- 2026-11-14 — Prova sulla libreria cloud reale: NFO, .plexmatch, Cues MKV, scansione parallela, rclone rc, protezione del mount,
  corretto il taglio dei segmenti dopo un salto, coda lavori per corsia.
- 2026-11-14 — Avvio del progetto. Server, WebApp, compose, README. Fasi 0 e 1 complete, fase 2 quasi (manca Infuse).
