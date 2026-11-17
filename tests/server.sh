#!/usr/bin/env bash
set -Eeuo pipefail

smoke() (
set -Eeuo pipefail

export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-$(dpkg --print-architecture)}"
RELEASE_FILE="system/release/usr/lib/linos/release"
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$RELEASE_FILE")"

case "$ARCH" in
    amd64|arm64) ;;
    *) printf 'Unsupported smoke-test architecture: %s\n' "$ARCH" >&2; exit 2 ;;
esac

if [[ ! "$BUILD_ID" =~ ^[a-zA-Z0-9._-]+$ ]]; then
    echo "Invalid or missing build identifier" >&2
    exit 2
fi

BUILD_DIR="images/server/builds/$ARCH/$BUILD_ID"
KERNEL="$BUILD_DIR/boot/kernel"
INITRAMFS="$BUILD_DIR/boot/initramfs"
DISK="${LINOS_SMOKE_DISK:-$BUILD_DIR/disk.ext4}"
LOG="$BUILD_DIR/smoke.log"
QEMU_LOG="$BUILD_DIR/qemu.log"
if [[ -n "${LINOS_SMOKE_LOG_PREFIX:-}" ]]; then
    LOG="${LINOS_SMOKE_LOG_PREFIX}.smoke.log"
    QEMU_LOG="${LINOS_SMOKE_LOG_PREFIX}.qemu.log"
fi

for path in "$KERNEL" "$INITRAMFS" "$DISK"; do
    if [[ ! -f "$path" ]]; then
        printf 'Missing build artifact: %s\n' "$path" >&2
        exit 2
    fi
done

if ! dumpe2fs -h "$DISK" >/dev/null 2>&1; then
    echo "Disk image is not a formatted ext filesystem: $DISK" >&2
    echo "Complete ./build/image.sh before running the smoke test." >&2
    exit 2
fi

if [[ "$ARCH" == amd64 ]]; then
    COMMAND=(
        qemu-system-x86_64
        -machine pc,accel=tcg -cpu qemu64 -smp 2 -m 2048
        -display none -no-reboot -monitor none
        -chardev "file,id=linos_serial,path=$LOG" -serial chardev:linos_serial
        -kernel "$KERNEL" -initrd "$INITRAMFS"
        -drive "file=$DISK,if=none,format=raw,id=rootfs"
        -device virtio-blk-pci,drive=rootfs
        -append "root=/dev/vda rootwait rw console=ttyS0,115200n8 quiet loglevel=3 systemd.show_status=yes rd.systemd.show_status=yes systemd.mask=linos-enroll.service systemd.unit=multi-user.target"
    )
else
    COMMAND=(
        qemu-system-aarch64
        -machine virt -cpu cortex-a76 -smp 2 -m 2048
        -display none -no-reboot -monitor none
        -chardev "file,id=linos_serial,path=$LOG" -serial chardev:linos_serial
        -kernel "$KERNEL" -initrd "$INITRAMFS"
        -drive "file=$DISK,if=none,format=raw,id=rootfs"
        -device virtio-blk-device,drive=rootfs
        -append "root=/dev/vda rootwait rw console=ttyAMA0,115200n8 quiet loglevel=3 systemd.show_status=yes rd.systemd.show_status=yes systemd.mask=linos-enroll.service systemd.unit=multi-user.target"
    )
fi

SNAPSHOT_ARGS=(-snapshot)
if [[ "${LINOS_SMOKE_WRITE_DISK:-0}" == 1 ]]; then
    SNAPSHOT_ARGS=()
fi

echo "Boot-testing Linos 13 ($ARCH), build $BUILD_ID"
echo "Serial log: $LOG"
: > "$LOG"
# Stop QEMU once we observe both boot signals, retaining the 180-second guard.
timeout --signal=TERM --kill-after=5s 180s "${COMMAND[@]}" "${SNAPSHOT_ARGS[@]}" -net none >"$QEMU_LOG" 2>&1 &
QEMU_PID=$!
STATUS=0
while kill -0 "$QEMU_PID" 2>/dev/null; do
    if grep -Fq 'Linos 13 · Sign in' "$LOG" &&
       grep -q "Provisioning state: unclaimed" "$LOG"; then
        kill -TERM "$QEMU_PID" 2>/dev/null || true
        break
    fi
    sleep 2
done
if wait "$QEMU_PID"; then
    STATUS=0
else
    STATUS=$?
fi
if grep -Fq 'Linos 13 · Sign in' "$LOG"; then
    if ! grep -q "Provisioning state: unclaimed" "$LOG"; then
        echo "FAIL: login observed but Linos device initialization was not confirmed" >&2
        exit 1
    fi
    echo "PASS: cliclack local sign-in and first-boot identity initialization observed"
    exit 0
fi

printf 'FAIL: cliclack sign-in screen not observed (QEMU exit: %s)\n' "$STATUS" >&2
tail -n 35 "$LOG" >&2
if [[ -s "$QEMU_LOG" ]]; then
    echo "QEMU diagnostics:" >&2
    tail -n 20 "$QEMU_LOG" >&2
fi
exit 1
)

persistence() (
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

BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' system/release/usr/lib/linos/release)"
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
        ./tests/server.sh smoke "$ARCH"
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
)

