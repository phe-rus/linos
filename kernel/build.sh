#!/usr/bin/env bash
set -Eeuo pipefail

# Build an upstream Linux kernel as a Linos research artifact.
# Does NOT modify the installed OS, any USB drive, or existing Server images.
# Usage: ./kernel/build.sh <amd64|arm64> <source-tree> <output-dir>
# The source tree must contain the exact signed, verified upstream release tag.
if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <amd64|arm64> <upstream-linux-git-tree> <output-dir>" >&2
  exit 2
fi

ARCH_NAME="$1"
SOURCE="$(realpath -- "$2")"
OUTPUT="$(realpath -m -- "$3")"
case "$ARCH_NAME" in
  amd64) KARCH=x86_64; IMAGE=arch/x86/boot/bzImage ;;
  arm64) KARCH=arm64; IMAGE=arch/arm64/boot/Image ;;
  *) echo "Unsupported target: $ARCH_NAME" >&2; exit 2 ;;
esac
if [[ ! -f "$SOURCE/Makefile" || ! -d "$SOURCE/.git" ]]; then
  echo "Expected an upstream Linux git checkout: $SOURCE" >&2
  exit 2
fi
if [[ "$OUTPUT" == "$SOURCE" || "$OUTPUT" == "$SOURCE/"* ]]; then
  echo "Use a separate output directory outside the Linux source tree" >&2
  exit 2
fi
if [[ -e "$OUTPUT" && -n "$(ls -A "$OUTPUT")" ]]; then
  echo "Output directory must be empty: $OUTPUT" >&2
  exit 2
fi

# A local trusted keyring must be configured in GnuPG before this step.
# Do not accept unsigned source tags or implicitly trust arbitrary fetched keys.
RELEASE="${LINOS_KERNEL_RELEASE:-7.3.4}"
case "$RELEASE" in
  7.3.4|6.18.63) ;;
  *) echo "Release not in initial research allowlist: $RELEASE" >&2; exit 2 ;;
esac
TAG="v$RELEASE"
git -C "$SOURCE" diff --quiet
git -C "$SOURCE" diff --cached --quiet
if [[ -n "$(git -C "$SOURCE" status --porcelain --untracked-files=no)" ]]; then
  echo "Upstream source has local changes; apply Linos patches only through reviewed patch sets" >&2
  exit 1
fi
if [[ "$(git -C "$SOURCE" rev-parse HEAD)" != "$(git -C "$SOURCE" rev-list -n 1 "$TAG")" ]]; then
  echo "HEAD must exactly match $TAG" >&2
  exit 1
fi
git -C "$SOURCE" verify-tag "$TAG" || {
  echo "Cannot verify upstream signed tag $TAG against your configured trusted keys" >&2
  exit 1
}

for cmd in make gcc bc bison flex openssl; do
  command -v "$cmd" >/dev/null || { echo "Missing build prerequisite: $cmd" >&2; exit 2; }
done
mkdir -p "$OUTPUT"
export KBUILD_BUILD_USER=linos
export KBUILD_BUILD_HOST=builder
export KBUILD_BUILD_TIMESTAMP="${SOURCE_DATE_EPOCH:-@0}"
export KBUILD_BUILD_VERSION=1
make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" defconfig
"$SOURCE/scripts/config" --file "$OUTPUT/.config" --set-str LOCALVERSION '-linos13'
make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" olddefconfig
JOBS="${LINOS_KERNEL_JOBS:-2}"
make -C "$SOURCE" O="$OUTPUT" ARCH="$KARCH" -j "$JOBS" "${IMAGE##*/}" modules

# Keep machine-readable provenance with the compiled kernel, not in source control.
{
  printf 'product=linos13\n'
  printf 'upstream_tag=%s\n' "$TAG"
  printf 'upstream_commit=%s\n' "$(git -C "$SOURCE" rev-parse HEAD)"
  printf 'architecture=%s\n' "$ARCH_NAME"
  printf 'build_timestamp=%s\n' "$KBUILD_BUILD_TIMESTAMP"
  printf 'config_sha256=%s\n' "$(sha256sum "$OUTPUT/.config" | awk '{print $1}')"
  printf 'kernel_sha256=%s\n' "$(sha256sum "$OUTPUT/$IMAGE" | awk '{print $1}')"
} >"$OUTPUT/provenance.txt"
echo "Linos research kernel built: $OUTPUT/$IMAGE"
echo "This is NOT a boot-tested or signed release."
