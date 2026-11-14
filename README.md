# Platea

Open source media server

Platea serve il file originale quando il client lo regge, fa remux o transcodifica (anche su GPU Intel, NVIDIA o AMD) solo quando deve, e scansiona librerie anche su storage cloud senza aprire i file. Server in Rust a processo unico, web UI separata, un'API pubblica per tutti i client.

- **Direct play prima di tutto.** Il client dichiara cosa sa riprodurre; il server sceglie direct play, poi remux, poi transcodifica, e registra il motivo (`VideoCodecNotSupported`, `ContainerNotSupported`, ...).
- **Scansioni leggere.** Una scansione legge solo l'elenco delle cartelle, diverse in parallelo: zero file aperti se nulla è cambiato. 8.500 film su un mount rclone in meno di 40 secondi. Sui dischi locali un film copiato compare da solo dopo pochi secondi.
- **Il mount che cade non cancella nulla.** Radice vuota o illeggibile: la scansione si ferma. Cartella che va in errore: i suoi file restano. Tanti file spariti insieme (mount a metà): nessuna rimozione finché non confermi. I file che cancelli davvero, pochi alla volta, spariscono da soli.
- **NFO e .plexmatch.** Legge gli NFO di Kodi, Jellyfin e Radarr e i `.plexmatch` di Plex: identificativi esatti e, anche senza chiave TMDB, trama, cast, generi e locandine. Le cartelle dei media non vengono mai scritte: immagini e metadati stanno nel database e nella cache.
- **Nessuna licenza, nessun limite.** Utenti, dispositivi, stream e funzioni illimitati, transcodifica hardware inclusa.
- **Collezioni automatiche.** Le saghe (James Bond, Harry Potter…) si formano da sole dagli NFO e da TMDB.
- **Limite per età.** Per ogni utente una classificazione massima: le sigle di ogni paese (T, VM14, PG-13, TV-MA…) diventano età, e i titoli oltre il limite spariscono ovunque, Infuse compreso.
- **Account con scadenza.** Dopo la data scelta l'utente continua a sfogliare la libreria ma non riproduce né scarica.
- **Tutto dalla dashboard.** Nessun file di configurazione da modificare dopo il primo avvio.

## Avvio rapido

Serve Docker con il plugin Compose.

```sh
git clone https://github.com/enrico1203/Platea.git
cd Platea
cp .env.example .env          # imposta MEDIA_DIR con la cartella dei tuoi film e serie
docker compose up -d --build
docker compose logs server | grep -i codice
```

Apri `http://<host>:8080`, inserisci il codice di primo avvio stampato nei log e segui la procedura guidata: amministratore, prima libreria, chiave TMDB, prova della GPU.

| Servizio | Porta | A cosa serve |
| --- | --- | --- |
| `web` | 8080 | Web UI (nginx con i file statici; inoltra `/api` al server) |
| `server` | 8096 | API pubblica, streaming, Infuse |

| Cartella | Contenuto | Si può cancellare |
| --- | --- | --- |
| `./data` | Database e segreto dell'installazione | No: va nel backup |
| `./cache` | Immagini ridimensionate, sottotitoli estratti | Sì |
| `./transcode` | Segmenti delle sessioni attive | Sì |
| `MEDIA_DIR` → `/media` | La libreria, in sola lettura | — |

Per aggiornare: `git pull && docker compose up -d --build`. Le migrazioni del database partono da sole, dopo una copia di sicurezza.

### GPU

Togli il commento alle righe nel servizio `server` di `docker-compose.yml`:

- Intel (QSV, VA-API) e AMD (VA-API): `devices: - /dev/dri:/dev/dri`
- NVIDIA (NVENC): il blocco `deploy` con il runtime NVIDIA

Poi in **Amministrazione → Transcodifica → Prova ora**: il server codifica e decodifica un secondo di video vero per ogni combinazione e mostra la matrice delle capacità. Se la GPU fallisce, la sessione riparte in software da sola.

### Storage cloud (rclone)

Monta lo storage sull'host e indica il punto di mount in `MEDIA_DIR` (l'opzione `rslave` fa vedere al container anche un mount fatto dopo l'avvio). Crea la libreria con il profilo **Remoto**: scansione ogni 6 ore, un'analisi alla volta, cartelle elencate in parallelo.

