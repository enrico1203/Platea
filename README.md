# Platea

Open source media server

Platea serve il file originale quando il client lo regge, fa remux o transcodifica (anche su GPU Intel, NVIDIA o AMD) solo quando deve, e scansiona librerie anche su storage cloud senza aprire i file. Server in Rust a processo unico, web UI separata, un'API pubblica per tutti i client.

- **Remux leggero anche per HEVC.** Un browser che decodifica HEVC (Safari, Chrome ed Edge con supporto hardware) riceve il video copiato in segmenti fMP4: il server converte solo l'audio.
- **Direct play prima di tutto.** Il client dichiara cosa sa riprodurre; il server sceglie direct play, poi remux, poi transcodifica, e registra il motivo (`VideoCodecNotSupported`, `ContainerNotSupported`, ...).
- **Scansioni leggere.** Una scansione legge solo l'elenco delle cartelle, diverse in parallelo: zero file aperti se nulla è cambiato. 8.500 film su un mount rclone in meno di 40 secondi. Sui dischi locali un film copiato compare da solo dopo pochi secondi.
- **Il mount che cade non cancella nulla.** Radice vuota o illeggibile: la scansione si ferma. Cartella che va in errore: i suoi file restano. Tanti file spariti insieme (mount a metà): nessuna rimozione finché non confermi. I file che cancelli davvero, pochi alla volta, spariscono da soli.
- **NFO e .plexmatch.** Legge gli NFO di Kodi, Jellyfin e Radarr e i `.plexmatch` di Plex: identificativi esatti e, anche senza chiave TMDB, trama, cast, generi e locandine. Le cartelle dei media non vengono mai scritte: immagini e metadati stanno nel database e nella cache.
- **Nessuna licenza, nessun limite.** Utenti, dispositivi, stream e funzioni illimitati, transcodifica hardware inclusa.
- **Collezioni automatiche.** Le saghe (James Bond, Harry Potter…) si formano da sole dagli NFO e da TMDB.
- **Limite per età.** Per ogni utente una classificazione massima: le sigle di ogni paese (T, VM14, PG-13, TV-MA…) diventano età, e i titoli oltre il limite spariscono ovunque, Infuse compreso.
- **Ogni utente a modo suo.** Lingua dell'audio e dei sottotitoli, episodio successivo automatico, home su misura, immagine del profilo; per l'amministratore download consentito o no, fasce orarie, profili per bambini.
- **Account con scadenza.** Dopo la data scelta l'utente continua a sfogliare la libreria ma non riproduce né scarica.
- **Miniature sulla barra di avanzamento**, attivabili per libreria (spente di default e sempre spente alla creazione di una libreria remota, perché richiedono di leggere ogni file per intero): griglie JPEG in `cache/trickplay`, solo keyframe decodificati, un file alla volta, in pausa durante le riproduzioni.
- **Quick Connect.** Sul televisore o in un'app si sceglie "Accedi con un codice": si conferma il codice di 6 cifre da un dispositivo già collegato (Account › Collega un dispositivo, oppure `/link`). Funziona anche con le app Jellyfin che lo supportano. Si spegne in Impostazioni › Generali.
- **Qualità predefinita.** Ogni utente sceglie una qualità per la rete di casa e una per fuori casa; nel player si cambia per il singolo video.
- **Generi e studi.** Dal dettaglio di un titolo si apre la pagina di un genere o di uno studio (HBO, Pixar…) con i titoli di tutte le librerie.
- **Trailer.** Dai metadati di TMDB e dagli NFO di Kodi, nel dettaglio del titolo e in Infuse.
- **Notifiche.** Webhook verso Discord, ntfy, Home Assistant o uno script: riproduzioni, titoli nuovi, accessi, mount che non risponde (Impostazioni › Notifiche).
- **Sottotitoli in più.** L'amministratore carica un SRT/VTT/ASS o lo cerca su OpenSubtitles (con l'hash del file, per la versione giusta): finisce nella cache di Platea, non nella cartella del film.
- **App Jellyfin.** Oltre a Infuse, le app Jellyfin si collegano al server (Swiftfin 1.7 provato: Platea si presenta come Jellyfin 12.1); quando un'app non apre un formato, Platea fa remux o transcodifica in HLS come per la web UI.
- **Tutto dalla dashboard.** Nessun file di configurazione da modificare dopo il primo avvio.

## Avvio rapido

Serve Docker con il plugin Compose.

