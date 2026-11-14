#!/usr/bin/env bash
# Esegue cargo dentro un container Rust: nessuna toolchain sull'host.
# Uso: scripts/cargo.sh test -p platea-naming
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
docker volume create platea-cargo-registry >/dev/null
exec docker run --rm -i \
  -v "$ROOT/Server:/src" -w /src \
  -v platea-cargo-registry:/usr/local/cargo/registry \
  -e CARGO_TERM_COLOR=never \
  --network host \
  ${PLATEA_DEV_EXTRA:-} \
  rust:1-trixie cargo "$@"
