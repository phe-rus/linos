#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${linos13_ARCH:-$(dpkg --print-architecture)}"
SUITE="${linos13_SUITE:-trixie}"

RELEASE_FILE="packages/release/usr/lib/linos13/release"
PACKAGE="packages/dist/linos13-release_1.0.0_all.deb"

BUILD_ID="$(
    sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$RELEASE_FILE"
)"

if [[ -z "$BUILD_ID" ]]; then
    echo "Missing linos13 build identifier"
    exit 1
fi

if [[ ! -f "$PACKAGE" ]]; then
    echo "Missing linos13 release package: $PACKAGE"
    exit 1
fi

PACKAGE_VERSION="$(dpkg-deb -f "$PACKAGE" Version)"

if [[ "$PACKAGE_VERSION" != "1.0.0" ]]; then
    echo "Unexpected release package version: $PACKAGE_VERSION"
    exit 1
fi

KERNEL_PACKAGE="linux-image-${ARCH}"

BUILD_DIR="images/server/builds/${ARCH}/${BUILD_ID}"
ROOTFS="${BUILD_DIR}/rootfs.tar"
DISK="${BUILD_DIR}/disk.ext4"
BOOT_DIR="${BUILD_DIR}/boot"

if [[ -e "$BUILD_DIR" ]]; then
    echo "Build already exists: $BUILD_DIR"
    echo "Existing builds are never overwritten automatically."
    exit 1
fi

mkdir -p "$BOOT_DIR"

echo "linos13 Image Builder"
echo "===================="
echo "Architecture : $ARCH"
echo "Debian base  : $SUITE"
echo "Build        : $BUILD_ID"
echo "Kernel       : $KERNEL_PACKAGE"

mmdebstrap \
    --variant=minbase \
    --architectures="$ARCH" \
    --hook-dir=/usr/share/mmdebstrap/hooks/file-mirror-automount \
    --include=systemd-sysv,dbus,iproute2,procps,ca-certificates,openssh-server,initramfs-tools,e2fsprogs \
    --include="$KERNEL_PACKAGE" \
    --include="$ROOT/$PACKAGE" \
    --customize-hook='rm -f "$1/etc/hostname"' \
    --customize-hook="copy-in $ROOT/system/base/identity/issue /etc/" \
    "$SUITE" \
    "$ROOT/$ROOTFS"

KERNEL_MEMBER="$(
    tar -tf "$ROOTFS" |
        awk '
            /^\.\/boot\/vmlinuz-[^/]+$/ && !found {
                print
                found = 1
            }
        '
)"

if [[ -z "$KERNEL_MEMBER" ]]; then
    echo "No kernel found inside root filesystem."
    exit 1
fi

KERNEL_VERSION="${KERNEL_MEMBER#./boot/vmlinuz-}"

tar -xOf "$ROOTFS" \
    "$KERNEL_MEMBER" \
    > "$BOOT_DIR/kernel"

tar -xOf "$ROOTFS" \
    "./boot/initrd.img-${KERNEL_VERSION}" \
    > "$BOOT_DIR/initramfs"

truncate -s 3G "$DISK"

mkfs.ext4 \
    -F \
    -q \
    -L linos13-root \
    -d "$ROOTFS" \
    "$DISK"

echo
echo "Build completed."
echo "Kernel version: $KERNEL_VERSION"
echo "Disk image: $DISK"

sha256sum \
    "$BOOT_DIR/kernel" \
    "$BOOT_DIR/initramfs" \
    "$DISK"
