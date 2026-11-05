#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-$(dpkg --print-architecture)}"
case "$ARCH" in amd64|arm64) ;; *) echo "Unsupported payload architecture" >&2; exit 2 ;; esac
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' packages/release/usr/lib/linos/release)"
[[ "$BUILD_ID" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "Invalid build ID" >&2; exit 2; }
BUILD_DIR="images/server/builds/$ARCH/$BUILD_ID"
OUTPUT="images/install/payloads/$ARCH/$BUILD_ID"

for file in "$BUILD_DIR/rootfs.tar" "$BUILD_DIR/boot/kernel" "$BUILD_DIR/boot/initramfs"; do
    [[ -s "$file" ]] || { echo "Missing server image artifact: $file" >&2; exit 2; }
done
[[ ! -e "$OUTPUT" ]] || { echo "Payload already exists: $OUTPUT" >&2; exit 1; }
mkdir -p "$OUTPUT"

gzip -n -1 -c "$BUILD_DIR/rootfs.tar" > "$OUTPUT/rootfs.tar.gz"
install -m 644 "$BUILD_DIR/boot/kernel" "$OUTPUT/kernel"
install -m 644 "$BUILD_DIR/boot/initramfs" "$OUTPUT/initramfs"
printf '%s\n' \
    'format=1' \
    'edition=server' \
    "architecture=$ARCH" \
    "build=$BUILD_ID" \
    'base=debian-trixie' \
    > "$OUTPUT/manifest"

(
    cd "$OUTPUT"
    sha256sum rootfs.tar.gz kernel initramfs manifest > SHA256SUMS
    sha256sum -c SHA256SUMS
)
echo "Offline development payload built: $OUTPUT"
echo "This checksum manifest does not provide cryptographic authenticity."
