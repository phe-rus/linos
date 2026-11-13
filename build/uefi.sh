#!/usr/bin/env bash
set -Eeuo pipefail

# Build a firmware-bootable GPT/UEFI disk from the existing tested server rootfs.
# This is the UEFI boot foundation, not yet the Linos Recovery installer.
export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-amd64}"
if [[ "$ARCH" != amd64 ]]; then
    echo "UEFI disk images currently support amd64 only (requested: $ARCH)" >&2
    exit 2
fi

BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' packages/release/usr/lib/linos/release)"
if [[ ! "$BUILD_ID" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "Invalid release build identifier" >&2
    exit 2
fi

PROFILE="${LINOS_UEFI_PROFILE:-server}"
case "$PROFILE" in
    server) BUILD_DIR="images/server/builds/$ARCH/$BUILD_ID" ;;
    recovery) BUILD_DIR="images/recovery/builds/$ARCH/$BUILD_ID" ;;
    *) echo "Unsupported UEFI profile: $PROFILE" >&2; exit 2 ;;
esac
KERNEL="$BUILD_DIR/boot/kernel"
INITRAMFS="$BUILD_DIR/boot/initramfs"
ROOTFS="$BUILD_DIR/disk.ext4"
OUTPUT="$BUILD_DIR/disk-uefi.raw"

for utility in sgdisk mkfs.vfat mmd mcopy dd; do
    if ! command -v "$utility" >/dev/null; then
        echo "Missing tool: $utility (install gdisk dosfstools mtools)" >&2
        exit 2
    fi
done
for path in "$KERNEL" "$INITRAMFS" "$ROOTFS"; do
    if [[ ! -s "$path" ]]; then
        echo "Missing boot artifact: $path; first run ./build/image.sh" >&2
        exit 2
    fi
done

BOOTLOADER=""
for candidate in /usr/lib/systemd/boot/efi/systemd-bootx64.efi /usr/lib/systemd/boot/efi/systemd-bootx64.efi.signed; do
    if [[ -r "$candidate" ]]; then
        BOOTLOADER="$candidate"
        break
    fi
done
if [[ -z "$BOOTLOADER" ]]; then
    echo "systemd-boot EFI binary missing (install systemd-boot-efi)" >&2
    exit 2
fi
if [[ -e "$OUTPUT" ]]; then
    echo "UEFI disk already exists, refusing to overwrite: $OUTPUT" >&2
    exit 1
fi

WORK="$(mktemp -d /tmp/linos-uefi.XXXXXXXX)"
trap 'rm -r -- "$WORK"' EXIT
ESP="$WORK/esp.fat"
DISK="$WORK/disk.raw"

# Leave room for primary and backup GPT structures, ESP, and the 3 GiB ext4 root.
truncate -s 4G "$DISK"
sgdisk --clear \
    --new=1:2048:+512M --typecode=1:ef00 --change-name=1:linos-efi \
    --new=2:0:+3G --typecode=2:8300 --change-name=2:linos-system \
    "$DISK" >/dev/null

ROOT_UUID="$(dumpe2fs -h "$ROOTFS" 2>/dev/null | sed -n 's/^Filesystem UUID:[[:space:]]*//p')"
ESP_START="$(sgdisk -i 1 "$DISK" | sed -n 's/^First sector: \([0-9]*\).*$/\1/p')"
ROOT_START="$(sgdisk -i 2 "$DISK" | sed -n 's/^First sector: \([0-9]*\).*$/\1/p')"
ROOT_LAST="$(sgdisk -i 2 "$DISK" | sed -n 's/^Last sector: \([0-9]*\).*$/\1/p')"
if [[ -z "$ROOT_UUID" || -z "$ESP_START" || -z "$ROOT_START" || -z "$ROOT_LAST" ]]; then
    echo "Could not inspect GPT partition metadata" >&2
    exit 1
fi
ROOT_BYTES="$(stat -c '%s' "$ROOTFS")"
if (( ROOT_BYTES > (ROOT_LAST - ROOT_START + 1) * 512 )); then
    echo "Root filesystem exceeds target system partition" >&2
    exit 1
fi

truncate -s 512M "$ESP"
mkfs.vfat -F 32 -n LINOS_EFI "$ESP" >/dev/null
mmd -i "$ESP" ::/EFI ::/EFI/BOOT ::/linos ::/loader ::/loader/entries
mcopy -i "$ESP" "$BOOTLOADER" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$ESP" "$KERNEL" ::/linos/kernel
mcopy -i "$ESP" "$INITRAMFS" ::/linos/initramfs

BOOT_PRESENTATION='quiet loglevel=3 systemd.show_status=auto rd.systemd.show_status=auto'
TITLE='Linos 13 Server'
KERNEL_OPTIONS='systemd.mask=linos-enroll.service systemd.unit=multi-user.target'
if [[ "$PROFILE" == recovery ]]; then
    TITLE='Linos 13 Recovery (Server CLI)'
    KERNEL_OPTIONS='systemd.mask=serial-getty@ttyS0.service systemd.unit=multi-user.target'
fi

printf '%s\n' 'default linos' 'timeout 2' 'console-mode keep' > "$WORK/loader.conf"
printf '%s\n' \
    "title $TITLE" \
    'linux /linos/kernel' \
    'initrd /linos/initramfs' \
    "options root=UUID=$ROOT_UUID rootwait rw console=ttyS0,115200n8 $BOOT_PRESENTATION $KERNEL_OPTIONS" \
    > "$WORK/linos.conf"
mcopy -i "$ESP" "$WORK/loader.conf" ::/loader/loader.conf
mcopy -i "$ESP" "$WORK/linos.conf" ::/loader/entries/linos.conf

dd if="$ESP" of="$DISK" bs=512 seek="$ESP_START" conv=notrunc status=none
dd if="$ROOTFS" of="$DISK" bs=512 seek="$ROOT_START" conv=notrunc status=none
mv "$DISK" "$OUTPUT"

echo "UEFI image created: $OUTPUT"
echo "System filesystem UUID: $ROOT_UUID"
echo "Boot mode: GPT + FAT32 EFI + systemd-boot ($TITLE)"
