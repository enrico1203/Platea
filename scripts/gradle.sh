#!/usr/bin/env bash
# Esegue Gradle per l'app Android dentro un container (JDK 17 + Android SDK): nessuna toolchain sull'host.
# Uso: scripts/gradle.sh assembleDebug · scripts/gradle.sh assembleRelease · scripts/gradle.sh test
# L'APK finisce in Android/app/build/outputs/apk/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE=platea-android-build
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  docker build -t "$IMAGE" "$ROOT/Android/docker"
fi
docker volume create platea-gradle-cache >/dev/null
exec docker run --rm -i \
  -v "$ROOT/Android:/src" -w /src \
  -v platea-gradle-cache:/home/builder/.gradle \
  -e GRADLE_USER_HOME=/home/builder/.gradle \
  --user "$(id -u):$(id -g)" \
  --network host \
  "$IMAGE" sh -c 'if [ -x ./gradlew ]; then exec ./gradlew --no-daemon "$@"; else exec gradle --no-daemon "$@"; fi' gradle "$@"
