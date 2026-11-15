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
- 15/11/2026: da telefono le sezioni admin non erano raggiungibili (menu laterale nascosto, la tab porta solo a
  /admin). Ora sotto la barra in alto, nelle pagine /admin/*, c'è una barra di sezioni scorrevole (`.admin-subnav`,
  solo ≤900px); l'elenco delle sezioni è uno solo (`ADMIN_LINKS` in `Layout.tsx`) per menu laterale e barra.
  Il logo in alto mostra il nome del server (Impostazioni › Generali; col nome predefinito resta il wordmark
  `platea`), anche nel titolo della scheda; dopo il salvataggio si aggiorna subito (`refreshInfo` in `session.tsx`).
  Safari su iPhone restava sul nero con l'HLS fMP4 (1917 4K HDR10, Spider-Man DOVI): ogni frammento di FFmpeg ha
  styp + sidx propri e il primo sidx del segmento composto dichiarava un solo frammento (~1 s su ~6,6 s di EXTINF);
  hls.js li ignora, il player nativo di Safari no. Ora il segmento ha un solo styp e poi solo moof/mdat
  (`append_fragment` in `transcode/mod.rs`). Verificato su un server di prova con copia del DB: segmenti 0–2 continui
  (19,4 s), salto al 600 con inizio esatto a 3620,909 s.
  **Causa vera (trovata con una pagina di prova su iPhone):** il player nativo di Safari legge i segmenti a pezzi con
  `Range: bytes=a-b`; l'API HLS ignorava Range e mandava ogni volta il segmento intero, che Safari leggeva al posto del
  pezzo → loop sul segmento 0 ed "error 3 Media failed to decode". Gli stessi segmenti di Platea serviti da nginx
  (che rispetta Range) partivano. Ora init e segmenti rispondono 206 con Content-Range (`stream::serve_bytes`).
  Verificato: 200 con Accept-Ranges, 206 con i byte esatti. **Confermato dall'utente: su iPhone funziona.**
  Intanto: se l'HLS nativo non mostra fotogrammi entro 10 s dal play, il player passa alla transcodifica
  (prima aspettava l'errore di Safari, ~3 min). Log HTTP dettagliato attivo (`PLATEA_LOG=info,tower_http=debug` in
  .env): rimettere `info` a indagine finita.
- 15/11/2026: transcodifica software più capiente. Misure su questa macchina (8 core), 30 s di film:
  1080p HEVC → H.264 veryfast 4,8×, superfast 7,7×, ultrafast 13×; 1917 4K HDR10 → 1080p: catena zscale+tonemap a
  piena risoluzione 1,5×, `tonemapx` di jellyfin-ffmpeg dopo lo scale 3,4× (decodifica 4K da sola 3,8×).
  Ora: tonemapx dopo lo scale (zscale dopo lo scale se FFmpeg non è di Jellyfin), software al massimo 1080p,
  `max_sw_transcodes` e `max_hw_transcodes` predefiniti 0 = illimitate (richiesta dell'utente; prima 2 e 8 fissi),
  configurabili; dashboard mostra ∞; suggerimenti su preset e limite nelle impostazioni,
  ripiego dalla posizione realmente riprodotta (Safari dopo un errore riporta currentTime senza senso).
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
  Da provare dall'utente: un browser con HEVC hardware (Safari, Chrome/Edge su Mac o Windows). Safari: vedi fix sidx del 15/11.

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

Aggiornati il 15/11/2026. Chiave TMDB inserita dall'utente durante l'onboarding: 10.883 film e 5.790 serie associati;
restano 321 "da verificare" e 328 non riconosciuti (da vedere a mano dall'utente).

1. [x] Keyframe degli MP4/MOV dal box `moov` (`mp4.rs`: stss + stts + ctts + elst), anche remoti: identici a ffprobe
   su 5 file di prova (H.264/HEVC, moov in testa e in coda, MOV, ritardo iniziale) e su un episodio vero dal cloud
   (190 keyframe, scarto 0 ms). MP4 frammentati: ancora ffprobe (solo locali).
2. [x] Miniature sulla barra di avanzamento (`trickplay.rs`, migrazione 0013): opzione per libreria (`libraries.trickplay`),
   lavoro `generate_trickplay` in corsia propria (1 alla volta, `nice`, pausa durante le riproduzioni e fuori dalla fascia
   oraria delle analisi), griglie 10×10 JPEG in `cache/trickplay/<file_id>/`, solo keyframe (`-skip_frame nokey`, con
   ripiego alla decodifica completa se non esce nulla: certi HEVC a GOP aperto escono fuori ordine), tone mapping per
   l'HDR. Manifesto in `playback/info` (`trickplay`), griglie da `/api/v1/trickplay/{sessione}/{n}.jpg` (permessi della
   sessione). Player: anteprima al passaggio del mouse e trascinando. Pulizia delle orfane nella manutenzione notturna.
   Impostazioni in Riproduzione (intervallo 10 s, larghezza 320, thread, pausa). Provato su server separato: film di
   10 min in 0,65 s, HDR in 0,27 s, API 200/404/401, anteprima nel browser al punto giusto, zero errori in console.
   **Richiesta dell'utente: le librerie remote hanno tutto spento per default.** La creazione di una libreria remota forza
   `extract_keyframes` e `trickplay` a falso (prima il dialogo mandava i suoi valori e accendeva i keyframe); nel dialogo
   scegliere Remoto spegne le letture complete e, per una libreria nuova, mette 360 min e 1 analisi alla volta.
   Le librerie dell'utente (tutte remote): keyframe e miniature spenti.
3. [x] Quick Connect (`quickconnect.rs`, `api/quickconnect.rs`): codici di 6 cifre validi 10 min, solo in memoria,
   segreto monouso, al massimo 5 richieste aperte per indirizzo e 256 in tutto, codici sbagliati rallentati come i login.
   API: initiate/status/redeem pubbliche, authorize con login. Strato Jellyfin: /QuickConnect/Enabled|Initiate|Connect|
   Authorize e /Users/AuthenticateWithQuickConnect. Web: "Accedi con un codice" nel login, "Collega un dispositivo" in
   Account e su `/link`. Impostazione `general.quick_connect` (attiva). Provato su server separato: flusso nativo e
   Jellyfin via curl; nel browser il dispositivo entra da solo dopo la conferma, campo del codice a 16 px, zero errori.
4. [x] `.plexmatch` righe `ep:`/`episode:` (S01E02, S01E02-E03, 2x04, assoluto, data; percorso relativo alla serie): lo
   scanner vede i `.plexmatch` durante l'elenco, li legge solo nelle librerie di serie e riconosce il file come un nome
   standard nella cartella della serie (stessa serie degli altri episodi). Vale per i file nuovi o mai riconosciuti.
   NFO degli episodi (`<episodio>.nfo`, anche con più `<episodedetails>`): lo scanner segna `media_files.has_nfo`
   (migrazione 0014) vedendo l'NFO nell'elenco, senza aprire nulla; dopo i metadati della serie (TMDB o NFO, anche
   senza chiave) gli NFO degli episodi vincono sui testi di TMDB, campi bloccati esclusi.
   **Corretto:** un episodio nuovo in una serie già associata restava "Episodio N" fino all'aggiornamento periodico
   (fino a `refresh_days`): ora la scansione mette in coda l'aggiornamento della serie. Sulla libreria dell'utente:
   nessun NFO di episodio, pochi `.plexmatch` senza righe `ep:`; 8.435 episodi "in attesa" sono numeri che TMDB non ha.
   Provato su server separato: ep: → episodi 1 e 2-3 della stessa serie, NFO dell'episodio applicato, episodio aggiunto
   dopo → serie aggiornata da sola con l'NFO nuovo.
4b. [x] Qualità predefinita per utente (`max_bitrate_local`/`max_bitrate_remote` nelle preferenze, Account): applicata dal
   server in `playback/info` se il player non ha scelto (`explicit_quality`), risposta con `max_bitrate` applicato; nel
   player la voce "Automatica". Provato: 4 Mb/s in casa → transcodifica a 4 Mb/s; scelta esplicita → originale.
4c. [x] Pagine per genere e studio (`/browse?genre=…`, `/browse?studio=…`, filtro `studio` nell'API): generi e studi
   cliccabili nel dettaglio. Provato su una copia del DB dell'utente (Dramma, HBO).
4d. [x] Blocco dopo N password sbagliate di fila (`general.lockout_after_failures`, 0 = spento, predefinito;
   migrazione 0015 `users.failed_logins`): account disattivato, voce nell'attività; gli amministratori non si
   bloccano; riattivandolo il conteggio riparte. Provato sulla copia.
   **Corretto difetto grave:** dopo il cambio "0 = transcodifiche illimitate" la validazione rifiutava 0 e, siccome
   valida tutte le impostazioni insieme, NESSUN gruppo di impostazioni si poteva più salvare. Tolta la regola,
   aggiunto un test che i valori predefiniti siano validi.
   Prove con dati veri: copia del DB in scratchpad (`reset-password` sulla copia, coda in pausa con
   `server_state.jobs_paused_until`), server e web di prova separati. Attenzione: l'immagine ha già `platea` come
   entrypoint, quindi `docker run … platea-server:latest reset-password …` (senza ripetere `platea`).
4e. [x] Trailer (migrazione 0016 `items.trailers`): fino a 3 YouTube da TMDB (`videos`, lingua della libreria poi
   inglese, Trailer ufficiali prima dei Teaser) e dal `<trailer>` degli NFO di Kodi (finché TMDB non ne ha). Pulsante
   "Trailer" nel dettaglio con video youtube-nocookie; `RemoteTrailers` nello strato Jellyfin. I titoli già associati
   li ricevono al prossimo aggiornamento dei metadati. Provato sulla copia: Dune (1984) → 3 trailer.
   Corretto: `Dialog` ora si apre in un portal su `body` (dentro il dettaglio veniva tagliata e coperta).
4f. [x] Episodi mancanti (migrazione 0017 `missing_episodes`): calcolati in `apply_seasons` dagli episodi TMDB con
   data passata non coperti da file (anche multi-episodio), speciali esclusi; API `/items/{id}/missing` (serie o
   stagione); nella lista della stagione al loro posto, in grigio con "Mancante". Si aggiornano a ogni rinnovo della
   serie (anche quando arriva un episodio nuovo). Provato sulla copia: 9-1-1 S9 → episodi 5 e 6.
4g. [x] Messaggio nella pagina di accesso (testo semplice, ≤ 2000 caratteri) e CSS personalizzato (≤ 64 KB, servito da
   `/api/v1/system/custom.css`, versione nel system/info per la cache): campi multilinea nello schema
   (`MULTILINE_FIELDS` → `x-multiline` → `TextAreaField`). Provato nel browser sulla copia.
4h. [x] Webhook (`webhooks.rs`, gruppo di impostazioni `notifications`): fino a 10 indirizzi, JSON generico, Discord
   (`content`) e ntfy (testo). Eventi: inizio riproduzione (web e Infuse, al massimo uno ogni 10 min per utente e
   titolo, così un ripiego non duplica), fine dopo almeno 1 minuto, titolo nuovo (alla prima associazione, non nella
   prima importazione né per elementi più vecchi di 24 h), accessi/password sbagliate/blocchi, problemi di scansione
   (solo al cambio di stato). Invii in task con timeout 10 s, al massimo 32 in corso. Provato con un ricevitore.
4i. [x] Playlist condivise (migrazione 0018 `playlists.shared`): il proprietario la condivide con tutti; gli altri la
   vedono (con "di <utente>") e la riproducono, senza poterla modificare; ognuno vede solo le voci permesse a lui.
   "Aggiungi a playlist" propone solo le proprie. Provato via API sulla copia (404 prima, lettura sì, modifiche no).
4j. [x] Scelta delle immagini tra le alternative di TMDB (`/items/{id}/remote-images/{kind}` GET/POST, solo admin):
   griglia nel dialogo Modifica, lingua della libreria per prima; solo percorsi TMDB accettati; campo bloccato.
4k. [x] Controllo remoto delle sessioni web (`POST /admin/sessions/{id}/command`: pause, resume, message): evento
   `session.command` sul WebSocket dell'utente; il player esegue o mostra il messaggio; interrotta dall'admin → avviso.
   Pulsanti nel pannello riproduzioni (solo client Platea). Provato: messaggio mostrato sul player.
4l. [x] Sottotitoli aggiunti (`api/subtitles.rs`): caricamento (SRT/VTT/ASS, ≤ 2 MB, Latin-1 convertito in UTF-8) e
   OpenSubtitles (chiave, utente e password in Metadati; ricerca per TMDB/IMDb, serie+stagione+episodio, hash del file
   da 128 KB; download con token se c'è l'account). Salvati in `cache/subtitles/added/`, registrati come esterni con
   percorso `@platea/…` e indice ≥ 2000: la scansione li lascia, web e Infuse li servono dalla cache. Provato con un
   finto OpenSubtitles (`PLATEA_OPENSUBTITLES_BASE`): ricerca con hash, login, download, VTT corretto, scansione.
4m. [x] Collezioni create a mano (`api/collections.rs`, chiave `manual:…`, match_state `manual`): crea, rinomina,
   cancella, aggiungi/togli film e serie (anche di librerie diverse). Il collegamento automatico alle saghe non le
   tocca più (`collections::link`). Web: "Aggiungi a collezione" nel dettaglio, gestione nella pagina della collezione.
   Provato sulla copia: Goldfinger resta in James Bond e nella collezione manuale dopo il rinnovo.
4n. Swiftfin (app Jellyfin ufficiale per iOS) dava "errore 404" dopo il login: lo strato Jellyfin dichiara 10.11 e le
   app recenti usano endpoint che mancavano. Aggiunti: /System/Configuration, /Items/Root, Similar (vero, come la web
   UI), LocalTrailers, SpecialFeatures, Ancestors, ThemeMedia, Suggestions, Movies/Recommendations, Playlists, Devices,
   MediaSegments, Localization/*, UserItems/{id}/UserData (GET/POST), POST Users/{id}/Configuration|Policy,
   DisplayPreferences (POST). Secondo tentativo (log): Swiftfin chiede anche /socket (WebSocket Jellyfin: ora
   ForceKeepAlive/KeepAlive), /UserImage (avatar 10.9+) e /Branding/Splashscreen (PNG vuoto): aggiunti.
   **Causa vera del 401:** Swiftfin 1.7 manda `Authorization: MediaBrowser Token=…, Client=…` SENZA virgolette e in
   ordine variabile; il server cercava solo `Token="`. Ora `auth::mediabrowser_fields` legge valori con/senza
   virgolette, codificati, in qualsiasi ordine (test). Jellyfin è passato da 10.11 a 12.x: dichiarata 12.1.0
   (Swiftfin chiede ≥ 12.1.0, tre numeri). Log mascherato dei token rifiutati (`richiesta Jellyfin senza token
   riconosciuto`) lasciato a livello info per diagnosi future.
   Poi: /Playback/BitrateTest (prova di velocità prima di ogni video: senza, "Impossibile caricare l'elemento"),
   LiveTv vuoti, e parametri ripetuti (`includeItemTypes=Movie&includeItemTypes=Series`) uniti con la virgola
   (prima valeva solo l'ultimo). **Confermato dall'utente: Swiftfin entra, riproduce e mostra Media.**
   Log di nuovo `PLATEA_LOG=info`. I percorsi sconosciuti finiscono nel log ("percorso non gestito").
4o. [x] Transcodifica per le app Jellyfin: PlaybackInfo legge il `DeviceProfile` (`api/jellyfin_profile.rs`:
   DirectPlay/Codec/Transcoding/SubtitleProfiles, MaxStreamingBitrate) e decide come Platea; se non è direct play apre
   la sessione HLS di Platea (`playback::start`, ora riusabile) e la restituisce come `TranscodingUrl`; i rapporti con
   `PlaySessionId` vanno a quella sessione. Infuse resta sempre in direct play. Provato: profilo solo MP4 → remux HLS,
   segmenti scaricati, progresso e stop agganciati; profilo con MKV e Infuse → direct play.
5. Fase 3: QSV/VA-API/NVENC/AMD, tone mapping in GPU, uscita HEVC/AV1 su hardware reale (qui non c'è GPU).
6. Fase 5 (rilascio). [x] Revisione di sicurezza interna (15/11/2026), corretti:
   - download delle immagini: la risposta era letta tutta prima del controllo di dimensione (una risposta senza fine
     riempiva la RAM) → lettura a pezzi, tetto 20 MB, Content-Length controllato;
   - SSRF dagli indirizzi delle immagini negli NFO (un NFO arrivato con un download poteva far chiamare al server il
     router o servizi interni, anche via redirect) → per gli indirizzi non TMDB solo http/https, nome risolto e IP
     privati/loopback/link-local/CGNAT rifiutati, nessun redirect (`images::check_external_url`, test);
   - bombe di decompressione: immagini e avatar decodificati con limiti (16384 px, 256 MB) (`images::decode_limited`);
   - XML degli NFO: tetto di 200.000 nodi (le entità ricorsive erano già bloccate da roxmltree).
   Verificati senza problemi: IP del client da X-Forwarded-For (da destra, solo proxy fidati: non falsificabile
   attraverso il tunnel), percorsi di HLS/miniature/sottotitoli esterni/download, nessun nome di file nei filtri FFmpeg,
   endpoint Jellyfin pubblici (solo info, login, Quick Connect, immagini), codice di primo avvio rallentato.
   Resta (accettato): il WebSocket porta il token nella query (può finire nei log di un proxy esterno); una revisione
   esterna resta nel piano.
   [x] Aggiornamento provato: DB creato dalla prima versione compilabile (`4000486`, solo migrazione 0001, girava
   come root) aperto dalla versione attuale: 14 migrazioni con copia di sicurezza, integrity_check ok, token, utenti,
   visto/preferito intatti, funzioni nuove ok. Trovato: i file creati da root non sono apribili dall'uid 1000 → ora
   `BootConfig::check_writable` all'avvio prova scrittura e apertura e dice il file e il `chown` da fare.
   [x] Documentazione: README con reverse proxy (Caddy, nginx), aggiornamento, ep:/NFO episodi, funzioni nuove.
   **Decisione dell'utente (15/11/2026): solo Docker. Niente binari per Windows/macOS/Linux per ora.**
7. Fase 6: client Kotlin/Swift dall'OpenAPI, app Android/TV e Apple.
8. Priorità C di `docs/confronto-jellyfin.md` e decisioni aperte del piano (licenza, sistemi supportati, PostgreSQL).

## Debito tecnico noto (da fare dopo)

- Nessuno in sospeso dai punti 1–4.

## Registro

- 2026-11-15 — Trailer, revisione di sicurezza, aggiornamento tra versioni provato.

- 2026-11-15 — Pagine per genere e studio, blocco account facoltativo, corretto il salvataggio delle impostazioni.

- 2026-11-15 — Qualità predefinita in casa e fuori casa.

- 2026-11-15 — .plexmatch ep:, NFO degli episodi, aggiornamento della serie quando arrivano episodi nuovi.

- 2026-11-15 — Quick Connect (web, API nativa e Jellyfin).

- 2026-11-15 — Miniature sulla barra di avanzamento; librerie remote con tutte le letture complete spente.

- 2026-11-15 — Keyframe degli MP4 dal moov: remux anche per gli MP4 remoti.

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
