#!/usr/bin/env bash
set -Eeuo pipefail

# Debian admin utilities may be absent from an unprivileged user PATH.
export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${LINOS_ARCH:-$(dpkg --print-architecture)}"
SUITE="${LINOS_SUITE:-trixie}"

RELEASE_FILE="packages/release/usr/lib/linos/release"
PACKAGE="$(./build/package.sh --print-path)"

BUILD_ID="$(
    sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$RELEASE_FILE"
)"

if [[ -z "$BUILD_ID" ]]; then
    echo "Missing Linos build identifier"
    exit 1
fi

if [[ ! -f "$PACKAGE" ]]; then
    echo "Missing Linos release package: $PACKAGE"
    exit 1
fi

PACKAGE_VERSION="$(dpkg-deb -f "$PACKAGE" Version)"

if [[ "$PACKAGE_VERSION" != "$(awk '/^Version:/ { print $2 }' packages/release/DEBIAN/control)" ]]; then
    echo "Release package version does not match control metadata: $PACKAGE_VERSION"
    exit 1
fi

PACKAGE_ARCH="$(dpkg-deb -f "$PACKAGE" Architecture)"
if [[ "$PACKAGE_ARCH" != "$ARCH" ]]; then
    echo "Package architecture $PACKAGE_ARCH does not match image architecture $ARCH" >&2
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

# mmdebstrap may run in an unshared user namespace. ChromeOS home
# directories are not always traversable from that namespace, so stage
# local inputs under /tmp with read/traverse access.
STAGING_DIR="$(mktemp -d /tmp/linos-image.XXXXXXXX)"
STAGED_PACKAGE="$STAGING_DIR/$(basename "$PACKAGE")"
STAGED_ISSUE="$STAGING_DIR/issue"
STAGED_MOTD="$STAGING_DIR/motd"
STAGED_OS_RELEASE="$STAGING_DIR/os-release"
trap 'rm -r -- "$STAGING_DIR"' EXIT
chmod 755 "$STAGING_DIR"
install -m 644 "$PACKAGE" "$STAGED_PACKAGE"
install -m 644 system/base/identity/issue "$STAGED_ISSUE"
install -m 644 system/base/identity/motd "$STAGED_MOTD"
install -m 644 system/base/identity/os-release "$STAGED_OS_RELEASE"

mkdir -p "$BOOT_DIR"

echo "Linos Image Builder"
echo "===================="
echo "Architecture : $ARCH"
echo "Debian base  : $SUITE"
echo "Build        : $BUILD_ID"
echo "Kernel       : $KERNEL_PACKAGE"

KEYRING_ARGS=()
if [[ -n "${LINOS_KEYRING:-}" ]]; then
    if [[ ! -r "$LINOS_KEYRING" ]]; then
        echo "Configured Debian archive keyring is not readable: $LINOS_KEYRING" >&2
        exit 1
    fi
    install -m 644 "$LINOS_KEYRING" "$STAGING_DIR/debian-keyring.gpg"
    KEYRING_ARGS+=(--keyring="$STAGING_DIR/debian-keyring.gpg")
fi

mmdebstrap \
    "${KEYRING_ARGS[@]}" \
    --variant=minbase \
    --architectures="$ARCH" \
    --hook-dir=/usr/share/mmdebstrap/hooks/file-mirror-automount \
    --include=systemd-sysv,dbus,iproute2,procps,ca-certificates,openssh-server,passwd,sudo,initramfs-tools,e2fsprogs,locales,tzdata,keyboard-configuration \
    --include="$KERNEL_PACKAGE" \
    --include="$STAGED_PACKAGE" \
    --customize-hook='rm -f "$1/etc/hostname"' \
    --customize-hook="copy-in $STAGED_ISSUE /etc/" \
    --customize-hook="copy-in $STAGED_MOTD /etc/" \
    --customize-hook='rm -f "$1/etc/os-release"' \
    --customize-hook="copy-in $STAGED_OS_RELEASE /etc/" \
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

ROOTFS_STAGE="$STAGING_DIR/rootfs"
mkdir -p "$ROOTFS_STAGE"
if ! command -v fakeroot >/dev/null 2>&1; then
    echo "fakeroot is required to preserve root ownership in the disk image" >&2
    exit 1
fi

# Never use host-user ownership in the guest image. Extract under fakeroot,
# then reload its recorded metadata while mkfs populates the ext4 image.
FAKE_STATE="$STAGING_DIR/fakeroot.state"
fakeroot -s "$FAKE_STATE" -- \
    tar --same-owner --exclude="./dev/*" -xf "$ROOTFS" -C "$ROOTFS_STAGE"

fakeroot -i "$FAKE_STATE" -- mkfs.ext4 \
    -F \
    -q \
    -L linos-root \
    -d "$ROOTFS_STAGE" \
    "$DISK"

# A disk with incorrectly owned setuid tools is unbootably insecure.
SUDO_METADATA="$(debugfs -R "stat /usr/bin/sudo" "$DISK" 2>/dev/null)"
if ! grep -Eq "User:[[:space:]]*0([[:space:]]|$)" <<<"$SUDO_METADATA"; then
    echo "Image ownership validation failed: /usr/bin/sudo must belong to root" >&2
    exit 1
fi

echo
echo "Build completed."
echo "Kernel version: $KERNEL_VERSION"
echo "Disk image: $DISK"

sha256sum \
    "$BOOT_DIR/kernel" \
    "$BOOT_DIR/initramfs" \
    "$DISK"
