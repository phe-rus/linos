#!/usr/bin/env bash
set -Eeuo pipefail

export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-$(dpkg --print-architecture)}"
RELEASE_FILE="packages/release/usr/lib/linos/release"
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
    if grep -Eq '(^|[[:space:]])[^[:space:]]+[[:space:]]login:' "$LOG" &&
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
if grep -Eq '(^|[[:space:]])[^[:space:]]+[[:space:]]login:' "$LOG"; then
    if ! grep -q "Provisioning state: unclaimed" "$LOG"; then
        echo "FAIL: login observed but Linos device initialization was not confirmed" >&2
        exit 1
    fi
    echo "PASS: serial login and Linos first-boot identity initialization observed"
    exit 0
fi

printf 'FAIL: serial login prompt not observed (QEMU exit: %s)\n' "$STATUS" >&2
tail -n 35 "$LOG" >&2
if [[ -s "$QEMU_LOG" ]]; then
    echo "QEMU diagnostics:" >&2
    tail -n 20 "$QEMU_LOG" >&2
fi
exit 1
