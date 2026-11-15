# Come contribuire a Platea

Grazie dell'interesse! Platea è un progetto aperto: segnalazioni, idee, traduzioni e codice sono benvenuti.

## Segnalare un problema o proporre un'idea

- Apri una [issue](../../issues) e cerca prima se esiste già.
- Per un problema indica: versione (Amministrazione › Sistema o `GET /api/v1/system/info`), come avvii Platea
  (Docker, GPU, storage locale o cloud), il client (browser, Infuse, Swiftfin…), cosa ti aspettavi e cosa è successo,
  e le righe utili del log (`docker compose logs server`). Togli dai log token, indirizzi e nomi che non vuoi rendere pubblici.
- Le vulnerabilità di sicurezza **non** vanno in una issue pubblica: vedi [SECURITY.md](SECURITY.md).
- Per un'app Jellyfin che non funziona con Platea, avvia il server con `PLATEA_LOG=info,tower_http=debug` e allega
  le richieste che falliscono ("percorso non gestito", stato 4xx/5xx).

## Preparare l'ambiente

Tutto gira in container: servono solo Docker e il plugin Compose.

```sh
git clone https://github.com/enrico1203/Platea.git && cd Platea
cp .env.example .env
docker compose up -d --build                 # server su :8096, web UI su :8080
scripts/cargo.sh test                        # test del server (Rust in un container)
docker run --rm -v "$PWD/WebApp:/app" -w /app node:22-alpine npm run typecheck   # tipi della web UI
```

## Prima di una pull request

Gli stessi controlli della CI (`.github/workflows/ci.yml`):

```sh
scripts/cargo.sh fmt --all
scripts/cargo.sh clippy --all-targets --locked -- -D warnings
scripts/cargo.sh test --locked
docker run --rm -v "$PWD/WebApp:/app" -w /app node:22-alpine sh -c "npm ci && npm run typecheck && npm run build"
```

- Una pull request per argomento, piccola quando si può, con una descrizione di cosa cambia e di come l'hai provata.
- Aggiungi test per la logica nuova (nomi dei file, decisione di riproduzione, parser, permessi).
- Se cambi i tipi in `WebApp/src/api/types.ts` o `requests.ts`, rigenera gli schemi dell'API: `scripts/openapi-schemas.sh`.
- Se cambi lo schema del database aggiungi una migrazione nuova in `Server/crates/platea-server/migrations/`:
  quelle già pubblicate non si modificano.

## Regole del progetto

- **Mai scrivere nelle cartelle dei media.** Immagini, metadati e sottotitoli aggiunti stanno nel database o nella cache.
- **Il server non si deve bloccare.** Niente I/O bloccante sul runtime async, tempi limite su ogni operazione su file
  e rete, semafori sulle risorse scarse.
- **Interfaccia** (`docs/brand-identity.md`): su iOS nessun campo deve far scattare lo zoom (testo dei campi almeno
  16px su touch); niente `maximum-scale` né `user-scalable=no`.
- **Testi:** ogni testo della web UI passa dalle traduzioni (`WebApp/src/i18n/`), in italiano e inglese.
  Per aggiungere una lingua basta un file nuovo registrato in `locales.ts`.
- Il codice e i commenti sono in italiano; issue e pull request anche in inglese.

## Licenza dei contributi

Inviando un contributo accetti che sia distribuito con la licenza del progetto,
[AGPL-3.0-only](LICENSE).
