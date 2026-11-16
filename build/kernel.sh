#!/usr/bin/env bash
set -Eeuo pipefail

kernel_compile() (
set -Eeuo pipefail

# Research build only. Never changes host OS, storage devices, or Server images.
# ./build/kernel.sh build <amd64|arm64> <verified-upstream-git-checkout> <empty-output-dir>
if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <amd64|arm64> <verified-upstream-git-checkout> <empty-output-dir>" >&2
  exit 2
fi
ARCH_NAME="$1"
SOURCE="$(realpath -- "$2")"
OUTPUT="$(realpath -m -- "$3")"
case "$ARCH_NAME" in
  amd64) KARCH=x86; IMAGE=arch/x86/boot/bzImage ;;
  arm64) KARCH=arm64; IMAGE=arch/arm64/boot/Image ;;
  *) echo "Unsupported target: $ARCH_NAME" >&2; exit 2 ;;
esac
[[ -f "$SOURCE/Makefile" ]] && git -C "$SOURCE" rev-parse --is-inside-work-tree >/dev/null 2>&1 || {
  echo "Expected an upstream Linux git checkout" >&2; exit 2;
}
SOURCE="$(realpath -- "$SOURCE")"
if [[ "$OUTPUT" == "$SOURCE" || "$OUTPUT" == "$SOURCE/"* ]]; then
  echo "Build output must be outside the source tree" >&2; exit 2
fi
if [[ -e "$OUTPUT" && -n "$(ls -A "$OUTPUT")" ]]; then
  echo "Output directory is not empty: $OUTPUT" >&2; exit 2
fi

RELEASE="${LINOS_KERNEL_RELEASE:-7.3.4}"
case "$RELEASE" in 7.3.4|6.18.63) ;; *) echo "Unapproved research release: $RELEASE" >&2; exit 2 ;; esac
TAG="v$RELEASE"
if [[ -n "$(git -C "$SOURCE" status --porcelain --untracked-files=no)" ]]; then
  echo "Upstream source has tracked changes" >&2; exit 1
fi
if [[ "$(git -C "$SOURCE" rev-parse HEAD)" != "$(git -C "$SOURCE" rev-list -n 1 "$TAG")" ]]; then
  echo "Upstream checkout is not $TAG" >&2; exit 1
fi
# Verifying a tag requires the upstream signing key in the local keyring.
git -C "$SOURCE" verify-tag "$TAG" || {
  echo "Upstream tag signature verification failed: $TAG" >&2; exit 1
}
for cmd in make gcc bc bison flex openssl sha256sum; do
  command -v "$cmd" >/dev/null || { echo "Missing build dependency: $cmd" >&2; exit 2; }
done
mkdir -p "$OUTPUT"
export KBUILD_BUILD_USER=linos
export KBUILD_BUILD_HOST=builder
export KBUILD_BUILD_TIMESTAMP="${SOURCE_DATE_EPOCH:-@0}"
export KBUILD_BUILD_VERSION=1

make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" defconfig
"$SOURCE/scripts/config" --file "$OUTPUT/.config" \
  --set-str LOCALVERSION '-linos13' \
  --enable MODULES \
  --enable BLK_DEV_INITRD \
  --enable DEVTMPFS \
  --enable DEVTMPFS_MOUNT \
  --enable SERIAL_8250 \
  --enable SERIAL_8250_CONSOLE \
  --enable PCI \
  --enable VIRTIO \
  --enable VIRTIO_PCI \
  --enable VIRTIO_BLK \
  --enable EXT4_FS \
  --enable PROC_FS \
  --enable SYSFS \
  --enable TMPFS
make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" olddefconfig
for required in CONFIG_MODULES=y CONFIG_BLK_DEV_INITRD=y CONFIG_DEVTMPFS=y CONFIG_DEVTMPFS_MOUNT=y CONFIG_SERIAL_8250_CONSOLE=y CONFIG_PCI=y CONFIG_VIRTIO=y CONFIG_VIRTIO_PCI=y CONFIG_VIRTIO_BLK=y CONFIG_EXT4_FS=y CONFIG_PROC_FS=y CONFIG_SYSFS=y CONFIG_TMPFS=y; do
  if ! grep -Fxq "$required" "$OUTPUT/.config"; then
    echo "Kernel config failed required policy: $required" >&2
    exit 1
  fi
done
JOBS="${LINOS_KERNEL_JOBS:-2}"
[[ "$JOBS" =~ ^[1-9][0-9]*$ ]] || { echo "LINOS_KERNEL_JOBS must be positive" >&2; exit 2; }
make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" -j "$JOBS" "${IMAGE##*/}" modules
KERNEL_RELEASE="$(make -s -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" kernelrelease)"
MODULES_DEST="$OUTPUT/staged"
make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" \
  INSTALL_MOD_PATH="$MODULES_DEST" DEPMOD=true modules_install
[[ -s "$OUTPUT/$IMAGE" ]] || { echo "Kernel image missing" >&2; exit 1; }
[[ -d "$MODULES_DEST/lib/modules/$KERNEL_RELEASE" || -d "$MODULES_DEST/usr/lib/modules/$KERNEL_RELEASE" ]] || {
  echo "Kernel modules staging directory missing" >&2; exit 1;
}
{
  printf 'product=Linos 13\n'
  printf 'source_tag=%s\n' "$TAG"
  printf 'source_commit=%s\n' "$(git -C "$SOURCE" rev-parse HEAD)"
  printf 'architecture=%s\n' "$ARCH_NAME"
  printf 'kernel_release=%s\n' "$KERNEL_RELEASE"
  printf 'config_sha256=%s\n' "$(sha256sum "$OUTPUT/.config" | cut -d ' ' -f1)"
  printf 'image_sha256=%s\n' "$(sha256sum "$OUTPUT/$IMAGE" | cut -d ' ' -f1)"
} >"$OUTPUT/provenance.txt"
echo "Research kernel: $OUTPUT/$IMAGE"
echo "Modules: $MODULES_DEST"
echo "This build has not been qualified as a Linos release."
)

kernel_server_image() (
set -Eeuo pipefail

# Assemble a disposable Linos Server QEMU disk with a separately built kernel.
# The existing Debian-based rootfs is an INPUT; no physical device is ever touched.
# Usage: bash build/kernel.sh server-image <rootfs.tar> <kernel-build-dir> <empty-output-dir>
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
)

case "${1:-build}" in
 build) shift; kernel_compile "$@" ;;
 server-image) shift; kernel_server_image "$@" ;;
 *) kernel_compile "$@" ;;
esac
