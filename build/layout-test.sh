#!/usr/bin/env bash
set -Eeuo pipefail

# Test the proposed Linos Server GPT layout exclusively on a disposable sparse
# file. Never accept a device path or caller-supplied output target.
export PATH="/usr/sbin:/sbin:$PATH"

if [[ $# -ne 0 ]]; then
    echo "Usage: ./build/layout-test.sh (no arguments accepted)" >&2
    exit 2
fi

for utility in sgdisk truncate stat; do
    command -v "$utility" >/dev/null || { echo "Missing $utility" >&2; exit 2; }
done

WORK="$(mktemp -d /tmp/linos-layout-test.XXXXXXXX)"
trap 'rm -r -- "$WORK"' EXIT
DISK="$WORK/server-layout.raw"
truncate -s 16G "$DISK"

sgdisk --clear \
    --new=1:2048:+512M --typecode=1:ef00 --change-name=1:linos-efi \
    --new=2:0:+2G --typecode=2:8300 --change-name=2:linos-recovery \
    --new=3:0:+4G --typecode=3:8300 --change-name=3:linos-system-a \
    --new=4:0:+4G --typecode=4:8300 --change-name=4:linos-system-b \
    --new=5:0:0 --typecode=5:8300 --change-name=5:linos-data \
    "$DISK" >/dev/null

sgdisk --verify "$DISK"
for item in \
    '1:linos-efi' \
    '2:linos-recovery' \
    '3:linos-system-a' \
    '4:linos-system-b' \
    '5:linos-data'; do
    number="${item%%:*}"
    expected="${item#*:}"
    info="$(sgdisk --info="$number" "$DISK")"
    grep -q "Partition name: '$expected'" <<<"$info" || {
        echo "Partition $number has the wrong name" >&2
        exit 1
    }
done

echo "PASS: GPT geometry, EFI, Recovery, A/B System and Data verified on sparse image"
echo "No physical block device or existing disk was accessed."
