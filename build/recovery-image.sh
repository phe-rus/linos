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

KEYRING_ARGS=()
if [[ -n "${LINOS_KEYRING:-}" ]]; then
    [[ -r "$LINOS_KEYRING" ]] || { echo "Unreadable Debian keyring" >&2; exit 2; }
    install -m 644 "$LINOS_KEYRING" "$WORK/debian-keyring.gpg"
    KEYRING_ARGS+=(--keyring="$WORK/debian-keyring.gpg")
fi

echo "Building independent Linos Server Recovery root filesystem ($ARCH)"
mmdebstrap \
    "${KEYRING_ARGS[@]}" \
    --variant=minbase \
    --architectures="$ARCH" \
    --include=systemd-sysv,dbus,udev,iproute2,procps,ca-certificates,initramfs-tools,e2fsprogs,linux-image-amd64 \
    --customize-hook='mkdir -p "$1/usr/lib/linos" "$1/etc/systemd/system/multi-user.target.wants"' \
    --customize-hook="copy-in $WORK/linos-recovery /usr/lib/linos/" \
    --customize-hook="copy-in $WORK/release /usr/lib/linos/" \
    --customize-hook="copy-in $WORK/linos-recovery.service /usr/lib/systemd/system/" \
    --customize-hook='ln -s /usr/lib/systemd/system/linos-recovery.service "$1/etc/systemd/system/multi-user.target.wants/linos-recovery.service"' \
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
