# Platea — Stato dei lavori

Questo file è la memoria del progetto tra una sessione e l'altra. Va aggiornato a ogni passo concluso.
Se la sessione si interrompe, alla ripresa ("continua") si riparte da qui: leggere **Dove sono arrivato** e **Prossimi passi**.
Le note sull'installazione dell'utente (mount, porte, librerie) stanno in `PROGRESS.local.md`, non versionato: leggerlo insieme a questo.

## Indicazioni di lavoro (dall'utente)

- Leggere `docs/piano-di-progetto.md` e `docs/brand-identity.md` e sviluppare tutto, senza fermarsi.
- Usare sempre container Docker (niente toolchain installate sull'host: Rust, Node, FFmpeg girano nei container).
- Un `docker-compose.yml` nella root che avvia sia `Server/` sia `WebApp/`.
- Repository GitHub privata `Platea`; fare sempre commit e push su `main`.
- Testare da soli le cose (build, test unitari, prove end-to-end con curl/browser nei container).
- Tenere aggiornato il README. **Dal 15/11/2026 il README è in inglese, scritto in modo semplice (niente formule da
  intelligenza artificiale, niente grassetti a inizio elenco): una frase, app compatibili, compose d'esempio, poi il resto.**
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
  Cartelle dei media, mount rclone e loro controllo remoto: in `docker-compose.override.yml` e in `PROGRESS.local.md`
  (entrambi non versionati).
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

Mount rclone di uno storage cloud cifrato: 8.517 cartelle, 8.495 MKV quasi tutti con `.nfo` Kodi.
Montato SOLO in lettura nel container (`docker-compose.override.yml` locale, non versionato:
`/mnt/<mount>:/media/<mount>:ro,rslave` + `extra_hosts host.docker.internal:host-gateway` per l'rc di rclone).
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

Aggiornati il 15/11/2026. Stato dell'installazione dell'utente: in `PROGRESS.local.md` (non versionato).

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
4p. App Android indicate nel README come compatibili da provare (l'utente le proverà): Findroid (telefoni) e Jellyfin
   per Android TV (nativa, a differenza dell'app per telefoni). Se danno errore: log ("percorso non gestito",
   "richiesta Jellyfin senza token riconosciuto"), eventualmente `PLATEA_LOG=info,tower_http=debug` per un po'.
4q. [x] Contenuti extra (migrazione 0019 `media_files.extra_of`, `extra_type`): file con `item_id` NULL collegati al
   film o alla serie della cartella (`naming::extra_info`: cartelle Extras, Trailers, Featurettes, Behind The Scenes,
   Deleted Scenes, Interviews, Scenes, Shorts, Clips, Other, Contenuti speciali; suffissi -trailer, -featurette…;
   i "sample" ignorati; libreria con un solo titolo nella radice gestita). Tutte le query per `item_id` li escludono
   da sole. Riproduzione con `file_id` (anche per gli extra), sessione `extra`: l'avanzamento non tocca il titolo.
   Sezione "Contenuti extra" nel dettaglio. Strato Jellyfin: SpecialFeatures/LocalTrailers ancora vuoti.
   Provato su server di prova: extra di film e serie, sample ignorato, film non segnato come iniziato.
4r. [x] Generi nascosti / consentiti per utente (migrazione 0020 `users.blocked_genres`, `allowed_genres`, JSON di
   nomi risolti in identificativi al caricamento dei permessi): `AuthUser::genre_filter` nel `library_filter` (elenchi,
   ricerca, home, strato Jellyfin) e in `can_see_item` (dettaglio, riproduzione); stagioni ed episodi usano i generi
   della serie, collezioni sempre visibili. Scelta a pulsanti nel dialogo dell'utente. Provato sulla copia: horror
   nascosto ovunque (404 su dettaglio e riproduzione), "solo Animazione" → 28 film su 10.960 anche via Jellyfin.
4s. [x] File `.strm` (migrazione 0021 `media_files.stream_url`, `strm.rs`): estensione video riconosciuta dalla
   scansione; durante l'analisi si legge l'indirizzo (solo http/https, prima riga non commento, ≤ 16 KB; percorsi
   locali, file://, concat: rifiutati) e lo si salva; ffprobe/FFmpeg leggono da lì (analisi, remux, transcodifica,
   sottotitoli, capitoli, miniature) tramite `probe::media_input` e `FileInfo::input()`; direct play, download e
   stream dello strato Jellyfin passano dal server (`stream::proxy`, con Range: funziona anche con sorgenti della rete
   di casa usate da fuori, e con pagine https). Niente keyframe dai .strm (si transcodifica quando servirebbero).
   Provato: sorgente nginx, analisi, direct play 206 con byte esatti, transcodifica HLS, .strm con percorso locale
   rifiutato.
5. Fase 3: QSV/VA-API/NVENC/AMD, tone mapping in GPU, uscita HEVC/AV1 su hardware reale (qui non c'è GPU).
5b. [x] Apertura del progetto (15/11/2026, richiesta dell'utente): repository pubblica; README con licenza
   AGPL-3.0, progetti collegati con link e licenze verificate (Jellyfin GPL-2.0, Swiftfin MPL-2.0, Findroid GPL-3.0,
   Jellyfin per Android TV GPL-2.0, jellyfin-ffmpeg LGPL/GPL, Infuse proprietaria, TMDB con logo e avviso,
   rclone MIT), nessun logo altrui, avviso di non affiliazione; CONTRIBUTING.md, SECURITY.md (segnalazione privata di
   GitHub); logo TMDB approvato in Informazioni (era solo testo); esempi generici (niente dati dell'installazione).
   Immagini: `.github/workflows/release.yml` su ghcr.io (edge da main, versione + latest dai tag v*, arm64 solo nei
   tag); Docker Hub pronto ma spento finché non ci sono i segreti DOCKERHUB_USERNAME e DOCKERHUB_TOKEN.
   docker-compose usa ghcr.io/enrico1203/platea-*:${PLATEA_TAG:-latest}. Commit con email noreply di GitHub.
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
7. Fase 6: [x] app Android nativa (16/11/2026, vedi Registro). [x] documento OpenAPI con gli schemi dei dati, generati dai tipi TypeScript della web UI
   (`scripts/openapi-schemas.sh` → `api/openapi_schemas.json`, tabella route→schemi in `api/openapi_map.rs`, test che
   route e riferimenti esistano), `operationId` dal percorso; `scripts/generate-clients.sh` valida il documento e
   genera i client Kotlin e Swift (openapi-generator 7.12, in `clients/`, non versionato). Restano le app Android/TV
   e Apple. Quando cambiano i tipi della web UI: rilanciare `scripts/openapi-schemas.sh`.
8b. [x] **Nodi** (15/11/2026, richiesta dell'utente; guida in `docs/nodi.md`). Stesso eseguibile in modalità nodo
   (`platea node`, porta 8097, `docker-compose.node.yml`), senza database: chiave condivisa (`PLATEA_NODE_KEY` o
   generata in `/data/node.key`, `platea node key`), reti ammesse facoltative, sola lettura sotto `/media`.
   Ruoli, anche insieme: transcodifica (il nodo riceve il file dal server via `/api/v1/node-source/{token}` o lo legge
   dalla sua copia, e rimanda i segmenti; la playlist resta del server) e copie delle cartelle (CDN: direct play,
   download, Infuse/Jellyfin e sorgente di FFmpeg leggono dalla copia, con Range e failover).
   Bilanciamento per esecutore, server compreso (riga `is_local` della migrazione 0023): priorità a livelli, pesi
   (90/10), limite di sessioni, regole "accetta solo" (altezza, HDR, codec, librerie, transcodifica/remux, solo con
   GPU), svuotamento; impostazioni `nodes` (strategia, preferenza per chi ha il file, ripiego sul server, indirizzo
   del server per i nodi, consegna diretta con link firmati HMAC verso l'indirizzo pubblico del nodo).
   Failover: heartbeat ogni 5 s (offline dopo 3), avvio sul successivo dell'elenco, a metà sessione si riparte su un
   altro esecutore dallo stesso segmento (fMP4 transcodificato: 410 e il player riapre da solo), interruttore dopo 3
   errori in 5 min, nodo senza heartbeat per 60 s chiude FFmpeg. Pagina Amministrazione › Nodi (stato dal vivo,
   matrice GPU, log, riavvio, cartelle con Sfoglia e verifica), "Eseguita da"/"Letta da" nelle riproduzioni.
   Codice: `src/node/`, `src/nodes/` (route.rs puro con test, hls.rs, cdn.rs, client.rs, proto.rs), `api/nodes.rs`.
   Provato con `scripts/nodes-test/run.sh` (server + 2 nodi in container, file sintetici): **52 prove su 52**
   (pesi 9/1 su 10 sessioni, solo-4K, 1+2 dal file locale, failover in 2,2 s, CDN byte per byte, copia troncata
   esclusa, link firmato e manomesso, nodo orfano). Browser (Playwright): pagina su desktop e telefono, zero errori,
   campi a 16 px. **Da provare su hardware vero: un nodo con GPU (QSV/VA-API), vedi `PROGRESS.local.md`.**
8. Priorità C di `docs/confronto-jellyfin.md` e decisioni aperte del piano (licenza, sistemi supportati, PostgreSQL).

## Debito tecnico noto (da fare dopo)

- Nessuno in sospeso dai punti 1–4.

## Registro

- 2026-11-19 — **0.1.15**: un film da mount cloud restava nero nel player web (Safari, HLS nativo). Causa: il
  sottotitolo interno si estraeva in parallelo al video leggendo tutto il file (5,5 GB) e gli toglieva la banda;
  il remux non mandava `init.mp4` in 10 s, il player ripiegava sulla transcodifica, che apriva una seconda
  estrazione dello stesso sottotitolo: primo segmento dopo 59 s. Correzioni: estrazione di un sottotitolo interno
  solo dopo il primo segmento (massimo 25 s di attesa), una sola estrazione per file di cache (le altre richieste
  aspettano), semaforo separato dai keyframe; una sessione chiusa nel frattempo non estrae più. Il player web, fermo
  all'avvio, chiede al server (`client-error` con `stall: true` → `pending`) e se il server non ha ancora mandato
  nulla aspetta altri 10 s (fino a 60 s) invece di ripiegare sulla transcodifica.

- 2026-11-19 — **0.1.14**: guida XMLTV da file locale (lista M3U con percorso sul server) non si caricava: la
  cartella della cache non esisteva ancora (si creava solo per i download http). Copertine degli audiolibri prese da
  Audible segnate con origine `audible` (prima `tmdb`). Prova di contratto Android estesa alla Live TV (sorgente M3U
  + XMLTV locale, stato, canali con il programma in onda, avvio HLS): 78 OK; test JVM dell'app verdi.
  Provata la 0.1.13 pubblicata su una copia del DB vero: migrazioni 0025–0026 in 40 s, home, librerie, ricerca,
  pannello e strato Jellyfin rispondono. Sul server dell'utente la libreria degli audiolibri c'è già: 16.112 libri,
  3.848 associati (3.630 con Audible), 28 da verificare, il resto in coda, nessun errore.
  **L'aggiornamento del server di produzione aspetta la conferma dell'utente** (bloccato dai permessi).

- 2026-11-18 notte — **Live TV con Dispatcharr (0.1.13; la 0.1.12 ha un Cargo.lock sbagliato e non si compila)**, richiesta dell'utente ("fai tutta l'implementazione" del
  piano). Migrazione 0026: `tv_sources`, `tv_channels`, `tv_programs` (+FTS), `user_tv_favorites`, `tv_recordings`,
  permessi `users.allow_livetv`, `livetv_groups`, `allow_recording`.
  - Sorgenti (`livetv/mod.rs`): Dispatcharr (M3U `/output/m3u` + XMLTV `/output/epg`, indirizzi riportati all'host
    della sorgente; UUID del canale dall'URL del flusso, id numerico dall'API REST con `X-API-Key` per le
    registrazioni), M3U + XMLTV (anche file e .gz), HDHomeRun (`lineup.json`), Xtream Codes (`player_api.php`).
    XMLTV letto in streaming con quick-xml (`livetv/xmltv.rs`), finestra di giorni, abbinamento per id e poi per nome.
    Lavoro `refresh_tv_source` in corsia `livetv`, pianificato ogni minuto (canali ogni 6 h, guida ogni 12 h).
  - Visione (`livetv/stream.rs`): ffprobe del canale (codec in cache 24 h), variante copy/audio/transcode/direct,
    un FFmpeg per canale e variante condiviso, HLS a finestra (2 s × 8), fermato 30 s dopo l'ultimo spettatore,
    riacceso se cade; deinterlacciamento yadif per il DVB; limite di flussi della sorgente; sessioni dal vivo nel
    pannello (stessa forma di SessionView) e nel limite di flussi dell'utente; un dispositivo una cosa alla volta.
  - API (`api/livetv.rs`): canali con in onda/successivo, gruppi, guida a finestre, ricerca FTS, preferiti, logo in
    cache, play/ping/stop, `/api/v1/live/{sessione}/{index.m3u8|segN.ts|stream.ts}`, registrazioni (Dispatcharr:
    singole, regole di serie, stop di quelle in corso, file collegato alla libreria), amministrazione delle sorgenti
    con prova senza salvare, canali nascosti.
  - Strato Jellyfin: vista "Live TV", `/LiveTv/Info|Channels|Programs|Programs/Recommended|GuideInfo|Timers|
    SeriesTimers`, canali e programmi in `/Items/{id}`, PlaybackInfo con MPEG-TS infinito, `/Videos/{canale}/stream.ts`
    (anche senza token con il PlaySessionId). `jellyfin-check.py`: 13 chiamate conformi allo schema 12.4.
  - Web: pagina TV (guida a griglia caricata a blocchi, elenco canali, ricerca, registrazioni, scheda del
    programma), player dal vivo (`/tv/watch/:id`: cambio canale con frecce e numeri, mini guida, ripiego sulla
    conversione se il browser non decodifica), "In onda ora" in home (preferenze versione 3), Amministrazione ›
    Live TV, permessi nel dialogo utenti. App Android: schermata TV dalle Librerie e `LiveTvActivity` (Media3 legge
    l'MPEG-TS, canale su/giù dal telecomando).
  - Prove: 160 test; server di prova con Dispatcharr 0.34.1 e una sorgente IPTV finta (3 canali: H.264+AAC,
    H.264+AC3, MPEG-2 576i+MP2) — canali e 446 programmi, copia, AC3→AAC, MPEG-2→H.264 deinterlacciato, flusso
    diretto, registrazione singola e serie create in Dispatcharr, Chrome desktop e telefono (guida, scheda,
    player, cambio canale, mini guida), PlaybackInfo e stream dello strato Jellyfin. Nota: Dispatcharr accumula 4
    blocchi prima di mandare un canale (con canali a bassissimo bitrate ~20 s): tempi limite di apertura 40–45 s.
    **Da provare dall'utente** con il suo Dispatcharr e con Infuse/Swiftfin (Live TV).

- 2026-11-18 sera — Piano: Live TV con Dispatcharr (`docs/piano-di-progetto.md`, sezione omonima), richiesta
  dell'utente; SRS tolto. Dispatcharr resta un servizio separato: Platea legge canali e guida (API REST con X-API-Key,
  oppure M3U + XMLTV, HDHomeRun, Xtream Codes), trasforma il flusso MPEG-TS in HLS dal vivo condiviso per canale,
  delega le registrazioni a Dispatcharr e le legge come libreria. Sei fasi, strato Jellyfin LiveTv per Infuse e
  Swiftfin. Interfacce verificate nel codice di Dispatcharr 0.34.1. Solo progetto, nessun codice.
- 2026-11-18 sera — **Audiolibri (0.1.11)**. Richiesta dell'utente: supporto agli audiolibri, metadati e copertine come
  Emby/Jellyfin, un buon player web. Nuovo tipo di libreria `audiobooks`, elementi `audiobook` (migrazione 0025: i CHECK
  si allargano riscrivendo lo schema con `writable_schema`, niente copia delle tabelle; la connessione di scrittura si
  riapre dopo le migrazioni; colonna `media_files.tags` con i tag dei file audio; ruoli `author`/`narrator`).
  - Riconoscimento (`platea-naming/src/audiobook.rs`): cartella con file audio = un libro, file = parti (traccia, poi
    data nel nome per i podcast, poi ordine naturale); `CD1`/`Disc 2` appartengono al libro sopra; ogni `.m4b` è un
    libro (più m4b in una cartella = serie/raccolta, la cartella diventa la collezione); `Libro (1).m4b` = copia,
    `Libro - Parte 1.m4b` = parte; nomi tipo `Autore - Titolo 2021 Serie 4 Letto da Lettore @tag`.
  - Metadati (`metadata/audiobook.rs`): metadata.json (Audible o Audiobookshelf) e .opf → NFO (album.nfo di Jellyfin,
    book.nfo) e desc.txt/reader.txt → tag del primo file → nome; poi il catalogo pubblico Audible (`api.audible.<paese>`,
    per ASIN o titolo+autore, punteggio con la durata; dubbi → "Da verificare", Identifica accetta titolo o ASIN).
    In un libro a più file con album diversi (raccolta di puntate) titolo e trama di album/NFO non valgono.
    Traduttori e prefatori di Audible tolti dagli autori. Autori anche in `original_title`, così la ricerca li trova.
    Copertina: immagine nella cartella → Audible (1024 px) → incorporata (estratta con FFmpeg, senza ricodifica).
    Impostazioni `metadata.audible` (attivo) e `metadata.audible_region` ("auto" = paese della libreria).
  - Riproduzione: una sessione per parte (`playback/info` con `start_ms` nel libro, risponde `part_offset_ms` e
    `book_duration_ms`), posizione e durata della sessione sono quelle del libro (pannello admin corretto); un libro è
    finito a meno di un minuto dalla fine, non al 90%. File che il browser non legge: HLS solo audio (niente keyframe).
    `audiobook.rs`: parti, durata totale (aggiornata dopo ogni analisi), capitoli sull'intero libro.
  - Web: player persistente (`audio/AudioPlayer.tsx` + `AudioBar.tsx`): barra in basso su ogni pagina, vista completa
    con capitoli, velocità 0,5–3× (tono invariato), salti configurabili, timer (minuti o fine capitolo, dissolvenza),
    Media Session (schermata di blocco, cuffie), avanzamento ogni 10 s/pausa/chiusura, ripresa dopo pause lunghe ed
    errori, libro ripreso dopo un ricaricamento. Pagina del libro (`pages/Audiobook.tsx`), copertine quadrate, "Continua
    ad ascoltare" in home (preferenze `home_sections_version` 2: compare una volta anche a chi aveva già scelto).
    Lo strato Jellyfin non mostra le librerie di audiolibri.
  - Prove: 150 test; server di prova con 8 cartelle vere (65 file, 30 libri) — scansione 76 ms, tutti associati ad
    Audible con copertina in ~1 min; API (direct play 206, HLS solo audio con segmento 100 a 600,2 s, finito/posizione,
    ricerca per autore); Chrome con Playwright desktop e telefono (riproduzione, salto di capitolo, 1,5×, timer, parti
    mp3, ripresa tra dispositivi e dopo ricarica, zero errori). Migrazione provata su una copia del DB vero: integrità
    ok, avvio in 37 s. **Da fare dall'utente:** creare la libreria di audiolibri (profilo
    Remoto) e provare il player su iPhone (schermata di blocco, riproduzione in background).

- 2026-11-18 — **Findroid: il video non partiva (0.1.10)**. Dopo il login (ok con la 0.1.9) la riproduzione restava
  ferma: Findroid apre `/Videos/{id}/stream?static=true&…` con ExoPlayer senza token (né intestazioni né `api_key`) e
  Platea rispondeva 401 a ogni tentativo. Jellyfin non chiede l'autenticazione su quell'URL; Platea sì, ma ora accetta
  lo stream senza token se c'è un PlaybackInfo recente (12 h, rinnovate a ogni richiesta) di un utente autenticato
  per lo stesso elemento: riconosciuto dal `PlaySessionId` dato in risposta o, se manca, dallo stesso IP; il token di
  quel PlaybackInfo deve esistere ancora (`StreamUser` e `STREAM_GRANTS` in `api/jellyfin.rs`). Provato su server di
  prova: 206 con PlaySessionId e dallo stesso IP, 401 su altri film, PlaySessionId inventato o di un altro film, e
  dopo il logout. **Da confermare dall'utente su Findroid.**
- 2026-11-18 — **Findroid (Android): login e API conformi all'SDK Kotlin di Jellyfin (0.1.9)**. Findroid dava "errore
  sconosciuto" subito dopo il login (200): l'SDK Kotlin di Jellyfin 1.9.4 (API 12.4) rifiuta le risposte se manca un
  campo non nullable. Mancavano: UserConfiguration/UserPolicy (DisplayMissingEpisodes, SyncPlayAccess, …),
  SessionInfo (SupportedCommands, IsActive, …), nelle sorgenti (GenPtsInput, HasSegments, TranscodingSubProtocol…),
  nei flussi (IsInterlaced, VideoRange anche per audio/sottotitoli, AudioSpatialFormat…), `Studios[].Id`
  (prefisso GUID `4000…`, solo in uscita), MediaType e UserData.ItemId delle librerie; preferiti/visti ora
  restituiscono lo UserItemDataDto completo; aggiunto `POST /Devices/Options`. Nuovo `scripts/jellyfin-check.py`:
  confronta le risposte con lo schema OpenAPI di Jellyfin. Le 37 chiamate di Findroid (presi dal suo
  `JellyfinRepositoryImpl`) passano con 0 problemi su un server di prova con copia del DB.
  Nota: `/tmp` dell'host è un tmpfs da 7,9 GB: le copie del DB (4 GB) vanno in `/var/tmp`, non nello scratchpad.
  **Da confermare dall'utente su Findroid**; log HTTP dettagliato attivo in .env, rimettere `PLATEA_LOG=info` dopo.
- 2026-11-17 sera — **Personalizzazione completa: nome, logo, "Nascondi Platea" (0.1.8)**. Richiesta dell'utente: col
  nome scelto dall'amministratore (es. "itdrive") non si deve capire che il server è Platea, e si può cambiare il logo.
  Server: `api/branding.rs` (logo in `data/branding/`: SVG com'è, raster → `logo.png` + icone quadrate 192/512;
  servito con CSP sandbox; versione in `logo.json`, in memoria in `AppState.branding`), impostazioni
  `general.white_label` e `general.source_url`. `/api/v1/system/info` ha `white_label`, `logo`, `source_url`; `product`
  diventa il nome del server. Testata di index.html (titolo, descrizione, favicon, apple-touch-icon) e manifest
  generati dal server: nginx li inserisce con **SSI** (`<!--# include virtual="/api/v1/branding/head.html" -->`
  in `index.html`, `ssi on` in `location /`) e inoltra `/manifest.webmanifest`; con `PLATEA_WEB_DIR` il server sostituisce
  il segnaposto da sé. Senza logo e con Platea nascosto: icona con l'iniziale (`/api/v1/branding/icon.svg`).
  Tolti gli altri "Platea" visibili: OpenAPI (titolo col nome), Jellyfin (`PlateaVersion` via, provider di
  autenticazione con i nomi di Jellyfin), mittente Discord dei webhook, messaggi su sottotitoli, ripristino, nodi.
  Web UI: `{app}` in ogni testo (`setAppName` in i18n, impostato dalla sessione), `Mark` = logo / iniziale / P,
  logo largo senza il nome accanto, ultimo marchio visto in `localStorage` (`platea.brand`) per la pagina di
  collegamento, pannello Logo in Impostazioni › Generali, "Platea Web" mostrato col nome del server, file scaricati
  `<nome>-backup.db`. Android: `Mark` mostra logo (coil-svg) o iniziale, Informazioni col nome del server.
  Restano visibili solo a chi cerca: immagini Docker, comando `platea`, `X-Platea-Key`, chiavi del browser, nome e
  icona dell'app Android nel launcher, nomi dei file interni (platea.db, backup).
  Provato su uno stack di prova (porte 18080/18096, dati in scratchpad): SSI e manifest col nome e col logo, icone
  512 PNG, monogramma, OpenAPI, System/Info/Public; browser (Playwright) su desktop e telefono: login, home,
  Informazioni, Impostazioni senza "Platea" nel testo.

- 2026-11-17 — **Chiavi API con ambito `admin` (0.1.7)**. Prima le chiavi avevano solo l'ambito `scan`: un pannello
  esterno che crea e gestisce gli account (utenti, password, stream, librerie, blocco, sessioni, dispositivi) doveva
  fare il login con un amministratore. Ora una chiave può avere ambito `admin`: nell'intestazione `X-Platea-Key` vale
  come un amministratore su tutta l'API (`resolve_admin_key` in `auth.rs`, utente sintetico `user_id` 0 di nome
  "API <nome chiave>", che compare nel registro attività). Vale anche per le scansioni. In Sistema › Chiavi API si
  sceglie l'ambito, con avviso per `admin`. README aggiornato. Provato dal vivo: creazione utente, password (login ok),
  `max_streams`, `library_ids` senza la libreria 4k, `allow_transcode`, `disabled`, sessioni, dispositivi, eliminazione;
  una chiave `scan` riceve 403 "Questa chiave API non ha l'ambito admin".
- 2026-11-16 notte — **App Android provata sugli emulatori e ridisegnata** (richiesta dell'utente: "verifica con
  l'emulatore, sistema tutti i problemi, migliora la grafica"). Prove sul Mac dell'utente: emulatori telefono
  (API 36), tablet (API 36) e Android TV (API 34), server di prova dall'immagine ghcr con media sintetici e un TMDB
  finto che disegna locandine e sfondi (strumenti nello scratchpad, non versionati). Build locale con JDK 21
  (`cd Android && ./gradlew assembleDebug`, `local.properties` non versionato).
  Bug corretti: (1) **dopo il login si tornava al login** (campi vuoti; sulla TV un nuovo codice Quick Connect):
  `Session.reload()` girava nella coroutine della schermata di accesso, che usciva dalla composizione appena lo
  stato passava a Loading; l'annullamento faceva fallire `/me`. Ora ricaricamento nello scope della sessione
  (`acceptToken`, `logout`, `retry`) e la CancellationException non è più un errore. (2) Tema chiaro all'avvio sui
  telefoni in modalità chiara: ora il tema è del dispositivo come nella web UI, scuro predefinito (`Store.theme`;
  "Come il sistema" = `auto`). (3) Icone della barra di stato illeggibili: colore secondo il tema e velo sotto la
  barra di stato. (4) Marchio con la P bianca fissa (invisibile sul chiaro): disegnato col colore del tema.
  (5) Sezione della barra non evidenziata nelle pagine interne. (6) Su TV lo scorrimento "a perno" di Compose
  tagliava la parte alta di Home e schede: `TvBringIntoView` scorre solo quanto serve. (7) Sul tablet la barra
  laterale finiva prima della barra di navigazione. (8) Titoli, filtri e lettere fuori margine nelle griglie.
  (9) I test delle schermate si bloccavano (runBlocking sul thread principale): login del test su un altro thread
  con scadenza di 60 s, timeout di 15 min sul task Gradle e di 30 min sul job CI, ID dispositivo fisso nei test.
  Grafica: font Geist e Geist Mono inclusi (OFL, licenza in `assets/licenses`), scala tipografica del marchio
  (display/h1/h2/eyebrow/mono), Home con carosello "In scena" (fino a 5, avanzamento ogni 8 s, uno solo sulla TV),
  locandine con angoli 6 dp, filo interno e luce blu a fuoco col telecomando, scheda con sfondo grande, tag tecnici
  in mono (risoluzione, HDR, codec, audio), Riproduci a tutta larghezza sul telefono e azioni con etichetta,
  librerie come card con lo sfondo di un titolo recente, campi `surface-2`, account a righe (scelte a destra,
  Altro con Informazioni/Cambia server/Esci), stati vuoti col marchio monocromatico, Play del player con la luce.
  Test: 6 di contratto + 33 schermate (telefono, tablet, TV) verdi; APK di rilascio ~3,7 MB provato (login, direct
  play MP4, remux MKV+AC3). **Firma:** il 2026-11-17, su autorizzazione dell'utente, creata la chiave di rilascio
  (RSA 4096, alias `platea`) e caricati i segreti `ANDROID_KEYSTORE_*`: da qui in poi gli APK della CI sono tutti
  firmati con la stessa chiave e gli aggiornamenti si installano sopra. Chi ha un APK precedente deve disinstallarlo
  una volta. La chiave non è nel repository (dove sta: `PROGRESS.local.md`).

- 2026-11-16 — **App Android nativa** (`Android/`, richiesta dell'utente: "completa con tutte le schermate e il player,
  per telefoni, tablet e Android TV tipo Fire Stick"). Una sola app Kotlin + Jetpack Compose + Media3 (ExoPlayer),
  minSdk 22 (Fire OS 5), pacchetto `app.platea`. Su TV (leanback/Fire TV/niente touch) barra laterale, focus visibile
  col telecomando, accesso con Quick Connect come prima scelta; su tablet barra laterale, su telefono barra in basso.
  Schermate: collegamento al server (prova https, :8096, :8080), server irraggiungibile, login con messaggio, Quick
  Connect, Home (in scena, continua a guardare, prossimi episodi, aggiunti per libreria, ordine delle preferenze),
  Librerie, Libreria (ordine, filtri, decennio, qualità, genere, lettere, ricerca, pagine col cursore), generi/studi,
  Ricerca divisa per libreria, Scheda (film/serie/stagione/episodio/collezione: riprendi/dall'inizio, trailer su
  YouTube, visto, preferito, playlist, download col DownloadManager, stagioni, episodi mancanti, extra, dettagli
  tecnici per versione, cast, simili), Persona, Playlist (riproduci tutto, sposta, togli, rinomina, condividi,
  elimina), Account (preferenze salvate come oggetto intero, home, tema, collega un dispositivo, dispositivi,
  password, cambia server, esci), Informazioni (profilo del dispositivo).
  Player (`player/`): attività a sé, orizzontale sui telefoni, immersivo, picture-in-picture, MediaSession. Profilo dal
  `MediaCodecList` (H.264/HEVC/AV1/VP9 con 10 bit e risoluzione massima) + passthrough AC3/E-AC3/DTS/TrueHD +
  HDR dello schermo (`data/Profile.kt`). Direct play di MKV/MP4, HLS per remux/transcodifica, sottotitoli WebVTT
  esterni (a immagine: nuova decisione impressa), audio cambiato nel file in direct play, qualità, capitoli,
  anteprime trickplay, doppio tocco, D-pad (OK play/pausa, ←/→ salti, barra con punto d'arrivo), tasti media,
  ripiego automatico sulla transcodifica (errore o traccia non decodificabile), riapertura su 410, avanzamento ogni
  10 s, stop, episodio/voce successiva con conto alla rovescia, comandi dalla dashboard (WebSocket).
  Build: `scripts/gradle.sh` (immagine `Android/docker`, JDK 17 + SDK 35). Prove: niente KVM qui, quindi niente
  emulatore. `scripts/android-test/contract.py` avvia un server di prova separato (porta 18196, solo `./media`),
  72 controlli sull'API come la usa l'app (anche decisioni: telefono HEVC → direct play, Fire TV vecchio → transcodifica,
  MKV+AC3 su telefono → remux con AAC) e salva le risposte in `Android/app/src/test/resources/contract/` (token tolti);
  `ContractTest` le decodifica coi modelli dell'app (6/6, anche in CI). `ScreensTest` (Robolectric + Roborazzi)
  disegna le schermate vere sulla JVM collegate al server di prova, per telefono, tablet e TV (33/33), immagini in
  `Android/app/build/screens/`. CI: `.github/workflows/android.yml` (test + APK; con un tag allega platea-X.Y.Z.apk
  alla release). APK di rilascio ~3,2 MB. **Da provare dall'utente su dispositivi veri** (telefono, Fire TV).
  Firma: per ora chiave di debug; per aggiornamenti senza disinstallare servono i segreti ANDROID_KEYSTORE_* (vedi
  il workflow). Amministrazione: resta nella web UI (non nell'app).
- 2026-11-16 — Versione 0.1.6 (insieme alle voci sotto): Panoramica › Risorse con i grafici di CPU (macchina e
  Platea, cgroup del container compreso FFmpeg), rete del container (inviati/ricevuti), memoria e riproduzioni/
  transcodifiche; ultima ora (ogni 5 s) o 24 ore (media al minuto), solo in memoria (`src/metrics.rs`,
  `GET /api/v1/admin/metrics`, componente `LineChart`, colori validati per daltonismo). Attività: 50 righe per pagina,
  ricerca per testo e filtro per utente in Riproduzioni e Registro (`page`, `q`, `user_id`). Pagina della persona
  divisa per libreria come la ricerca.
- 2026-11-16 — Versione 0.1.6: nella scheda del nodo, se il nodo non raggiunge il server ma i file glieli manda il
  server (modalità automatica o "il server manda"), niente più rosso: "File da transcodificare · Li manda il server"
  con una nota. Rosso solo con "Il nodo legge dal server". Stesso criterio nella prova di velocità e nel log (INFO).
  Dischi Blu-ray copiati come cartella (`BDMV/STREAM/*.m2ts`): il film è solo il flusso più grande, con il nome della
  cartella del disco; gli altri flussi e `BDMV/BACKUP` si ignorano. Prima ogni flusso era un "film" (00021, 00050…,
  156 nella libreria Film, in cima all'ordine per titolo). Migrazione 0024: toglie quei file e i loro elementi, la
  scansione successiva rilegge il disco. Provata su un estratto del database reale (via 156 file e 156 elementi, nient'altro).
- 2026-11-16 — Versione 0.1.5: prova di velocità tra server e nodo (pulsante Velocità nella scheda del nodo).
  Tre misure da 5 s con dati casuali: server → nodo, nodo → server, il nodo legge dal server (token monouso su
  `/api/v1/node-speed/`), più la latenza. Ultimo risultato nella scheda. Prova end-to-end: 84/84.
- 2026-11-16 — Versione 0.1.4: partenza più rapida con i file ricevuti dal server. Il blocco sotto un lettore fermo
  (FFmpeg che apre un MKV salta a inizio, indice in fondo, punto di ripresa) passa davanti alla lettura anticipata e
  la sospende finché non arriva; prima aspettava i 4 blocchi già in viaggio a ogni salto. Avviso "il nodo non
  raggiunge il server" sul nodo solo in debug (il server lo segnala quando cambia).
- 2026-11-16 — Versione 0.1.3: nodi (parità al nodo, prova di collegamento, lettura a blocchi, push dal server).
- 2026-11-16 — Nodi: il server prova se il nodo lo raggiunge (`POST /node/v1/probe`, ogni 5 min, 15 s se fallisce) e
  lo mostra nella scheda; un nodo che non lo raggiunge non riceve transcodifiche di file che non ha in copia (prima:
  "Connection refused" e failover dopo ~24 s).
- 2026-11-16 — Nodi: lettura a blocchi dei file ricevuti dal server (`node/fetch.rs`): blocchi da 8 MiB in parallelo,
  in anticipo su FFmpeg, cache su disco con tetto (`/data/sources`). Modalità nel dialogo del nodo: a blocchi in
  anticipo (predefinita, 1024 MB, 4 in parallelo, cache 50 GB), copia completa, in diretta. FFmpeg legge da
  `127.0.0.1:8097/node/v1/local-source/<token>`. Rete della prova dei nodi con sottorete esplicita (10.201.77.0/24).
- 2026-11-16 — Nodi: "Chi muove il file" (automatico/il nodo legge/il server manda). In push il server manda i blocchi
  al nodo (`nodes/push.rs`, `/node/v1/push/{id}/wants|chunk/{n}`): basta che il server raggiunga il nodo. Automatico =
  push se la prova di collegamento fallisce. Pensato per i nodi dietro NAT o con la VPN solo sul router.
- 2026-11-16 — Nodi: a parità di priorità, peso e carico vince il nodo invece del server (prima la prima transcodifica
  restava sempre sul server). Vale anche per le copie CDN. Test `tie_goes_to_the_node`.
- 2026-11-15 — Release v0.1.2 (nodi compresi): immagini 0.1.2, 0.1 e latest su ghcr.io (amd64 e arm64). Versione nel codice allineata (era rimasta 0.1.0).
- 2026-11-15 — Nodi: transcodifica e copie delle cartelle su altre macchine, con bilanciamento e failover.
- 2026-11-15 — Release v0.1.1 (prima con immagini native amd64+arm64; la build di v0.1.0 in emulazione è stata annullata, tag e release v0.1.0 restano senza immagine del server).
- 2026-11-15 — File .strm (indirizzi http/https), riprodotti attraverso il server.
- 2026-11-15 — Cambio di tipo di una libreria (film ↔ serie): prima veniva ignorato. Ora i titoli si cancellano e la
  scansione li ricrea dagli stessi file, tenendo righe e analisi (`change_kind` in `api/libraries.rs`, file senza titolo
  ricollegati in `scanner::apply_listing`). Avanzamento e visti dei vecchi titoli si perdono; la UI lo avvisa.
- 2026-11-15 — Avanzamento dei lavori (`GET /api/v1/admin/jobs/progress`, `components/JobProgress.tsx` in Panoramica e
  Lavori): per tipo e libreria, fatti/totale del lotto (dal lavoro in attesa più vecchio), velocità sugli ultimi 15
  minuti e tempo stimato. Scansioni: cartelle lette dal vivo (`scanner::progress`) su quelle dell'ultima scansione
  completa (migrazione 0022 `libraries.last_scan_dirs`); alla prima scansione solo il conteggio.
- 2026-11-15 — Ricerca della web UI divisa per libreria (lo stesso film in Film, Film HD e Film 4k si distingue).
  Solo lato web: `/api/v1/search` e le ricerche Jellyfin (Infuse, Swiftfin) sono invariate, verificate dopo la modifica.

- 2026-11-15 — Rilascio: build native amd64 e arm64 (niente emulazione), immagini unite per digest.

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
