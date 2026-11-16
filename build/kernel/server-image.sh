#!/usr/bin/env bash
set -Eeuo pipefail

# Assemble a disposable Linos Server QEMU disk with a separately built kernel.
# The existing Debian-based rootfs is an INPUT; no physical device is ever touched.
# Usage: bash build/kernel/server-image.sh <rootfs.tar> <kernel-build-dir> <empty-output-dir>
[[ $# -eq 3 ]] || { echo "Usage: $0 <rootfs.tar> <kernel-build-dir> <empty-output-dir>" >&2; exit 2; }
ROOTFS="$(realpath -- "$1")"
BUILD="$(realpath -- "$2")"
DEST="$(realpath -m -- "$3")"
[[ -s "$ROOTFS" && -s "$BUILD/provenance.txt" && -s "$BUILD/arch/x86/boot/bzImage" ]] || {
  echo "Missing rootfs.tar or completed amd64 research kernel" >&2; exit 2;
}
[[ "$DEST" != "$BUILD" && "$DEST" != "$BUILD/"* && "$DEST" != "$ROOTFS" ]] || {
  echo "Output must be independent of the inputs" >&2; exit 2;
}
[[ ! -e "$DEST" || -z "$(ls -A "$DEST")" ]] || {
  echo "Refusing nonempty output directory: $DEST" >&2; exit 2;
}
for cmd in tar mkfs.ext4 truncate sudo depmod timeout; do
  command -v "$cmd" >/dev/null || { echo "Missing prerequisite: $cmd" >&2; exit 2; }
done
sudo -n true || { echo "Non-interactive sudo required for isolated rootfs assembly" >&2; exit 2; }
REL="$(sed -n 's/^kernel_release=//p' "$BUILD/provenance.txt")"
ARCH="$(sed -n 's/^architecture=//p' "$BUILD/provenance.txt")"
[[ "$ARCH" == amd64 && "$REL" =~ ^[A-Za-z0-9._+-]+$ && "$REL" == *-linos13* ]] || {
  echo "Kernel provenance is not a matching Linos amd64 build" >&2; exit 1;
}
MODULE_SOURCE="$BUILD/staged/lib/modules/$REL"
[[ -d "$MODULE_SOURCE" ]] || { echo "Kernel module directory missing: $MODULE_SOURCE" >&2; exit 1; }

mkdir -p "$DEST"
WORK="$(mktemp -d /tmp/linos-kernel-server.XXXXXXXX)"
STAGE="$WORK/root"
mkdir -p "$STAGE"
MOUNTED=()
cleanup() {
  local index failed=0
  for ((index=${#MOUNTED[@]}-1; index>=0; index--)); do
    if ! sudo umount "${MOUNTED[index]}"; then failed=1; fi
  done
  if (( failed == 0 )); then
    sudo rm -rf -- "$WORK"
  else
    echo "WARNING: temporary mounts remain active; preserving $WORK to prevent deleting bound host filesystems" >&2
  fi
}
trap cleanup EXIT

# Extract with original root ownership. Chroot is used only in temporary workspace.
sudo tar --numeric-owner --same-owner -xf "$ROOTFS" -C "$STAGE"
sudo mkdir -p "$STAGE/usr/lib/modules" "$STAGE/boot" "$STAGE/proc" "$STAGE/sys" "$STAGE/dev"
sudo cp -a -- "$MODULE_SOURCE" "$STAGE/usr/lib/modules/$REL"
sudo install -m 644 "$BUILD/arch/x86/boot/bzImage" "$STAGE/boot/vmlinuz-$REL"
sudo install -m 644 "$BUILD/.config" "$STAGE/boot/config-$REL"
sudo install -m 644 "$BUILD/System.map" "$STAGE/boot/System.map-$REL"
# A new kernel's modules must be resolved in the target filesystem, not the host.
sudo depmod -b "$STAGE" "$REL"
sudo mount -t proc proc "$STAGE/proc"; MOUNTED+=("$STAGE/proc")
sudo mount --bind /sys "$STAGE/sys"; MOUNTED+=("$STAGE/sys")
sudo mount --bind /dev "$STAGE/dev"; MOUNTED+=("$STAGE/dev")
sudo chroot "$STAGE" /usr/sbin/mkinitramfs -o "/boot/initrd.img-$REL" "$REL"
[[ -s "$STAGE/boot/initrd.img-$REL" ]] || { echo "Initramfs not generated" >&2; exit 1; }
# No mounted pseudo-filesystems may be included in the produced disk.
for ((i=${#MOUNTED[@]}-1; i>=0; i--)); do sudo umount "${MOUNTED[i]}"; done
MOUNTED=()

install -m 644 "$BUILD/arch/x86/boot/bzImage" "$DEST/kernel"
sudo install -m 644 "$STAGE/boot/initrd.img-$REL" "$DEST/initramfs"
truncate -s 3G "$DEST/disk.ext4"
sudo mkfs.ext4 -F -q -L linos-root -d "$STAGE" "$DEST/disk.ext4"
printf '%s\n' "$REL" > "$DEST/kernel-release"
sha256sum "$DEST/kernel" "$DEST/initramfs" "$DEST/disk.ext4" > "$DEST/SHA256SUMS"
echo "Disposable Server kernel image assembled: $DEST"
