#!/usr/bin/env bash
# Genera i client dell'API (Kotlin per Android, Swift per iOS) dal documento OpenAPI di un server in funzione.
# Uso: scripts/generate-clients.sh [indirizzo del server]   (predefinito http://localhost:8096)
# Uscita in clients/kotlin e clients/swift (non versionati). Gli schemi vengono da scripts/openapi-schemas.sh.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SERVER="${1:-http://localhost:8096}"
mkdir -p "$ROOT/clients"
curl -fsS "$SERVER/api/v1/openapi.json" -o "$ROOT/clients/openapi.json"
GEN="openapitools/openapi-generator-cli:v7.12.0"
docker run --rm -v "$ROOT/clients:/c" "$GEN" validate -i /c/openapi.json
docker run --rm -v "$ROOT/clients:/c" "$GEN" generate -i /c/openapi.json -g kotlin -o /c/kotlin \
  --additional-properties=packageName=app.platea.client,groupId=app.platea,artifactId=platea-client,serializationLibrary=kotlinx_serialization >/dev/null
docker run --rm -v "$ROOT/clients:/c" "$GEN" generate -i /c/openapi.json -g swift5 -o /c/swift \
  --additional-properties=projectName=PlateaClient,responseAs=AsyncAwait >/dev/null
echo "client generati in clients/kotlin e clients/swift"
