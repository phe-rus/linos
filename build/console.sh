#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-$(dpkg --print-architecture)}"
if [[ "$ARCH" != amd64 ]]; then
    echo "Interactive enrollment is currently verified only for AMD64" >&2
    exit 2
fi

RELEASE="packages/release/usr/lib/linos/release"
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$RELEASE")"
DIR="images/server/builds/$ARCH/$BUILD_ID"
DISK="$DIR/disk.ext4"
OVERLAY="$DIR/console.qcow2"

for path in "$DIR/boot/kernel" "$DIR/boot/initramfs" "$DISK"; do
    [[ -f "$path" ]] || { echo "Missing build artifact: $path" >&2; exit 2; }
done

if [[ ! -f "$OVERLAY" ]]; then
    qemu-img create -f qcow2 -F raw -b "$(realpath "$DISK")" "$OVERLAY"
fi

echo "Linos 13 interactive console"
echo "Persistent guest changes are stored only in: $OVERLAY"
echo "No default login exists. On first boot, create a username and password in Linos enrollment."
echo "After setup, sign in using the username/password you selected."
echo "No network access is configured."
echo "To exit: press Ctrl+A, release both keys, then press X (not Ctrl+X)."
echo "Alternatively: Ctrl+A, release, C to open QEMU monitor, then type quit."

exec qemu-system-x86_64 \
    -machine pc,accel=tcg -cpu qemu64 -smp 2 -m 2048 \
    -nographic -no-reboot -net none \
    -kernel "$DIR/boot/kernel" -initrd "$DIR/boot/initramfs" \
    -drive "file=$OVERLAY,if=none,format=qcow2,id=rootfs" \
    -device virtio-blk-pci,drive=rootfs \
    -append "root=/dev/vda rootwait rw console=ttyS0,115200n8 quiet loglevel=3 systemd.show_status=auto rd.systemd.show_status=auto systemd.unit=multi-user.target"
