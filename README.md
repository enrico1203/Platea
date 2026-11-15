# Platea

Platea is an open source media server for movies and TV shows, written in Rust, that plays the original file whenever the client can and works well with libraries on cloud storage.

## Compatible apps

| App | Devices | Status |
| --- | --- | --- |
| Platea web app | Any browser on a computer, tablet or phone. Can be installed as a web app. | Works |
| [Infuse](https://firecore.com/infuse) | iPhone, iPad, Apple TV, Mac | Works (tested with Infuse 8.5 on iPhone) |
| [Swiftfin](https://github.com/jellyfin/Swiftfin) | iPhone, iPad, Apple TV | Works (tested with Swiftfin 1.7 on iPhone) |
| [Findroid](https://github.com/jarnedemeulemeester/findroid) | Android phones and tablets | Should work, not tested yet |
| [Jellyfin for Android TV](https://github.com/jellyfin/jellyfin-androidtv) | Android TV, Google TV, Fire TV | Should work, not tested yet |
| Official "Jellyfin" app | Phones, desktop | Does not work, see below |
| Native Platea apps | Android, Android TV, iOS | Planned |

Infuse and the Jellyfin apps connect to Platea as if it were a Jellyfin server: use the server address on port 8096 (or a hostname that points to it) and your Platea username and password. Platea reports itself as Jellyfin 12.1 and implements the part of the Jellyfin API these apps use. Infuse always plays the original file. For the other apps Platea reads the device profile and remuxes or transcodes to HLS when the app can't play a format.

The official "Jellyfin" app for phones and desktops doesn't draw its own screens. It loads the Jellyfin web interface from the server at `/web/`, which Platea doesn't include, so it stops with a 404 error. The Android TV app is a native app and is not affected.

## Quick start

You need Docker with the Compose plugin. Create a folder with this `docker-compose.yml`:

```yaml
services:
  server:
    image: ghcr.io/enrico1203/platea-server:latest
    container_name: platea-server
    restart: unless-stopped
    ports:
      - "8096:8096"
    environment:
      PLATEA_DATA_DIR: /data
      PLATEA_CACHE_DIR: /cache
      PLATEA_TRANSCODE_DIR: /transcode
      TZ: Europe/Rome
    volumes:
      - ./data:/data
      - ./cache:/cache
      - ./transcode:/transcode
      # Your media folders, always read-only. Add one line per folder.
      - /path/to/movies:/media/movies:ro
      - /path/to/shows:/media/shows:ro

  web:
    image: ghcr.io/enrico1203/platea-web:latest
    container_name: platea-web
    restart: unless-stopped
    depends_on:
      - server
    ports:
      - "8080:80"
    environment:
      PLATEA_SERVER_URL: http://server:8096
```

Then start it:

```sh
mkdir -p data cache transcode
sudo chown -R 1000:1000 data cache transcode   # the server runs as uid 1000, not root
docker compose up -d
docker compose logs server | grep -i codice    # the line with the first-run setup code
```

Open `http://<host>:8080`, enter the setup code and follow the wizard: administrator account, first library, TMDB key, GPU test.

The images support amd64 and arm64. `latest` is the latest release, `edge` is the latest commit on `main`, and you can also pin a version like `0.1.0`.

The [`docker-compose.yml`](docker-compose.yml) in this repository is more complete. It has the GPU settings, a Cloudflare tunnel and a `.env` file for ports and versions. To build the images from source, clone the repository and run `docker compose up -d --build`.

| Service | Port | Purpose |
| --- | --- | --- |
| `web` | 8080 | Web app (static files served by nginx, which forwards `/api` to the server) |
| `server` | 8096 | API, streaming, Infuse and Jellyfin apps |

| Folder | Contents | Safe to delete |
| --- | --- | --- |
| `./data` | Database and the installation secret | No, back it up |
| `./cache` | Resized images, thumbnails, extracted subtitles | Yes |
| `./transcode` | Segments of active playback sessions | Yes |

## Features

- Direct play first. The client declares what it can play and the server picks direct play, then remux, then transcoding, and records the reason.
- Light remux, HEVC included. A browser that decodes HEVC gets the video copied into fMP4 segments, and only the audio is converted.
- Hardware transcoding on Intel (QSV, VA-API), NVIDIA (NVENC) and AMD (VA-API), with HDR to SDR tone mapping. If the GPU fails, the session falls back to software.
- Nodes: other machines that transcode for the server (for example one with a GPU) or hold copies of library folders, with priorities, weights, rules like "4K only" and automatic failover.
- Fast scans. A scan only lists folders, several at a time, and opens no files when nothing changed. Thousands of movies on an rclone mount are listed in under a minute. New files on local disks show up within seconds.
- A missing mount never deletes anything. If the library root is empty or unreadable, the scan stops. If many files disappear at once, nothing is removed until you confirm.
- Metadata from TMDB, from Kodi `.nfo` files (episode NFOs included) and from Plex `.plexmatch` files. Media folders are never written to.
- Automatic collections, trailers from TMDB, local extras (`Extras/`, `Trailers/`, `Featurettes/`, files ending in `-trailer` and similar) and missing episodes shown in each season.
- Per-user settings: audio and subtitle language, default quality at home and away, autoplay of the next episode, home page layout, profile picture.
- Per-user limits: libraries, age rating, hidden or allowed genres, playback hours, downloads, remote access, account expiry date, lockout after failed logins.
- Quick Connect: sign in on a TV by confirming a 6-digit code from a device that is already signed in.
- Seek bar thumbnails, chapters, subtitles uploaded by an admin or downloaded from OpenSubtitles, shared playlists.
- Webhooks for Discord, ntfy or any URL: playback, new titles, sign-ins and mount problems.
- Everything is configured from the web dashboard. There are no config files to edit after the first start.

## Configuration

### GPU

Uncomment the matching lines in the `server` service of [`docker-compose.yml`](docker-compose.yml):

- Intel and AMD: `devices: - /dev/dri:/dev/dri` and `group_add` with the group that owns `/dev/dri/renderD128` on the host (`stat -c %g /dev/dri/renderD128`).
- NVIDIA: the `deploy` block with the NVIDIA runtime.

Then go to Administration › Transcoding › Test now. The server encodes and decodes one second of real video for each combination and shows what works.

### Nodes

A node is another machine running the same image with `platea node`. The server reaches it over the LAN or Tailscale and can give it two jobs, alone or together:

- Transcoding. The node gets the file from the server, transcodes it (on its GPU if it has one) and sends the segments back. Clients only ever talk to the server.
- File copies. The node holds a copy of some library folders (a real copy or a second rclone mount). Direct play, downloads and apps read the file from there, with load balancing and failover between copies. When the node also transcodes, it reads the file locally.

The node has no database and no settings of its own. Everything is set and checked from the server.

#### 1. Start the node

On the node machine, with Docker installed, create a folder and a `docker-compose.yml` like this one (or copy [`docker-compose.node.yml`](docker-compose.node.yml)):

```yaml
services:
  node:
    image: ghcr.io/enrico1203/platea-server:latest
    container_name: platea-node
    command: ["node"]
    restart: unless-stopped
    ports:
      - "8097:8097"
    environment:
      PLATEA_ROLE: node
      PLATEA_NODE_NAME: gpu-node
    volumes:
      - ./node-data:/data
      # Only for file copies, always read-only, same layout as the folder on the server:
      # - /mnt/movies:/media/movies:ro,rslave
    tmpfs:
      - /transcode:size=4g,uid=1000,gid=1000
      - /cache:size=256m,uid=1000,gid=1000
    # Intel or AMD GPU (group: stat -c %g /dev/dri/renderD128 on the host):
    # devices:
    #   - /dev/dri:/dev/dri
    # group_add:
    #   - "993"
```

Then start it and read its key:

```sh
mkdir node-data && sudo chown 1000:1000 node-data
docker compose up -d
docker compose exec node platea node key
```

The key is created on first start and kept in `node-data/node.key`. To choose it yourself, set `PLATEA_NODE_KEY` (at least 16 characters).

Keep port 8097 on the LAN or Tailscale, never open it to the Internet. To accept requests only from some networks, add for example `PLATEA_NODE_ALLOW: 100.64.0.0/10,192.168.1.0/24`. The node and the server must run the same Platea version.

#### 2. Add it to the server

In Administration › Nodes › Add node, enter a name, the node address (for example `100.64.0.2`, port 8097 is added for you) and the key. The card turns Online within a few seconds and shows the node GPU, load, memory and running jobs.

For a GPU node, press GPU test › Test now on its card. It encodes and decodes real video and shows what works. The GPU backend, device, limits and preset of the node are set in its Edit dialog and sent to the node automatically.

The node reads files from the server at the address it sees the server coming from, on port 8096. If that's not reachable (another port, a reverse proxy), set it in Settings › Nodes › Server address for nodes, for example `http://100.64.0.1:8096`.

#### 3. Folder copies (optional)

Mount the copies read-only under `/media` in the node, as in the commented line above. Then open the node's Edit dialog and, for each library folder, pick its copy with Browse. The server checks that a few files have the same size on both sides and shows the folder as verified. A copy that is missing or different is never used: the file comes from another copy. Scans always run on the server, so the server must still see every library.

#### 4. Load balancing

This server is listed with the nodes and has the same settings, so you can also decide that it only transcodes when the nodes can't.

- Priority: 1 is the first choice. Higher numbers are used only when every executor with a lower one is off, full or refuses the job.
- Weight: executors with the same priority share the work in proportion. 90 and 10 split sessions 90/10. 0 means never.
- Max at once: a limit on concurrent sessions.
- Accept only: source resolution (4K only, up to 1080p, custom), HDR or SDR, source codecs, libraries, transcodes and/or remuxes, only if the GPU can decode and encode the job.
- Drain: the node finishes current playbacks and takes no new ones, useful before maintenance.

Some setups:

| Goal | Node | This server |
| --- | --- | --- |
| GPU node for everything, server as backup | priority 1 | priority 2 |
| 90% on the node, 10% on the server | priority 1, weight 90 | priority 1, weight 10 |
| Only 4K movies on the node | priority 1, Accept only 4K | priority 1 |

Settings › Nodes also has: the balancing strategy (by weight, least loaded, in order), whether to prefer a node that has the file locally, whether the server transcodes when no one else can (when off, playback is refused), and direct delivery (direct play and downloads through a signed link to nodes that have a public address).

#### Failover

The server checks each node every 5 seconds. After 3 failed checks the node is offline and gets no work, and it comes back on its own. If a node dies during a movie, the next segment is produced by another executor from the same point, and the player keeps going. A transcode with HEVC or AV1 output can't switch encoder mid-stream: the session ends and the web player reopens it at the same position. If the server disappears, the node stops its transcodes after 60 seconds. Node outages show up in Activity and, with Problems enabled in Settings › Notifications, in your webhooks.

The full guide is in [docs/nodi.md](docs/nodi.md) (Italian).

### Cloud storage with rclone

Mount the storage on the host and add the mount point to the `server` volumes with `:ro,rslave` (`rslave` lets the container see a mount made after it started). Create the library with the Remote profile: it scans every 6 hours, probes one file at a time and lists folders in parallel.

Seeking in a remuxed movie doesn't download the file. The keyframe index comes from the MKV Cues or the MP4 `moov` box, which are a few hundred KB.

If the mount runs with `--rc` (for example `--rc --rc-addr :5572`), set its address in Settings › Scanning, for example `http://host.docker.internal:5572`. You also need `extra_hosts: ["host.docker.internal:host-gateway"]` on the `server` service. Before each scan the server asks rclone to refresh the folder listing.

To scan only the folder that changed, for example from Sonarr or Radarr:

```sh
curl -X POST http://<host>:8096/api/v1/scan/path \
  -H "X-Platea-Key: <api key>" -H "Content-Type: application/json" \
  -d '{"path": "/media/movies/New Movie (2026)"}'
```

API keys are created in Administration › System.

### Remote access with a Cloudflare tunnel

The repository compose file has a `cloudflared` service that stays off until you enable it. No router ports need to be opened.

1. In the Cloudflare dashboard go to Zero Trust › Networks › Tunnels, create a tunnel and copy its token.
2. In `.env` set `COMPOSE_PROFILES=tunnel` and `CLOUDFLARE_TUNNEL_TOKEN=<token>`, then run `docker compose up -d`.
3. Add two public hostnames to the tunnel: `platea.example.com` to `http://web:80`, and `media.example.com` to `http://server:8096` for Infuse and the Jellyfin apps.
4. In Settings › General › Public URL enter `https://media.example.com`.

Clients coming through the tunnel count as remote, so the remote access permission and the remote bitrate limit apply. Cloudflare blocks requests larger than 100 MB, so restore database backups from the local network.

### Reverse proxy

Put the proxy in front of the web app (port 8080), which already forwards `/api` to the server. Infuse and the Jellyfin apps need a second hostname pointing to port 8096. The proxy must pass WebSockets, must not buffer responses, needs long timeouts and should send `X-Forwarded-For`. If the proxy is not on a private network, add its address in Settings › Network › Trusted proxies.

Caddy:

```caddy
platea.example.com {
    reverse_proxy 127.0.0.1:8080 {
        flush_interval -1
    }
}
media.example.com {
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
        client_max_body_size 0;
    }
}
```

### Updating

```sh
docker compose pull
docker compose up -d
```

Database migrations run on start, after an automatic backup (`data/platea.db.bak-…`, the last 2 are kept). Upgrading from the first release to the current one in a single step has been tested. Very early versions ran as root. If the server stops after an update saying the data is not accessible, run `sudo chown -R 1000:1000 data cache transcode`.

### Backups

Every 24 hours the server copies the database to `./data/backups` and keeps the last 7. In Administration › System you can make a backup, download it, or restore one. After a restore the server restarts and keeps the previous database in `./data` as `platea.db.before-restore-…`.

### Forgotten password

```sh
docker compose exec server platea reset-password <username>
```

This prints a new password. Add a second argument to choose it yourself. The account is re-enabled and all its devices are signed out.

## File naming

Platea uses the same conventions as Jellyfin and Emby:

- Movies: `Movies/Title (Year)/Title (Year).mkv`
- Shows: `Shows/Name/Season 01/Name S01E02.mkv`, multi-episode files `S01E01-E02`, date-based episodes, absolute numbering for anime
- Explicit IDs in folder names: `[tmdbid-603]`, `[imdbid-tt0133093]`, `[tvdbid-…]`
- External subtitles next to the video: `Title (Year).en.srt`, `Title (Year).en.forced.srt`
- `.strm` files: a text file with an `http` or `https` address, named like a video (`Title (Year).strm`). Platea probes the stream, plays it through the server (so sources on your home network work from outside too) and can remux or transcode it. Local paths and other protocols in `.strm` files are refused.

Local metadata is read once and never written:

- `Title (Year).nfo` or `movie.nfo` next to a movie, `tvshow.nfo` in a show folder and `<episode>.nfo` next to an episode (Kodi format). With a TMDB key they give the exact ID. Without one, their fields become the metadata.
- `.plexmatch` in a movie or show folder (Plex format: `title:`, `year:`, `tmdbid:`, `imdbid:`, `tvdbid:`, `guid:`). It wins over NFO files. In a show folder, `ep:` lines map files with any name to episodes, for example `ep: S01E01: /Season 1/Pilot.mkv`.

## Languages

The web app is available in Italian and English. Translations live in `WebApp/src/i18n/`, one file per language, with keys checked by TypeScript. Adding a language means adding a file and registering it in `locales.ts`.

## Development

Everything runs in containers, so you don't need Rust, Node or FFmpeg on the host.

```sh
git clone https://github.com/enrico1203/Platea.git && cd Platea
cp .env.example .env
docker compose up -d --build                  # server on :8096, web app on :8080
scripts/cargo.sh test                         # server tests
scripts/nodes-test/run.sh                     # end-to-end test: server and two nodes in containers
docker run --rm -v "$PWD/WebApp:/app" -w /app node:22-alpine npm run typecheck
```

```
Server/       Rust workspace
  crates/platea-naming     file and folder name parsing
  crates/platea-playback   direct play, remux or transcode decision
  crates/platea-server     the server binary
WebApp/       React, Vite, TypeScript, TanStack Query, hls.js
scripts/      cargo in Docker, schema and API client generation, test tools
docs/         project plan and brand identity (Italian)
```

The API is described by an OpenAPI document at `GET /api/v1/openapi.json`. Authenticate with `Authorization: Bearer <token>` from `POST /api/v1/auth/login`. `scripts/generate-clients.sh` generates Kotlin and Swift clients from it.

## Contributing

Bug reports, ideas and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md). Please report security issues privately as described in [SECURITY.md](SECURITY.md).

## License

Platea is free software under the [GNU Affero General Public License v3.0](LICENSE). You can use, study, change and share it. If you change it and offer it to others over a network, you must also make your changed source code available.

## Related projects and thanks

Platea builds on, or works with, these projects. Their names belong to their owners and are used here only to describe compatibility. Platea is an independent project, it is not affiliated with or endorsed by any of them, and it doesn't use their logos.

| Project | Relation to Platea | License |
| --- | --- | --- |
| [Jellyfin](https://jellyfin.org) | The free media server that inspired Platea. Platea contains no Jellyfin code. It implements part of the public Jellyfin API on its own so that existing apps work. The name "Jellyfin" is reserved for the Jellyfin project and its official clients. | [GPL-2.0](https://github.com/jellyfin/jellyfin/blob/master/LICENSE) |
| [jellyfin-ffmpeg](https://github.com/jellyfin/jellyfin-ffmpeg) | The FFmpeg build shipped in the server image and run as a separate program for probing and transcoding | [LGPL-2.1+ with GPL parts](https://github.com/jellyfin/jellyfin-ffmpeg/blob/jellyfin/LICENSE.md), source in their repository |
| [Swiftfin](https://github.com/jellyfin/Swiftfin) | Jellyfin app for iPhone, iPad and Apple TV | [MPL-2.0](https://github.com/jellyfin/Swiftfin/blob/main/LICENSE.md) |
| [Infuse](https://firecore.com/infuse) | Video player by Firecore for Apple devices | Proprietary (Firecore, LLC) |
| [Findroid](https://github.com/jarnedemeulemeester/findroid) | Jellyfin app for Android | [GPL-3.0](https://github.com/jarnedemeulemeester/findroid/blob/main/LICENSE) |
| [Jellyfin for Android TV](https://github.com/jellyfin/jellyfin-androidtv) | Jellyfin app for Android TV and Fire TV | [GPL-2.0](https://github.com/jellyfin/jellyfin-androidtv/blob/master/LICENSE) |
| [The Movie Database (TMDB)](https://www.themoviedb.org) | Metadata and images. This product uses the TMDB API but is not endorsed or certified by TMDB. The attribution with the TMDB logo is on the About page of the web app. | [API terms](https://www.themoviedb.org/api-terms-of-use) |
| [rclone](https://rclone.org) | Mounting cloud storage, and the remote control used by scans | [MIT](https://github.com/rclone/rclone/blob/master/COPYING) |
| [Kodi](https://kodi.tv), [Plex](https://www.plex.tv) | The `.nfo` and `.plexmatch` file formats | |

The server is built on Rust libraries such as [tokio](https://tokio.rs), [axum](https://github.com/tokio-rs/axum), [sqlx](https://github.com/launchbadge/sqlx) and [SQLite](https://sqlite.org). The web app uses [React](https://react.dev), [Vite](https://vite.dev), [TanStack Query](https://tanstack.com/query), [hls.js](https://github.com/video-dev/hls.js) and [Lucide](https://lucide.dev). Their licenses are included in their packages. Thanks to everyone who maintains them.
