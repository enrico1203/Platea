# syntax=docker/dockerfile:1.7
# Platea server: build Rust in un'immagine completa, esecuzione in un'immagine minima
# con jellyfin-ffmpeg (driver Intel, AMD e NVIDIA inclusi).

FROM rust:1-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p platea-server --locked \
    && cp target/release/platea /usr/local/bin/platea

FROM debian:trixie-slim AS runtime
ARG TARGETARCH
# jellyfin-ffmpeg dal repository ufficiale di Jellyfin, senza pacchetti consigliati.
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl gnupg; \
    curl -fsSL https://repo.jellyfin.org/jellyfin_team.gpg.key | gpg --dearmor -o /usr/share/keyrings/jellyfin.gpg; \
    echo "deb [signed-by=/usr/share/keyrings/jellyfin.gpg] https://repo.jellyfin.org/debian trixie main" > /etc/apt/sources.list.d/jellyfin.list; \
    apt-get update; \
    # Correzioni di sicurezza di Debian uscite dopo l'immagine base (controllate da Trivy in CI).
    apt-get upgrade -y --no-install-recommends; \
    apt-get install -y --no-install-recommends jellyfin-ffmpeg8; \
    apt-get purge -y curl gnupg; apt-get autoremove -y; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/*; \
    /usr/lib/jellyfin-ffmpeg/ffmpeg -version | head -1

COPY --from=build /usr/local/bin/platea /usr/local/bin/platea

# Il server non gira come root. uid/gid 1000: le cartelle data, cache e transcode montate dall'host devono
# appartenere a questo utente (sudo chown -R 1000:1000 data cache transcode). Gruppo video per le GPU;
# per /dev/dri/renderD* serve anche il gruppo "render" dell'host (group_add nel compose, vedi README).
RUN groupadd -g 1000 platea \
 && useradd -u 1000 -g 1000 -G video -M -d /data -s /usr/sbin/nologin platea \
 && mkdir -p /data /cache /transcode \
 && chown platea:platea /data /cache /transcode
USER platea

ENV PLATEA_BIND=0.0.0.0:8096 \
    PLATEA_DATA_DIR=/data \
    PLATEA_CACHE_DIR=/cache \
    PLATEA_TRANSCODE_DIR=/transcode \
    PLATEA_LOG=info
VOLUME ["/data", "/cache", "/transcode"]
EXPOSE 8096
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["/usr/local/bin/platea", "--healthcheck"]
ENTRYPOINT ["/usr/local/bin/platea"]
