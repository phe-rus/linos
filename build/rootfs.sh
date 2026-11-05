#!/usr/bin/env bash
set -Eeuo pipefail

PROJECT_ROOT="$(
    cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." &&
    pwd
)"

ARCH="${linos13_ARCH:-$(dpkg --print-architecture)}"
SUITE="${linos13_SUITE:-trixie}"

OUTPUT_DIR="$PROJECT_ROOT/images/minimal"
OUTPUT="$OUTPUT_DIR/linos13-rootfs-${ARCH}.tar"

PACKAGES=(
    systemd-sysv
    dbus
    iproute2
    procps
    ca-certificates
    openssh-server
)

if [[ -e "$OUTPUT" ]]; then
    printf 'Existing image: %s\n' "$OUTPUT"
    printf 'Move it aside before rebuilding.\n'
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

PACKAGE_LIST="$(
    IFS=,
    printf '%s' "${PACKAGES[*]}"
)"

printf 'linos13 Linux Build\n'
printf 'Architecture: %s\n' "$ARCH"
printf 'Debian suite: %s\n' "$SUITE"
printf 'Output: %s\n' "$OUTPUT"

mmdebstrap \
    --variant=minbase \
    --architectures="$ARCH" \
    --include="$PACKAGE_LIST" \
    "$SUITE" \
    "$OUTPUT"

printf '\nBuild completed successfully.\n'
sha256sum "$OUTPUT"