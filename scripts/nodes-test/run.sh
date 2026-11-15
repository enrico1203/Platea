#!/usr/bin/env bash
# Prova end-to-end dei nodi: costruisce l'immagine e lancia server + 2 nodi in container separati.
# Uso: scripts/nodes-test/run.sh [cartella di lavoro] [opzioni di nodes_test.py, es. --skip-orphan --keep]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="${1:-$(mktemp -d)}"
shift || true
docker build -q -t platea-server:nodes-dev "$ROOT/Server" >/dev/null
exec python3 -u "$ROOT/scripts/nodes-test/nodes_test.py" --image platea-server:nodes-dev --work "$WORK" "$@"
