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
DISK="$BUILD_DIR/disk.ext4"
LOG="$BUILD_DIR/smoke.log"

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
        -nographic -no-reboot -monitor none
        -kernel "$KERNEL" -initrd "$INITRAMFS"
        -drive "file=$DISK,if=none,format=raw,id=rootfs"
        -device virtio-blk-pci,drive=rootfs
        -append "root=/dev/vda rootwait rw console=ttyS0,115200n8 earlyprintk=serial,ttyS0,115200 systemd.unit=multi-user.target"
    )
else
    COMMAND=(
        qemu-system-aarch64
        -machine virt -cpu cortex-a76 -smp 2 -m 2048
        -nographic
        -kernel "$KERNEL" -initrd "$INITRAMFS"
        -drive "file=$DISK,if=none,format=raw,id=rootfs"
        -device virtio-blk-device,drive=rootfs
        -append "root=/dev/vda rootwait rw console=ttyAMA0 systemd.unit=multi-user.target"
    )
fi

echo "Boot-testing Linos 13 ($ARCH), build $BUILD_ID"
echo "Serial log: $LOG"
set +e
timeout --signal=TERM --kill-after=5s 120s "${COMMAND[@]}" -snapshot -net none >"$LOG" 2>&1
STATUS=$?
set -e

if grep -Eq '(^|[[:space:]])[^[:space:]]+[[:space:]]login:' "$LOG"; then
    echo "PASS: serial login prompt observed"
    exit 0
fi

printf 'FAIL: serial login prompt not observed (QEMU exit: %s)\n' "$STATUS" >&2
tail -n 35 "$LOG" >&2
exit 1
