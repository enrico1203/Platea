#!/usr/bin/env bash
# Prova del "mount bloccato": il server deve restare reattivo quando lo storage di una libreria non risponde.
# Monta con rclone (sull'host) una cartella locale dentro ./media con banda limitata, crea una libreria,
# poi congela rclone con SIGSTOP e misura l'API mentre si scansiona e si riproduce da quel mount.
# Uso: scripts/stuck-mount-test.sh <cartella-con-un-film> <utente> <password>
# Richiede: rclone e fuse sull'host, stack del compose avviato. Tocca solo il proprio mount di prova.
set -euo pipefail
SRC=${1:?cartella sorgente}; USER_=${2:-admin}; PASS=${3:?password}
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; MNT="$ROOT/media/stuck-test"; B=http://127.0.0.1:8096
mkdir -p "$MNT"
rclone mount "$SRC" "$MNT" --read-only --bwlimit 300k --vfs-cache-mode off --allow-other --daemon
PID=$(pgrep -f "rclone mount $SRC $MNT")
cleanup() { kill -CONT "$PID" 2>/dev/null || true; fusermount -u "$MNT" 2>/dev/null || umount "$MNT" || true; rmdir "$MNT" 2>/dev/null || true; }
trap cleanup EXIT
TOK=$(curl -s -X POST $B/api/v1/auth/login -H 'content-type: application/json' -d "{\"username\":\"$USER_\",\"password\":\"$PASS\"}" | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
H="authorization: Bearer $TOK"
LID=$(curl -s -X POST $B/api/v1/libraries -H "$H" -H 'content-type: application/json' \
  -d '{"name":"Prova mount bloccato","kind":"movies","paths":["/media/stuck-test"],"scan_profile":"local","scan_now":true}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
sleep 15
echo "Congelo rclone ($PID)"; kill -STOP "$PID"
curl -s -X POST "$B/api/v1/libraries/$LID/scan" -H "$H" >/dev/null
max=0; for _ in $(seq 1 60); do
  t=$(curl -s -o /dev/null -w "%{time_total}" -m 20 "$B/api/v1/home" -H "$H"); ms=$(python3 -c "print(int(float('$t')*1000))")
  [ "$ms" -gt "$max" ] && max=$ms; sleep 0.5
done
echo "Latenza massima dell'API con il mount bloccato: $max ms"
curl -s "$B/api/v1/libraries/$LID" -H "$H" | python3 -c 'import sys,json;d=json.load(sys.stdin);print("Scansione:",d["last_scan_status"],"-",d["last_scan_message"])'
kill -CONT "$PID"
curl -s -o /dev/null -X DELETE "$B/api/v1/libraries/$LID" -H "$H"
