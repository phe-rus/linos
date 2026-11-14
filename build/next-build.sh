#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-${LINOS_ARCH:-$(dpkg --print-architecture)}}"
case "$ARCH" in
    amd64|arm64) ;;
    *) printf 'Unsupported Linos architecture: %s\n' "$ARCH" >&2; exit 2 ;;
esac

RELEASE="packages/release/usr/lib/linos/release"
[[ -f "$RELEASE" ]] || { echo "Missing release metadata: $RELEASE" >&2; exit 1; }

DATE="$(date +%Y%m%d)"
[[ "$DATE" =~ ^[0-9]{8}$ ]] || { echo "Invalid system date" >&2; exit 1; }

# Preserve old builds. An ID must be unused for both architectures to avoid
# collisions with CI or cross-device development.
number=1
while :; do
    BUILD_ID="$DATE.$number"
    if [[ ! -e "images/server/builds/amd64/$BUILD_ID" &&
          ! -e "images/server/builds/arm64/$BUILD_ID" &&
          ! -e "images/recovery/builds/amd64/$BUILD_ID" &&
          ! -e "images/recovery/builds/arm64/$BUILD_ID" ]]; then
        break
    fi
    ((number += 1))
done

current="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$RELEASE")"
[[ -n "$current" ]] || { echo "Missing BUILD_ID in $RELEASE" >&2; exit 1; }

# GNU sed and date are already part of the Debian development environment.
sed -i "s/^BUILD_ID=\"[^\"]*\"$/BUILD_ID=\"$BUILD_ID\"/" "$RELEASE"
printf 'Linos 13 %s build: %s\n' "$ARCH" "$BUILD_ID"
printf 'Next: ./build/package.sh && ./build/image.sh && ./build/smoke.sh %s\n' "$ARCH"