- **Remux senza scaricare il film.** Per gli MKV l'indice dei keyframe viene dai Cues del file (poche centinaia di KB), non dalla lettura di tutti i pacchetti: un salto a metà film parte in meno di un secondo.
- **Controllo remoto di rclone.** Se il mount ha `--rc` (per esempio `--rc --rc-addr :5572`), in **Impostazioni → Scansione** imposta l'URL (`http://host.docker.internal:5572`, aggiungendo al servizio `server` `extra_hosts: ["host.docker.internal:host-gateway"]`) e il percorso in cui il server vede la radice del mount (es. `/media`). Prima di ogni scansione il server chiede a rclone di aggiornare l'elenco delle cartelle in un colpo solo (`vfs/refresh`).
- **Protezione dal mount assente.** Vedi sopra: se il mount salta durante una scansione pianificata non si perde niente; se hai cancellato tu molti file insieme, conferma da **Amministrazione → Librerie**.

Per scansionare solo la cartella cambiata da uno strumento esterno:

```sh
curl -X POST http://<host>:8096/api/v1/scan/path \
  -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
  -d '{"path": "/media/Film/Nuovo film (2026)"}'
```

### Nomi dei file

Le stesse convenzioni di Jellyfin ed Emby: `Film/Titolo (Anno)/Titolo (Anno).mkv`, `Serie/Nome/Season 01/Nome S01E02.mkv`, episodi multipli `S01E01-E02`, episodi per data, numerazione assoluta per gli anime, identificativi espliciti `[tmdbid-603]`, `[imdbid-tt0133093]`, `[tvdbid-…]`. Sottotitoli esterni accanto al video: `Titolo (Anno).it.srt`, `.it.forced.srt`.

Metadati locali, letti una volta e mai scritti:

- `Titolo (Anno).nfo` o `movie.nfo` accanto al film, `tvshow.nfo` nella cartella della serie (formato Kodi). Con una chiave TMDB danno l'identificativo esatto; senza, i loro campi diventano i metadati.
- `.plexmatch` nella cartella del film o della serie (formato Plex: `title:`, `year:`, `tmdbid:`, `imdbid:`, `tvdbid:`, `guid: tmdb://…`). È la scelta esplicita dell'utente e vince sugli NFO.

## App compatibili

| Client | Piattaforme | Stato |
| --- | --- | --- |
| WebApp Platea | Qualunque browser: computer, tablet, telefono (installabile come app web) | OK |
| Infuse | iPhone, iPad, Apple TV, Mac (collegamento come server Jellyfin) | OK |
| App Android nativa | Telefoni, tablet, Android TV, Fire TV con Fire OS | TODO (fase 6) |
| App iOS nativa | iPhone, iPad, Mac | TODO (fase 6) |

Infuse si collega come a un server Jellyfin: indirizzo del server sulla porta 8096, utente e password di Platea, modalità diretta (quella predefinita). Platea espone solo la parte dell'API Jellyfin che serve a Infuse (login, librerie, elenchi, dettagli, immagini, ricerca, stato di visione, stream del file originale con Range, rapporti di riproduzione); le riproduzioni di Infuse compaiono in Panoramica come direct play.

## Lingue

La web UI è in italiano e inglese. Le traduzioni stanno in `WebApp/src/i18n/`: un file per lingua, con chiavi verificate da TypeScript sul dizionario italiano. Per aggiungere una lingua basta un nuovo file registrato in `locales.ts`.

## Struttura del repository

```
Server/                 Rust workspace
  crates/platea-naming    lettura dei nomi di film e serie (con test)
  crates/platea-playback  profilo dispositivo e decisione di riproduzione (funzione pura, con test)
  crates/platea-server    eseguibile: API, scanner, analisi, metadati, transcodifica, sessioni
  Dockerfile              build Rust → debian:trixie-slim + jellyfin-ffmpeg
WebApp/                 React + Vite + TypeScript, TanStack Query, hls.js
  Dockerfile              build Node → nginx:alpine
docker-compose.yml      avvia server e web UI
docs/                   piano di progetto e brand identity
scripts/cargo.sh        cargo dentro un container Rust
PROGRESS.md             stato dei lavori
```

## Sviluppo

Tutto gira in container: non servono Rust, Node o FFmpeg sull'host.

```sh
scripts/cargo.sh test                     # test del server
scripts/cargo.sh build -p platea-server   # build di sviluppo
docker run --rm -v "$PWD/WebApp:/app" -w /app node:22-alpine npx tsc -b --noEmit   # controllo dei tipi della web UI
docker compose up -d --build              # tutto lo stack
```

L'API è descritta da un documento OpenAPI generato dalla stessa tabella che registra le route: `GET /api/v1/openapi.json`. Autenticazione con `Authorization: Bearer <token>` ottenuto da `POST /api/v1/auth/login`.

## Licenza

AGPL-3.0. FFmpeg (jellyfin-ffmpeg) è un programma separato avviato dal server, con la propria licenza. Metadati e immagini da [The Movie Database](https://www.themoviedb.org): questo prodotto usa l'API di TMDB ma non è approvato né certificato da TMDB.