```sh
git clone https://github.com/enrico1203/Platea.git
cd Platea
cp .env.example .env          # porte, fuso orario, tunnel (facoltativo)
# in docker-compose.yml, servizio server → volumes: una riga per ogni cartella dei media
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
| Cartelle dei media → `/media/…` | La libreria, in sola lettura: una riga per ognuna nei `volumes` del servizio `server` (es. `/mnt/jottamio:/media/jottamio:ro,rslave`) | — |

**Utente dei container.** Server e web UI non girano come root: il server usa l'utente `platea` (uid e gid 1000), nginx l'utente `nginx`. Le cartelle `data`, `cache` e `transcode` devono appartenere all'uid 1000; aggiornando da una versione precedente: `docker compose down && sudo chown -R 1000:1000 data cache transcode && docker compose up -d`. Se non sono scrivibili, il server si ferma con questo messaggio nei log.

**Password dimenticata.** Da terminale, con il server in funzione: `docker compose exec server platea reset-password <utente>` stampa una password nuova (o `… reset-password <utente> <password>` per sceglierla). L'account torna attivo e tutti i suoi dispositivi vengono scollegati.

Per aggiornare: `git pull && docker compose up -d --build`. Le migrazioni del database partono da sole, dopo una copia di sicurezza (`data/platea.db.bak-…`, si tengono le ultime 2). Provato aggiornando dalla prima versione pubblicata a quella attuale in un colpo solo: account, token, stato di visione e preferiti restano. Le versioni più vecchie giravano come root: se dopo l'aggiornamento il server si ferma con «i dati non sono accessibili all'utente del server», esegui il `chown` indicato sopra.

**Backup.** Ogni 24 ore il server copia il database in `./data/backups` (le ultime 7; intervallo e numero in Impostazioni → Generali). Da **Amministrazione → Sistema** si crea una copia al volo, la si scarica, o si ripristina il database da una copia o da un file: il server si riavvia da solo e il database precedente resta in `./data` come `platea.db.before-restore-…`.

### GPU

Togli il commento alle righe nel servizio `server` di `docker-compose.yml`:

- Intel (QSV, VA-API) e AMD (VA-API): `devices: - /dev/dri:/dev/dri` e `group_add` con il gruppo che possiede `/dev/dri/renderD128` sull'host (`stat -c %g /dev/dri/renderD128`): il server non gira come root
- NVIDIA (NVENC): il blocco `deploy` con il runtime NVIDIA

Poi in **Amministrazione → Transcodifica → Prova ora**: il server codifica e decodifica un secondo di video vero per ogni combinazione e mostra la matrice delle capacità. Se la GPU fallisce, la sessione riparte in software da sola.

### Storage cloud (rclone)

Monta lo storage sull'host e aggiungi il punto di mount ai `volumes` del servizio `server` (l'opzione `rslave` fa vedere al container anche un mount fatto dopo l'avvio). Crea la libreria con il profilo **Remoto**: scansione ogni 6 ore, un'analisi alla volta, cartelle elencate in parallelo.

- **Remux senza scaricare il film.** Per gli MKV l'indice dei keyframe viene dai Cues del file, per gli MP4/MOV dal box `moov` (poche centinaia di KB), non dalla lettura di tutti i pacchetti: un salto a metà film parte in meno di un secondo.
- **Controllo remoto di rclone.** Se il mount ha `--rc` (per esempio `--rc --rc-addr :5572`), in **Impostazioni → Scansione** imposta l'URL (`http://host.docker.internal:5572`, aggiungendo al servizio `server` `extra_hosts: ["host.docker.internal:host-gateway"]`) e il percorso in cui il server vede la radice del mount (es. `/media`). Prima di ogni scansione il server chiede a rclone di aggiornare l'elenco delle cartelle in un colpo solo (`vfs/refresh`).
- **Protezione dal mount assente.** Vedi sopra: se il mount salta durante una scansione pianificata non si perde niente; se hai cancellato tu molti file insieme, conferma da **Amministrazione → Librerie**.

Per scansionare solo la cartella cambiata da uno strumento esterno:

```sh
curl -X POST http://<host>:8096/api/v1/scan/path \
  -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
  -d '{"path": "/media/Film/Nuovo film (2026)"}'
```

### Accesso da fuori (tunnel Cloudflare)

Il compose ha un servizio `cloudflared`, spento finché non lo attivi: nessuna porta da aprire sul router.

1. Nel pannello Cloudflare, **Zero Trust → Networks → Tunnels → Create a tunnel** (tipo Cloudflared), e copia il token.
2. In `.env`: `COMPOSE_PROFILES=tunnel` e `CLOUDFLARE_TUNNEL_TOKEN=<token>`, poi `docker compose up -d`.
3. Nel tunnel, **Public hostnames**: `platea.tuodominio.it` → `http://web:80` (web UI) e, per Infuse, `infuse.tuodominio.it` → `http://server:8096`. Infuse si collega a `https://infuse.tuodominio.it` sulla porta 443.
4. In **Impostazioni → Generali → URL pubblico** l'indirizzo per Infuse (`https://infuse.tuodominio.it`): è quello che il server comunica alle app Jellyfin.

