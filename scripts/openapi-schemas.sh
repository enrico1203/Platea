#!/usr/bin/env bash
# Rigenera gli schemi dei dati del documento OpenAPI dai tipi TypeScript della web UI
# (WebApp/src/api/types.ts e requests.ts), in Server/crates/platea-server/src/api/openapi_schemas.json.
# Va rilanciato quando cambiano quei tipi; la CI controlla che il file sia aggiornato.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
docker run --rm -v "$ROOT:/repo" -w /tmp node:22-alpine sh -c '
  npm i -s --no-audit --no-fund ts-json-schema-generator@2.4.0 >/dev/null 2>&1
  cd /repo/WebApp
  /tmp/node_modules/.bin/ts-json-schema-generator --path "src/api/{types,requests}.ts" --tsconfig tsconfig.json \
    --type "*" --expose export --no-type-check --additional-properties -o /tmp/raw.json
  node /repo/scripts/openapi-schemas.mjs /tmp/raw.json /repo/Server/crates/platea-server/src/api/openapi_schemas.json
'
echo "schemi aggiornati: Server/crates/platea-server/src/api/openapi_schemas.json"
