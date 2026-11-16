#!/usr/bin/env bash
set -Eeuo pipefail

# Research build only. Never changes host OS, storage devices, or Server images.
# ./kernel/build.sh <amd64|arm64> <verified-upstream-git-checkout> <empty-output-dir>
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
  --enable SERIAL_8250_CONSOLE
make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" olddefconfig
for required in CONFIG_MODULES=y CONFIG_BLK_DEV_INITRD=y CONFIG_DEVTMPFS=y CONFIG_DEVTMPFS_MOUNT=y CONFIG_SERIAL_8250_CONSOLE=y; do
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