Il server legge l'indirizzo vero del client da `X-Forwarded-For` (il tunnel arriva da una rete Docker, già tra i proxy fidati): chi entra dal tunnel è un accesso remoto, quindi valgono il permesso di accesso da remoto e il bitrate massimo da remoto. Due limiti di Cloudflare: le richieste oltre 100 MB non passano (il ripristino del database da file si fa dalla rete locale) e le condizioni del piano gratuito non sono pensate per molto traffico video. Cloudflare Access (il login di Cloudflare) si può mettere davanti alla web UI, non davanti all'hostname di Infuse: Infuse non sa superarlo.

### Reverse proxy

La web UI (porta 8080) inoltra già `/api` al server: basta mettere il proxy davanti a lei. Per le app Jellyfin (Infuse) serve un nome a parte verso la porta 8096. Il proxy deve: passare il WebSocket (`/api/v1/events`), non bufferizzare (i video passano in streaming, le richieste Range restano tali), avere timeout lunghi, e mandare `X-Forwarded-For`. L'indirizzo del proxy va in **Impostazioni → Rete → Proxy fidati** se non è già nelle reti private predefinite.

Caddy:

```caddy
platea.example.com {
    reverse_proxy 127.0.0.1:8080 {
        flush_interval -1
    }
}
infuse.example.com {
    reverse_proxy 127.0.0.1:8096 {
        flush_interval -1
    }
}
```

nginx:

```nginx
server {
    server_name platea.example.com;
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_buffering off;
        proxy_request_buffering off;
        proxy_read_timeout 3600s;
        client_max_body_size 0;    # ripristino del database da file
    }
}
```

Poi **Impostazioni → Generali → URL pubblico** con l'indirizzo per le app Jellyfin.

### Nomi dei file

Le stesse convenzioni di Jellyfin ed Emby: `Film/Titolo (Anno)/Titolo (Anno).mkv`, `Serie/Nome/Season 01/Nome S01E02.mkv`, episodi multipli `S01E01-E02`, episodi per data, numerazione assoluta per gli anime, identificativi espliciti `[tmdbid-603]`, `[imdbid-tt0133093]`, `[tvdbid-…]`. Sottotitoli esterni accanto al video: `Titolo (Anno).it.srt`, `.it.forced.srt`.

Metadati locali, letti una volta e mai scritti:

- `Titolo (Anno).nfo` o `movie.nfo` accanto al film, `tvshow.nfo` nella cartella della serie (formato Kodi). Con una chiave TMDB danno l'identificativo esatto; senza, i loro campi diventano i metadati.
- `.plexmatch` nella cartella del film o della serie (formato Plex: `title:`, `year:`, `tmdbid:`, `imdbid:`, `tvdbid:`, `guid: tmdb://…`). È la scelta esplicita dell'utente e vince sugli NFO.
- Nella cartella di una serie, righe `ep:` del `.plexmatch` per i file con nomi qualsiasi: `ep: S01E01: /Stagione 1/Il pilota.mkv`, `ep: S01E02-E03: …`, `ep: 2x04: …`, `ep: 12: …` (numerazione assoluta), `ep: 2023-05-01: …` (per data). Il percorso è relativo alla cartella della serie.
- `<episodio>.nfo` accanto al video di un episodio (`<episodedetails>`, anche più blocchi per i file con più episodi): titolo, trama, data e voto vincono su quelli di TMDB. Lo scanner nota l'NFO dall'elenco della cartella, senza aprire nulla.

## App compatibili

| Client | Piattaforme | Stato |
| --- | --- | --- |
| WebApp Platea | Qualunque browser: computer, tablet, telefono (installabile come app web) | OK |
| Infuse | iPhone, iPad, Apple TV, Mac (collegamento come server Jellyfin) | OK (provato su iPhone con Infuse 8.5, anche dal tunnel Cloudflare) |
| Swiftfin | iPhone, iPad, Apple TV (app Jellyfin ufficiale) | OK (provato su iPhone con Swiftfin 1.7, dal tunnel Cloudflare: accesso, home, Media, riproduzione) |
| App Android nativa | Telefoni, tablet, Android TV, Fire TV con Fire OS | TODO (fase 6) |
| App iOS nativa | iPhone, iPad, Mac | TODO (fase 6) |

Infuse e Swiftfin si collegano come a un server Jellyfin: indirizzo del server sulla porta 8096 (o il nome del tunnel che punta lì), utente e password di Platea. Platea si presenta come Jellyfin 12.1 ed espone la parte dell'API Jellyfin che serve alle app: login (anche Quick Connect), librerie, elenchi, dettagli, immagini, ricerca, stato di visione, stream del file originale con Range, rapporti di riproduzione, WebSocket. Infuse riproduce sempre il file originale; per le altre app Platea legge il profilo del dispositivo e, se serve, fa remux o transcodifica in HLS. Le riproduzioni compaiono in Panoramica.

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
