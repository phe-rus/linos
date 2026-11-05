#!/usr/bin/env bash
set -Eeuo pipefail

# Test the real console enrollment flow on a private copy of a server image.
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

BUILD_DIR="images/server/builds/$ARCH/$BUILD_ID"
for path in "$BUILD_DIR/disk.ext4" "$BUILD_DIR/boot/kernel" "$BUILD_DIR/boot/initramfs"; do
    [[ -f "$path" ]] || { printf 'Missing image artifact: %s\n' "$path" >&2; exit 2; }
done

TEST_DIR="$(mktemp -d /tmp/linos-enrollment.XXXXXXXX)"
cleanup() {
    local status=$?
    if (( status != 0 )); then
        for log in "$TEST_DIR"/*.log; do
            [[ -f "$log" ]] && cp -- "$log" "$BUILD_DIR/enrollment-$(basename "$log")"
        done
    fi
    rm -r -- "$TEST_DIR"
}
trap cleanup EXIT

TEST_DISK="$TEST_DIR/disk.ext4"
cp --reflink=auto --sparse=always -- "$BUILD_DIR/disk.ext4" "$TEST_DISK"

export LINOS_ARCH="$ARCH"
export LINOS_ENROLL_DISK="$TEST_DISK"
export LINOS_ENROLL_KERNEL="$ROOT/$BUILD_DIR/boot/kernel"
export LINOS_ENROLL_INITRAMFS="$ROOT/$BUILD_DIR/boot/initramfs"
# Ephemeral, test-only password. Never written into the source or disk image.
export LINOS_ENROLL_PASSWORD="$(tr -d '-' </proc/sys/kernel/random/uuid)"

read_guest_file() {
    debugfs -R "cat $1" "$TEST_DISK" 2>"$TEST_DIR/debugfs.log"
}

recover_journal() {
    local status=0
    e2fsck -fy "$TEST_DISK" >"$TEST_DIR/fsck-$1.log" 2>&1 || status=$?
    if (( status > 1 )); then
        cat "$TEST_DIR/fsck-$1.log" >&2
        echo "Filesystem recovery failed after $1" >&2
        exit 1
    fi
}

run_phase() {
    export LINOS_ENROLL_PHASE="$1"
    export LINOS_ENROLL_LOG="$TEST_DIR/boot-$1.log"
    echo "Enrollment phase: $1 ($ARCH)"
    timeout --signal=TERM --kill-after=5s 200s expect ./build/enrollment.exp
    recover_journal "$1"
}

# Interrupt immediately after account creation, before a password is set.
run_phase interrupt
[[ "$(read_guest_file /var/lib/linos/pending-owner)" == linosadmin ]] || {
    echo "Pending owner was not retained after interruption" >&2; exit 1;
}
if read_guest_file /var/lib/linos/owner 2>/dev/null | grep -q '^username='; then
    echo "Owner was committed before a password was set" >&2
    exit 1
fi
shadow="$(read_guest_file /etc/shadow)"
locked="$(printf '%s\n' "$shadow" | awk -F: '$1 == "linosadmin" {print $2}')"
[[ "$locked" == '!'* || "$locked" == '*'* ]] || {
    echo "Interrupted administrator does not have a locked password" >&2; exit 1;
}

# Resume the same pending account, complete password setup, and commit owner.
run_phase complete
expected_id="$(read_guest_file /var/lib/linos/device-id)"
owner="$(read_guest_file /var/lib/linos/owner)"
[[ "$owner" == "$(printf 'username=linosadmin\ndevice-id=%s' "$expected_id")" ]] || {
    echo "Enrolled owner record does not match the device" >&2; exit 1;
}
[[ "$(read_guest_file /var/lib/linos/provisioning-state)" == provisioned ]] || {
    echo "Provisioned state was not committed" >&2; exit 1;
}
shadow="$(read_guest_file /etc/shadow)"
hash="$(printf '%s\n' "$shadow" | awk -F: '$1 == "linosadmin" {print $2}')"
[[ -n "$hash" && "$hash" != '!'* && "$hash" != '*'* ]] || {
    echo "Enrolled administrator does not have a password hash" >&2; exit 1;
}

# A later boot must recognize the owner without offering a new claim.
run_phase verify
[[ "$(read_guest_file /var/lib/linos/owner)" == "$owner" ]] || {
    echo "Owner record changed on the next boot" >&2; exit 1;
}

echo "PASS: interrupted enrollment resumed, account secured, owner persisted ($ARCH)"
