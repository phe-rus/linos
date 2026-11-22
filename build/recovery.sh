#!/usr/bin/env bash
set -Eeuo pipefail

recovery_image() (
set -Eeuo pipefail
export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
ARCH="${1:-amd64}"
[[ "$ARCH" == amd64 ]] || { echo "Recovery UEFI prototype currently supports amd64 only" >&2; exit 2; }
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' system/release/usr/lib/linos/release)"
[[ "$BUILD_ID" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "Invalid build identifier" >&2; exit 2; }
OUT="images/recovery/builds/$ARCH/$BUILD_ID"
[[ ! -e "$OUT" ]] || { echo "Recovery build exists: $OUT" >&2; exit 1; }
command -v fakeroot >/dev/null || { echo "fakeroot is required" >&2; exit 2; }
mkdir -p "$OUT/boot"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/linos-recovery.XXXXXXXX")"
trap 'rm -r -- "$WORK"' EXIT
chmod 755 "$WORK"

cargo build --release --bin recovery
install -m 755 target/release/recovery "$WORK/linos-recovery"
install -m 644 system/services/linos-recovery.service "$WORK/linos-recovery.service"
install -m 644 system/release/usr/lib/linos/release "$WORK/release"
install -m 644 hardware/profiles/amd64-uefi-server.conf "$WORK/hardware-profile"
install -m 644 system/identity/os-release "$WORK/os-release"

# Reuse the same qualified package as Server without rebuilding Linux.
# The legacy diagnostic build keeps its explicit Debian package input.
KERNEL_INPUT=linux-image-amd64
EXPECTED_KERNEL=""
if [[ -n "${LINOS_KERNEL_PACKAGE:-}" ]]; then
    source kernel/source.conf
    [[ -s "$LINOS_KERNEL_PACKAGE" &&
       "$(dpkg-deb -f "$LINOS_KERNEL_PACKAGE" Package)" == linos-kernel &&
       "$(dpkg-deb -f "$LINOS_KERNEL_PACKAGE" Architecture)" == "$ARCH" &&
       "$(dpkg-deb -f "$LINOS_KERNEL_PACKAGE" Version)" == "$LINOS_KERNEL_VERSION" ]] || {
        echo "Recovery kernel package metadata mismatch" >&2; exit 2;
    }
    MEMBER="$(dpkg-deb --fsys-tarfile "$LINOS_KERNEL_PACKAGE" | tar -tf - | grep -E '^\./boot/vmlinuz-[^/]+-linos$' || true)"
    [[ -n "$MEMBER" && "$(wc -l <<<"$MEMBER")" -eq 1 ]] || { echo "Expected one Linos kernel" >&2; exit 2; }
    EXPECTED_KERNEL="${MEMBER#./boot/vmlinuz-}"
    KERNEL_INPUT="$WORK/$(basename "$LINOS_KERNEL_PACKAGE")"
    install -m 644 "$LINOS_KERNEL_PACKAGE" "$KERNEL_INPUT"
fi

KEYRING_ARGS=()
if [[ -n "${LINOS_KEYRING:-}" ]]; then
    [[ -r "$LINOS_KEYRING" ]] || { echo "Unreadable Debian keyring" >&2; exit 2; }
    install -m 644 "$LINOS_KEYRING" "$WORK/debian-keyring.gpg"
    KEYRING_ARGS+=(--keyring="$WORK/debian-keyring.gpg")
fi

# Optional offline Server payload is staged only for dedicated installer-media tests.
# SHA-256 checks corruption; no payload is trusted or installed without signing.
PAYLOAD_HOOKS=()
if [[ -n "${LINOS_INSTALL_PAYLOAD_DIR:-}" ]]; then
    PAYLOAD_SOURCE="$LINOS_INSTALL_PAYLOAD_DIR"
    for name in rootfs.tar.gz kernel initramfs manifest SHA256SUMS; do
        [[ -s "$PAYLOAD_SOURCE/$name" ]] || {
            echo "Missing offline payload component: $name" >&2
            exit 2
        }
    done
    (
        cd "$PAYLOAD_SOURCE"
        sha256sum --check SHA256SUMS
    )
    grep -qx 'edition=server' "$PAYLOAD_SOURCE/manifest"
    grep -qx 'architecture=amd64' "$PAYLOAD_SOURCE/manifest"
    mkdir -p "$WORK/payload"
    for name in rootfs.tar.gz kernel initramfs manifest SHA256SUMS; do
        install -m 644 "$PAYLOAD_SOURCE/$name" "$WORK/payload/$name"
    done
    if [[ -n "${LINOS_TRUSTED_PAYLOAD_KEY:-}" ]]; then
        [[ -s "$LINOS_TRUSTED_PAYLOAD_KEY" && -s "$PAYLOAD_SOURCE/SHA256SUMS.sig" ]] || {
            echo "Signed installer media requires both a trusted public key and signature" >&2
            exit 2
        }
        command -v openssl >/dev/null || { echo "openssl is required for signed installer media" >&2; exit 2; }
        openssl pkeyutl -verify -rawin -pubin -inkey "$LINOS_TRUSTED_PAYLOAD_KEY" \
            -in "$PAYLOAD_SOURCE/SHA256SUMS" -sigfile "$PAYLOAD_SOURCE/SHA256SUMS.sig"
        install -m 644 "$LINOS_TRUSTED_PAYLOAD_KEY" "$WORK/payload/signing-public.pem"
        install -m 644 "$PAYLOAD_SOURCE/SHA256SUMS.sig" "$WORK/payload/SHA256SUMS.sig"
    elif [[ -e "$PAYLOAD_SOURCE/SHA256SUMS.sig" ]]; then
        echo "Refusing signed payload without independently supplied trusted public key" >&2
        exit 2
    fi
    PAYLOAD_HOOKS+=(--customize-hook='mkdir -p "$1/usr/share/linos/install"')
    for name in rootfs.tar.gz kernel initramfs manifest SHA256SUMS; do
        PAYLOAD_HOOKS+=(--customize-hook="copy-in $WORK/payload/$name /usr/share/linos/install/")
    done
    if [[ -n "${LINOS_TRUSTED_PAYLOAD_KEY:-}" ]]; then
        PAYLOAD_HOOKS+=(--customize-hook="copy-in $WORK/payload/signing-public.pem /usr/share/linos/install/")
        PAYLOAD_HOOKS+=(--customize-hook="copy-in $WORK/payload/SHA256SUMS.sig /usr/share/linos/install/")
    fi
fi

echo "Building independent Linos Server Recovery root filesystem ($ARCH)"
mmdebstrap \
    "${KEYRING_ARGS[@]}" \
    --variant=minbase \
    --architectures="$ARCH" \
    --hook-dir=/usr/share/mmdebstrap/hooks/file-mirror-automount \
    --include=systemd-sysv,dbus,udev,iproute2,procps,ca-certificates,initramfs-tools,e2fsprogs,openssl \
    --include="$KERNEL_INPUT" \
    --customize-hook='mkdir -p "$1/usr/lib/linos" "$1/etc/systemd/system/multi-user.target.wants"' \
    --customize-hook="copy-in $WORK/linos-recovery /usr/lib/linos/" \
    --customize-hook="copy-in $WORK/release /usr/lib/linos/" \
    --customize-hook="copy-in $WORK/hardware-profile /usr/lib/linos/" \
    --customize-hook='rm -f "$1/etc/os-release"' \
    --customize-hook="copy-in $WORK/os-release /etc/" \
    --customize-hook="copy-in $WORK/linos-recovery.service /usr/lib/systemd/system/" \
    --customize-hook='ln -s /usr/lib/systemd/system/linos-recovery.service "$1/etc/systemd/system/multi-user.target.wants/linos-recovery.service"' \
    "${PAYLOAD_HOOKS[@]}" \
    trixie "$ROOT/$OUT/rootfs.tar"

KERNEL_MEMBER="$(tar -tf "$OUT/rootfs.tar" | awk '/^\.\/boot\/vmlinuz-[^/]+$/ && !found { print; found=1 }')"
[[ -n "$KERNEL_MEMBER" ]] || { echo "Recovery kernel not found" >&2; exit 1; }
VERSION="${KERNEL_MEMBER#./boot/vmlinuz-}"
if [[ -n "$EXPECTED_KERNEL" ]]; then
    [[ "$VERSION" == "$EXPECTED_KERNEL" ]] || { echo "Recovery selected wrong kernel" >&2; exit 1; }
    package_hash="$(dpkg-deb --fsys-tarfile "$LINOS_KERNEL_PACKAGE" | tar -xOf - "$KERNEL_MEMBER" | sha256sum | cut -d' ' -f1)"
    rootfs_hash="$(tar -xOf "$OUT/rootfs.tar" "$KERNEL_MEMBER" | sha256sum | cut -d' ' -f1)"
    [[ "$package_hash" == "$rootfs_hash" ]] || { echo "Recovery kernel differs from package" >&2; exit 1; }
fi
tar -xOf "$OUT/rootfs.tar" "$KERNEL_MEMBER" > "$OUT/boot/kernel"
tar -xOf "$OUT/rootfs.tar" "./boot/initrd.img-$VERSION" > "$OUT/boot/initramfs"

STAGE="$WORK/rootfs"
mkdir -p "$STAGE"
FAKE_STATE="$WORK/fakeroot.state"
fakeroot -s "$FAKE_STATE" -- tar --same-owner --exclude="./dev/*" -xf "$OUT/rootfs.tar" -C "$STAGE"

# Fresh recovery media does not need package-generated shadow database backups.
# These may be mode 000 and unreadable to mkfs.ext4 in rootless fakeroot builds.
rm -f -- "$STAGE/etc/gshadow-" "$STAGE/etc/shadow-"
truncate -s 3G "$OUT/disk.ext4"
fakeroot -i "$FAKE_STATE" -- mkfs.ext4 -F -q -L linos-recovery -d "$STAGE" "$OUT/disk.ext4"
echo "Recovery root filesystem: $OUT/disk.ext4"

LINOS_UEFI_PROFILE=recovery ./build/media.sh "$ARCH"
)

recovery_payload() (
set -Eeuo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-$(dpkg --print-architecture)}"
case "$ARCH" in amd64|arm64) ;; *) echo "Unsupported payload architecture" >&2; exit 2 ;; esac
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' system/release/usr/lib/linos/release)"
[[ "$BUILD_ID" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "Invalid build ID" >&2; exit 2; }
BUILD_DIR="images/server/builds/$ARCH/$BUILD_ID"
OUTPUT="images/install/payloads/$ARCH/$BUILD_ID"

for file in "$BUILD_DIR/rootfs.tar" "$BUILD_DIR/boot/kernel" "$BUILD_DIR/boot/initramfs"; do
    [[ -s "$file" ]] || { echo "Missing server image artifact: $file" >&2; exit 2; }
done
[[ ! -e "$OUTPUT" ]] || { echo "Payload already exists: $OUTPUT" >&2; exit 1; }
mkdir -p "$OUTPUT"

gzip -n -1 -c "$BUILD_DIR/rootfs.tar" > "$OUTPUT/rootfs.tar.gz"
install -m 644 "$BUILD_DIR/boot/kernel" "$OUTPUT/kernel"
install -m 644 "$BUILD_DIR/boot/initramfs" "$OUTPUT/initramfs"
printf '%s\n' \
    'format=1' \
    'edition=server' \
    "architecture=$ARCH" \
    "build=$BUILD_ID" \
    'base=debian-trixie' \
    > "$OUTPUT/manifest"

(
    cd "$OUTPUT"
    sha256sum rootfs.tar.gz kernel initramfs manifest > SHA256SUMS
    sha256sum -c SHA256SUMS
)
echo "Offline development payload built: $OUTPUT"
echo "This checksum manifest does not provide cryptographic authenticity."
)