console() (
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-$(dpkg --print-architecture)}"
if [[ "$ARCH" != amd64 ]]; then
    echo "Interactive enrollment is currently verified only for AMD64" >&2
    exit 2
fi

RELEASE="system/release/usr/lib/linos/release"
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$RELEASE")"
DIR="images/server/builds/$ARCH/$BUILD_ID"
DISK="$DIR/disk.ext4"
OVERLAY="$DIR/console.qcow2"

for path in "$DIR/boot/kernel" "$DIR/boot/initramfs" "$DISK"; do
    [[ -f "$path" ]] || { echo "Missing build artifact: $path" >&2; exit 2; }
done

# Never silently label an old Debian-kernel VM as a Linos-kernel boot.
# Compatibility is an explicit test-only escape hatch.
MANIFEST="$DIR/kernel-manifest"
if [[ ! -s "$MANIFEST" ]]; then
    echo "No verified kernel manifest in $DIR" >&2
    echo "Build a fresh Linos image. Legacy testing: LINOS_ALLOW_COMPAT=1" >&2
    [[ "${LINOS_ALLOW_COMPAT:-0}" == 1 ]] || exit 2
else
    KERNEL_MODE="$(sed -n 's/^mode=//p' "$MANIFEST")"
    KERNEL_RELEASE="$(sed -n 's/^release=//p' "$MANIFEST")"
    KERNEL_HASH="$(sed -n 's/^kernel_sha256=//p' "$MANIFEST")"
    INITRAMFS_HASH="$(sed -n 's/^initramfs_sha256=//p' "$MANIFEST")"
    [[ "$KERNEL_MODE" == linos && "$KERNEL_RELEASE" == *-linos ]] || {
        echo "Image is not using a Linos kernel ($KERNEL_MODE $KERNEL_RELEASE)" >&2
        [[ "${LINOS_ALLOW_COMPAT:-0}" == 1 ]] || exit 2
    }
    [[ "$KERNEL_HASH" == "$(sha256sum "$DIR/boot/kernel" | cut -d' ' -f1)" ]] || {
        echo "Kernel hash differs from image manifest" >&2; exit 1;
    }
    [[ "$INITRAMFS_HASH" == "$(sha256sum "$DIR/boot/initramfs" | cut -d' ' -f1)" ]] || {
        echo "Initramfs hash differs from image manifest" >&2; exit 1;
    }
    echo "Verified image kernel: $KERNEL_RELEASE ($KERNEL_MODE)"
fi

if [[ ! -f "$OVERLAY" ]]; then
    qemu-img create -f qcow2 -F raw -b "$(realpath "$DISK")" "$OVERLAY"
fi

echo "Linos 13 interactive console"
echo "Persistent guest changes are stored only in: $OVERLAY"
echo "No default login exists. On first boot, create a username and password in Linos enrollment."
echo "After setup, sign in using the username/password you selected."
echo "No network access is configured."
if [[ -c /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]; then
    ACCEL="kvm"
    CPU="host"
else
    ACCEL="tcg"
    CPU="qemu64"
fi
echo "QEMU accelerator: $ACCEL"
if [[ "${LINOS_QEMU_DISPLAY:-gtk}" == "none" ]]; then
    echo "Serial-only mode selected. Use Ctrl+A then X to exit."
    exec qemu-system-x86_64 \
        -machine "pc,accel=$ACCEL" -cpu "$CPU" -smp 2 -m 2048 \
        -nographic -net none \
        -kernel "$DIR/boot/kernel" -initrd "$DIR/boot/initramfs" \
        -drive "file=$OVERLAY,if=none,format=qcow2,id=rootfs" \
        -device virtio-blk-pci,drive=rootfs \
        -append "root=/dev/vda rootwait rw console=ttyS0,115200n8 loglevel=6 systemd.show_status=yes rd.systemd.show_status=yes systemd.unit=multi-user.target"
fi

# Interactive Server setup belongs in the guest's QEMU VGA window, not the
# host terminal. Keep boot/service messages visible on tty1 until enrollment
# explicitly clears the screen for Cliclack. Mirror serial logs to a file.
DISPLAY_BACKEND="${LINOS_QEMU_DISPLAY:-gtk}"
echo "Opening the Linos Server guest display ($DISPLAY_BACKEND)."
echo "Boot messages will be visible in the guest before first-time setup."
echo "Serial diagnostics: $DIR/console-serial.log"
echo "Close the QEMU window to stop the guest."
exec qemu-system-x86_64 \
    -machine "pc,accel=$ACCEL" -cpu "$CPU" -smp 2 -m 2048 \
    -display "$DISPLAY_BACKEND" -device virtio-vga -monitor none -net none \
    -serial "file:$DIR/console-serial.log" \
    -kernel "$DIR/boot/kernel" -initrd "$DIR/boot/initramfs" \
    -drive "file=$OVERLAY,if=none,format=qcow2,id=rootfs" \
    -device virtio-blk-pci,drive=rootfs \
    -append "root=/dev/vda rootwait rw console=ttyS0,115200n8 console=tty0 loglevel=6 systemd.show_status=yes rd.systemd.show_status=yes systemd.unit=multi-user.target"
)

case "${1:-smoke}" in
 smoke) shift || true; smoke "$@" ;;
 persistence) shift; persistence "$@" ;;
 console) shift; console "$@" ;;
 *) smoke "$@" ;;
esac
