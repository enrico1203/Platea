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
    apt-get install -y --no-install-recommends jellyfin-ffmpeg8; \
    apt-get purge -y curl gnupg; apt-get autoremove -y; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/*; \
    /usr/lib/jellyfin-ffmpeg/ffmpeg -version | head -1

COPY --from=build /usr/local/bin/platea /usr/local/bin/platea

ENV PLATEA_BIND=0.0.0.0:8096 \
    PLATEA_DATA_DIR=/data \
    PLATEA_CACHE_DIR=/cache \
    PLATEA_TRANSCODE_DIR=/transcode \
    PLATEA_LOG=info
VOLUME ["/data", "/cache", "/transcode"]
EXPOSE 8096
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["/usr/local/bin/platea", "--healthcheck"]
ENTRYPOINT ["/usr/local/bin/platea"]
