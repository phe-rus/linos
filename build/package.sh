#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CONTROL="packages/release/DEBIAN/control"
RELEASE="packages/release/usr/lib/linos/release"

PACKAGE_VERSION="$(awk '/^Version:/ { print $2 }' "$CONTROL")"
PRODUCT_VERSION="$(sed -n 's/^VERSION_ID="\([^"]*\)"$/\1/p' "$RELEASE")"
BUILD_CHANNEL="$(sed -n 's/^BUILD_CHANNEL=\(.*\)$/\1/p' "$RELEASE")"

case "$BUILD_CHANNEL" in
    development|dev) CHANNEL=dev ;;
    alpha|beta|release) CHANNEL="$BUILD_CHANNEL" ;;
    *) echo "Unsupported Linos build channel: $BUILD_CHANNEL" >&2; exit 2 ;;
esac

if [[ ! "$PRODUCT_VERSION" =~ ^[0-9]+$ ]] ||
   [[ ! "$PACKAGE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Invalid release or package version" >&2
    exit 2
fi

OUTPUT="packages/dist/linos${PRODUCT_VERSION}_${CHANNEL}_v${PACKAGE_VERSION}.deb"

if [[ "${1:-}" == "--print-path" ]]; then
    printf '%s\n' "$OUTPUT"
    exit 0
fi

if [[ $# -ne 0 ]]; then
    echo "Usage: ./build/package.sh [--print-path]" >&2
    exit 2
fi

mkdir -p packages/dist
dpkg-deb --root-owner-group --build packages/release "$OUTPUT"
