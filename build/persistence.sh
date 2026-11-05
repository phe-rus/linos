#!/usr/bin/env bash
set -Eeuo pipefail

# Exercise actual disk writes across two QEMU boots without touching the
# original image or any host storage device.
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-$(dpkg --print-architecture)}"
case "$ARCH" in
    amd64|arm64) ;;
    *) printf 'Unsupported architecture: %s\n' "$ARCH" >&2; exit 2 ;;
esac

BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' packages/release/usr/lib/linos/release)"
if [[ ! "$BUILD_ID" =~ ^[a-zA-Z0-9._-]+$ ]]; then
    echo "Invalid build identifier" >&2
    exit 2
fi

SOURCE="images/server/builds/$ARCH/$BUILD_ID/disk.ext4"
if [[ ! -f "$SOURCE" ]]; then
    printf 'Missing disk image: %s\n' "$SOURCE" >&2
    exit 2
fi

TEST_DIR="$(mktemp -d /tmp/linos-persistence.XXXXXXXX)"
cleanup() {
    local status=$?
    if (( status != 0 )); then
        cp -- "$TEST_DIR"/*.log "images/server/builds/$ARCH/$BUILD_ID/" 2>/dev/null || true
    fi
    rm -r -- "$TEST_DIR"
}
trap cleanup EXIT
TEST_DISK="$TEST_DIR/disk.ext4"
cp --reflink=auto --sparse=always -- "$SOURCE" "$TEST_DISK"

read_identity() {
    local record
    record="$(debugfs -R 'cat /var/lib/linos/device-id' "$TEST_DISK" 2>"$TEST_DIR/debugfs.log")"
    if [[ ! "$record" =~ ^[[:xdigit:]]{8}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{12}$ ]]; then
        echo "Invalid or missing persistent device identity" >&2
        cat "$TEST_DIR/debugfs.log" >&2
        return 1
    fi
    printf '%s\n' "$record"
}

for iteration in 1 2; do
    echo "Persistent boot $iteration/2 ($ARCH)"
    LINOS_SMOKE_DISK="$TEST_DISK" \
        LINOS_SMOKE_LOG_PREFIX="$TEST_DIR/boot-$iteration" \
        LINOS_SMOKE_WRITE_DISK=1 \
        ./build/smoke.sh "$ARCH"
    # QEMU is stopped like a sudden power loss. Replay the ext4 journal
    # before inspecting the filesystem without mounting it.
    fsck_status=0
    e2fsck -fy "$TEST_DISK" >"$TEST_DIR/fsck-$iteration.log" 2>&1 || fsck_status=$?
    if (( fsck_status > 1 )); then
        echo "Filesystem recovery failed with code $fsck_status" >&2
        cat "$TEST_DIR/fsck-$iteration.log" >&2
        exit 1
    fi
    identity="$(read_identity)"
    if [[ "$iteration" == 1 ]]; then
        first_identity="$identity"
    elif [[ "$identity" != "$first_identity" ]]; then
        echo "FAIL: device identifier changed after reboot" >&2
        exit 1
    fi
done

echo "PASS: Linos device identity persisted unchanged across two QEMU boots"
