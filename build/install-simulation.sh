#!/usr/bin/env bash
set -Eeuo pipefail

# Test-only installation simulator. Accepts no disk path, block device or
# arbitrary output directory. All writes are to freshly-created /tmp files.
export PATH="/usr/sbin:/sbin:$PATH"
[[ $# -eq 0 ]] || { echo "Usage: ./build/install-simulation.sh" >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' packages/release/usr/lib/linos/release)"
PAYLOAD="images/install/payloads/amd64/$BUILD_ID"
for name in rootfs.tar.gz kernel initramfs manifest SHA256SUMS; do
    [[ -s "$PAYLOAD/$name" ]] || { echo "Missing payload artifact: $name" >&2; exit 2; }
done
(
    cd "$PAYLOAD"
    sha256sum -c SHA256SUMS
)
grep -qx 'edition=server' "$PAYLOAD/manifest"
grep -qx 'architecture=amd64' "$PAYLOAD/manifest"

WORK="$(mktemp -d /tmp/linos-install-simulation.XXXXXXXX)"
trap 'rm -r -- "$WORK"' EXIT
DISK="$WORK/installed.raw"
ESP="$WORK/efi.fat"
SYSTEM="$WORK/system-a.ext4"
STAGE="$WORK/rootfs"
mkdir -p "$STAGE"

# Stage offline system payload using fakeroot to preserve root-owned metadata.
FAKE_STATE="$WORK/fakeroot.state"
fakeroot -s "$FAKE_STATE" -- tar -xzf "$PAYLOAD/rootfs.tar.gz" --same-owner --exclude='./dev/*' -C "$STAGE"
rm -f -- "$STAGE/etc/gshadow-" "$STAGE/etc/shadow-"
truncate -s 4G "$SYSTEM"
fakeroot -i "$FAKE_STATE" -- mkfs.ext4 -F -q -L linos-system-a -d "$STAGE" "$SYSTEM"
SYSTEM_UUID="$(dumpe2fs -h "$SYSTEM" 2>/dev/null | sed -n 's/^Filesystem UUID:[[:space:]]*//p')"
[[ -n "$SYSTEM_UUID" ]] || { echo "Unable to read installed filesystem UUID" >&2; exit 1; }
# The installed System A must contain the new native administration console.
debugfs -R 'stat /usr/bin/linos' "$SYSTEM" 2>/dev/null | grep -q 'Type: regular' || {
    echo "Installed system has no Linos administration console" >&2
    exit 1
}

truncate -s 16G "$DISK"
sgdisk --clear \
    --new=1:2048:+512M --typecode=1:ef00 --change-name=1:linos-efi \
    --new=2:0:+2G --typecode=2:8300 --change-name=2:linos-recovery \
    --new=3:0:+4G --typecode=3:8300 --change-name=3:linos-system-a \
    --new=4:0:+4G --typecode=4:8300 --change-name=4:linos-system-b \
    --new=5:0:0 --typecode=5:8300 --change-name=5:linos-data \
    "$DISK" >/dev/null
sgdisk --verify "$DISK"
ESP_START="$(sgdisk -i 1 "$DISK" | sed -n 's/^First sector: \([0-9]*\).*$/\1/p')"
SYSTEM_START="$(sgdisk -i 3 "$DISK" | sed -n 's/^First sector: \([0-9]*\).*$/\1/p')"
[[ -n "$ESP_START" && -n "$SYSTEM_START" ]] || { echo "GPT inspection failed" >&2; exit 1; }

BOOTLOADER=/usr/lib/systemd/boot/efi/systemd-bootx64.efi
[[ -r "$BOOTLOADER" ]] || { echo "Missing systemd-boot EFI binary" >&2; exit 2; }
truncate -s 512M "$ESP"
mkfs.vfat -F 32 -n LINOS_EFI "$ESP" >/dev/null
mmd -i "$ESP" ::/EFI ::/EFI/BOOT ::/linos ::/loader ::/loader/entries
mcopy -i "$ESP" "$BOOTLOADER" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$ESP" "$PAYLOAD/kernel" ::/linos/kernel
mcopy -i "$ESP" "$PAYLOAD/initramfs" ::/linos/initramfs
printf '%s\n' 'default linos' 'timeout 2' > "$WORK/loader.conf"
printf '%s\n' \
    'title Linos 13 Server (simulated install)' \
    'linux /linos/kernel' \
    'initrd /linos/initramfs' \
    "options root=UUID=$SYSTEM_UUID rootwait rw console=ttyS0,115200n8 systemd.mask=linos-enroll.service systemd.unit=multi-user.target" \
    > "$WORK/linos.conf"
mcopy -i "$ESP" "$WORK/loader.conf" ::/loader/loader.conf
mcopy -i "$ESP" "$WORK/linos.conf" ::/loader/entries/linos.conf
dd if="$ESP" of="$DISK" bs=512 seek="$ESP_START" conv=notrunc status=none
dd if="$SYSTEM" of="$DISK" bs=512 seek="$SYSTEM_START" conv=notrunc status=none

CODE=/usr/share/OVMF/OVMF_CODE_4M.fd
VARS=/usr/share/OVMF/OVMF_VARS_4M.fd
[[ -r "$CODE" && -r "$VARS" ]] || { echo "OVMF 4M firmware missing" >&2; exit 2; }
cp "$VARS" "$WORK/vars.fd"
LOG="$WORK/serial.log"
: > "$LOG"
echo "Booting simulated Linos Server installation from System A"
timeout --signal=TERM --kill-after=5s 180s qemu-system-x86_64 \
    -machine q35,accel=tcg -cpu qemu64 -smp 2 -m 2048 \
    -display none -no-reboot -monitor none -net none -snapshot \
    -drive "if=pflash,format=raw,readonly=on,file=$CODE" \
    -drive "if=pflash,format=raw,file=$WORK/vars.fd" \
    -drive "file=$DISK,if=none,format=raw,id=installed" \
    -device virtio-blk-pci,drive=installed \
    -chardev "file,id=linos_serial,path=$LOG" -serial chardev:linos_serial \
    > "$WORK/qemu.log" 2>&1 &
PID=$!
while kill -0 "$PID" 2>/dev/null; do
    if grep -Eq '(^|[[:space:]])[^[:space:]]+[[:space:]]login:' "$LOG" &&
       grep -q 'Provisioning state: unclaimed' "$LOG"; then
        kill -TERM "$PID" 2>/dev/null || true
        break
    fi
    sleep 2
done
wait "$PID" || true
if grep -Eq '(^|[[:space:]])[^[:space:]]+[[:space:]]login:' "$LOG" &&
   grep -q 'Provisioning state: unclaimed' "$LOG"; then
    echo "PASS: offline payload deployed to GPT System A and booted in UEFI QEMU"
else
    echo "FAIL: simulated installed system did not reach login" >&2
    tail -n 50 "$LOG" >&2
    tail -n 25 "$WORK/qemu.log" >&2
    exit 1
fi