recovery_sign() (
set -Eeuo pipefail

PAYLOAD_DIR="${1:?Usage: LINOS_PAYLOAD_SIGNING_KEY=/path/to/private.pem ./build/recovery.sh sign /path/to/payload}"
KEY="${LINOS_PAYLOAD_SIGNING_KEY:?Set LINOS_PAYLOAD_SIGNING_KEY to an Ed25519 private key path}"
[[ -f "$KEY" && -r "$KEY" ]] || { echo "Private signing key is not readable" >&2; exit 2; }
[[ -s "$PAYLOAD_DIR/SHA256SUMS" ]] || { echo "Missing SHA256SUMS" >&2; exit 2; }
[[ ! -e "$PAYLOAD_DIR/SHA256SUMS.sig" ]] || { echo "Signature already exists" >&2; exit 2; }
command -v openssl >/dev/null || { echo "openssl is required" >&2; exit 2; }

(
    cd "$PAYLOAD_DIR"
    sha256sum --check SHA256SUMS
)
# Ed25519 signs the exact SHA256SUMS bytes, which cover manifest, kernel, initramfs and rootfs.
openssl pkeyutl -sign -rawin -inkey "$KEY" -in "$PAYLOAD_DIR/SHA256SUMS" \
    -out "$PAYLOAD_DIR/SHA256SUMS.sig"
chmod 644 "$PAYLOAD_DIR/SHA256SUMS.sig"
echo "Signed offline payload: $PAYLOAD_DIR"
echo "Keep the private signing key outside the repository and installation media."
)

case "${1:-image}" in
 image) shift || true; recovery_image "$@" ;;
 payload) shift; recovery_payload "$@" ;;
 sign) shift; recovery_sign "$@" ;;
 *) recovery_image "$@" ;;
esac
