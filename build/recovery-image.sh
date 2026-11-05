#!/usr/bin/env bash
set -Eeuo pipefail
export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
ARCH="${1:-amd64}"
[[ "$ARCH" == amd64 ]] || { echo "Recovery UEFI prototype currently supports amd64 only" >&2; exit 2; }
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' packages/release/usr/lib/linos/release)"
[[ "$BUILD_ID" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "Invalid build identifier" >&2; exit 2; }
OUT="images/recovery/builds/$ARCH/$BUILD_ID"
[[ ! -e "$OUT" ]] || { echo "Recovery build exists: $OUT" >&2; exit 1; }
command -v fakeroot >/dev/null || { echo "fakeroot is required" >&2; exit 2; }
mkdir -p "$OUT/boot"
WORK="$(mktemp -d /tmp/linos-recovery.XXXXXXXX)"
trap 'rm -r -- "$WORK"' EXIT
chmod 755 "$WORK"

cargo build --release -p recovery
install -m 755 target/release/recovery "$WORK/linos-recovery"
install -m 644 system/services/linos-recovery.service "$WORK/linos-recovery.service"
install -m 644 packages/release/usr/lib/linos/release "$WORK/release"
install -m 644 hardware/profiles/amd64-uefi-server.conf "$WORK/hardware-profile"

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
    --include=systemd-sysv,dbus,udev,iproute2,procps,ca-certificates,initramfs-tools,e2fsprogs,linux-image-amd64,openssl \
    --customize-hook='mkdir -p "$1/usr/lib/linos" "$1/etc/systemd/system/multi-user.target.wants"' \
    --customize-hook="copy-in $WORK/linos-recovery /usr/lib/linos/" \
    --customize-hook="copy-in $WORK/release /usr/lib/linos/" \
    --customize-hook="copy-in $WORK/hardware-profile /usr/lib/linos/" \
    --customize-hook="copy-in $WORK/linos-recovery.service /usr/lib/systemd/system/" \
    --customize-hook='ln -s /usr/lib/systemd/system/linos-recovery.service "$1/etc/systemd/system/multi-user.target.wants/linos-recovery.service"' \
    "${PAYLOAD_HOOKS[@]}" \
    trixie "$ROOT/$OUT/rootfs.tar"

KERNEL_MEMBER="$(tar -tf "$OUT/rootfs.tar" | awk '/^\.\/boot\/vmlinuz-[^/]+$/ && !found { print; found=1 }')"
[[ -n "$KERNEL_MEMBER" ]] || { echo "Recovery kernel not found" >&2; exit 1; }
VERSION="${KERNEL_MEMBER#./boot/vmlinuz-}"
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

LINOS_UEFI_PROFILE=recovery ./build/uefi.sh "$ARCH"
