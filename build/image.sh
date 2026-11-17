#!/usr/bin/env bash
set -Eeuo pipefail

# Build system and image operations share one entry point.
assemble_image() (
set -Eeuo pipefail

# Debian admin utilities may be absent from an unprivileged user PATH.
export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${LINOS_ARCH:-$(dpkg --print-architecture)}"
SUITE="${LINOS_SUITE:-trixie}"

RELEASE_FILE="system/release/usr/lib/linos/release"
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

if [[ "$PACKAGE_VERSION" != "$(awk '/^Version:/ { print $2 }' system/release/DEBIAN/control)" ]]; then
    echo "Release package version does not match control metadata: $PACKAGE_VERSION"
    exit 1
fi

# Fail before assembling an image if the .deb is stale or incomplete.
# This checks the actual package bytes, not only its unchanged 1.0.0 version.
verify_package_entry() {
    local name="$1"
    if ! dpkg-deb --fsys-tarfile "$PACKAGE" | tar -tf - | grep -Fx "./$name" >/dev/null; then
        printf 'Linos package is missing %s; rebuild with ./build/package.sh\n' "$name" >&2
        exit 1
    fi
}
for entry in \
    usr/bin/linos \
    usr/lib/linos/linos-login \
    usr/lib/systemd/system/serial-getty@ttyS0.service.d/linos-login.conf \
    usr/lib/systemd/system/getty@tty1.service.d/linos-login.conf; do
    verify_package_entry "$entry"
done
if ! dpkg-deb --fsys-tarfile "$PACKAGE" | tar -xOf - ./usr/bin/linos | grep -aF 'Type / for commands or enter a Linux command' >/dev/null; then
    echo "Linos package contains an outdated server TUI binary" >&2
    exit 1
fi

PACKAGE_ARCH="$(dpkg-deb -f "$PACKAGE" Architecture)"
if [[ "$PACKAGE_ARCH" != "$ARCH" ]]; then
    echo "Package architecture $PACKAGE_ARCH does not match image architecture $ARCH" >&2
    exit 1
fi

# Normal Linos Server image input: a prebuilt, architecture-specific kernel package.
# Compatibility mode must be requested explicitly until all release artifacts ship.
KERNEL_MODE="${LINOS_KERNEL_MODE:-linos}"
KERNEL_INPUT=""
if [[ "$KERNEL_MODE" == linos ]]; then
    source kernel/source.conf
    KERNEL_DEB="${LINOS_KERNEL_PACKAGE:-kernel/dist/linos-kernel_${LINOS_KERNEL_VERSION}_${ARCH}.deb}"
    [[ -s "$KERNEL_DEB" ]] || { echo "Missing Linos kernel package: $KERNEL_DEB (set LINOS_KERNEL_MODE=compat for legacy image)" >&2; exit 2; }
    [[ "$(dpkg-deb -f "$KERNEL_DEB" Package)" == linos-kernel && "$(dpkg-deb -f "$KERNEL_DEB" Architecture)" == "$ARCH" && "$(dpkg-deb -f "$KERNEL_DEB" Version)" == "$LINOS_KERNEL_VERSION" ]] || { echo "Kernel package metadata mismatch" >&2; exit 1; }
    KERNEL_MEMBER="$(dpkg-deb --fsys-tarfile "$KERNEL_DEB" | tar -tf - | grep -E "^\./boot/vmlinuz-[^/]+-linos$" || true)"
    [[ -n "$KERNEL_MEMBER" && "$(wc -l <<<"$KERNEL_MEMBER")" -eq 1 ]] || { echo "Linos package must contain one kernel image" >&2; exit 1; }
    KERNEL_RELEASE="${KERNEL_MEMBER#./boot/vmlinuz-}"
    dpkg-deb --fsys-tarfile "$KERNEL_DEB" | tar -tf - | grep -Fx "./usr/lib/modules/$KERNEL_RELEASE/" >/dev/null || { echo "Matching Linos modules absent" >&2; exit 1; }
    KERNEL_PACKAGE=linos-kernel
elif [[ "$KERNEL_MODE" == compat ]]; then
    KERNEL_PACKAGE="linux-image-${ARCH}"
else
    echo "LINOS_KERNEL_MODE must be linos or compat" >&2; exit 2
fi

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
if [[ "$KERNEL_MODE" == linos ]]; then
    KERNEL_INPUT="$STAGING_DIR/$(basename "$KERNEL_DEB")"
    install -m 644 "$KERNEL_DEB" "$KERNEL_INPUT"
fi
install -m 644 system/identity/issue "$STAGED_ISSUE"
install -m 644 system/identity/motd "$STAGED_MOTD"
install -m 644 system/identity/os-release "$STAGED_OS_RELEASE"

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
    --include="${KERNEL_INPUT:-$KERNEL_PACKAGE}" \
    --include="$STAGED_PACKAGE" \
    --customize-hook='rm -f "$1/etc/hostname"' \
    --customize-hook="copy-in $STAGED_ISSUE /etc/" \
    --customize-hook="copy-in $STAGED_MOTD /etc/" \
    --customize-hook='rm -f "$1/etc/os-release"' \
    --customize-hook="copy-in $STAGED_OS_RELEASE /etc/" \
    "$SUITE" \
    "$ROOT/$ROOTFS"

# Verify what mmdebstrap actually installed before producing the disk.
# The Debian package version alone does not establish code freshness.
verify_rootfs_entry() {
    local entry="$1"
    if ! tar -tf "$ROOTFS" | grep -Fx "./$entry" >/dev/null; then
        printf 'Incomplete rootfs: missing %s\n' "$entry" >&2
        exit 1
    fi
}
for entry in \
    usr/bin/linos \
    usr/lib/linos/linos-login \
    usr/lib/systemd/system/serial-getty@ttyS0.service.d/linos-login.conf \
    usr/lib/systemd/system/getty@tty1.service.d/linos-login.conf; do
    verify_rootfs_entry "$entry"
done

if ! tar -xOf "$ROOTFS" ./usr/bin/linos | grep -aF 'Type / for commands or enter a Linux command' >/dev/null; then
    echo "Incomplete rootfs: Linos TUI is not the expected build" >&2
    exit 1
fi
for entry in usr/bin/linos usr/lib/linos/linos-login; do
    package_hash="$(dpkg-deb --fsys-tarfile "$PACKAGE" | tar -xOf - "./$entry" | sha256sum | cut -d' ' -f1)"
    rootfs_hash="$(tar -xOf "$ROOTFS" "./$entry" | sha256sum | cut -d' ' -f1)"
    if [[ "$package_hash" != "$rootfs_hash" ]]; then
        echo "Incomplete rootfs: $entry differs from the packaged executable" >&2
        exit 1
    fi
done

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
if [[ "$KERNEL_MODE" == linos ]]; then
    [[ "$KERNEL_VERSION" == "$KERNEL_RELEASE" ]] || { echo "Rootfs selected wrong kernel" >&2; exit 1; }
    tar -tf "$ROOTFS" | grep -Fx "./usr/lib/modules/$KERNEL_RELEASE/" >/dev/null || { echo "Missing Linos modules in rootfs" >&2; exit 1; }
    tar -tf "$ROOTFS" | grep -Fx "./boot/initrd.img-$KERNEL_RELEASE" >/dev/null || { echo "Linos initramfs not generated by package" >&2; exit 1; }
    from_deb="$(dpkg-deb --fsys-tarfile "$KERNEL_DEB" | tar -xOf - "./boot/vmlinuz-$KERNEL_RELEASE" | sha256sum | cut -d" " -f1)"
    from_rootfs="$(tar -xOf "$ROOTFS" "$KERNEL_MEMBER" | sha256sum | cut -d" " -f1)"
    [[ "$from_deb" == "$from_rootfs" ]] || { echo "Rootfs kernel differs from package" >&2; exit 1; }
fi

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

# Optional Linos kernel candidate: use the same verified OS rootfs, but
# produce an independently bootable image instead of altering the fallback.
# The default image and its Debian kernel remain byte-for-byte unchanged.
if [[ -n "${LINOS_KERNEL_BUILD_DIR:-}" ]]; then
    [[ "$KERNEL_MODE" == compat ]] || { echo "Legacy kernel directory requires compatibility image" >&2; exit 2; }
    CANDIDATE_SOURCE="$(realpath -- "$LINOS_KERNEL_BUILD_DIR")"
    [[ -s "$CANDIDATE_SOURCE/provenance.txt" && -s "$CANDIDATE_SOURCE/.config" ]] || {
        echo "Linos kernel candidate is missing provenance or configuration" >&2
        exit 2
    }
    CANDIDATE_ARCH="$(sed -n 's/^architecture=//p' "$CANDIDATE_SOURCE/provenance.txt")"
    CANDIDATE_RELEASE="$(sed -n 's/^kernel_release=//p' "$CANDIDATE_SOURCE/provenance.txt")"
    [[ "$CANDIDATE_ARCH" == "$ARCH" && "$CANDIDATE_RELEASE" == *-linos ]] || {
        echo "Linos kernel candidate does not match this Server architecture" >&2
        exit 2
    }
    CANDIDATE_DEST="$ROOT/$BUILD_DIR/linos-kernel"
    echo "Assembling isolated Linos $CANDIDATE_RELEASE Server image"
    bash "$ROOT/build/kernel.sh" server-image "$ROOT/$ROOTFS" "$CANDIDATE_SOURCE" "$CANDIDATE_DEST"
    [[ -s "$CANDIDATE_DEST/kernel" && -s "$CANDIDATE_DEST/initramfs" &&
       -s "$CANDIDATE_DEST/disk.ext4" && -s "$CANDIDATE_DEST/rootfs.tar" ]] || {
        echo "Linos kernel Server image was not assembled" >&2
        exit 1
    }
    (
        cd "$ROOT"
        sha256sum -c "$BUILD_DIR/linos-kernel/SHA256SUMS"
    )
    echo "Linos kernel candidate: $CANDIDATE_DEST"
    if [[ "${LINOS_KERNEL_PRIMARY:-1}" == 1 ]]; then
        # Promote the complete, matching Linos artifact set. Retain the
        # previous image as an explicit fallback, not the default boot.
        # Relative links keep the build relocatable and avoid duplicate
        # multi-gigabyte disk copies.
        FALLBACK="$BUILD_DIR/fallback"
        mkdir -p "$FALLBACK/boot"
        mv -- "$BOOT_DIR/kernel" "$FALLBACK/boot/kernel"
        mv -- "$BOOT_DIR/initramfs" "$FALLBACK/boot/initramfs"
        mv -- "$DISK" "$FALLBACK/disk.ext4"
        mv -- "$ROOTFS" "$FALLBACK/rootfs.tar"
        ln -s ../linos-kernel/kernel "$BOOT_DIR/kernel"
        ln -s ../linos-kernel/initramfs "$BOOT_DIR/initramfs"
        ln -s linos-kernel/disk.ext4 "$DISK"
        ln -s linos-kernel/rootfs.tar "$ROOTFS"
        printf '%s\n' "$CANDIDATE_RELEASE" > "$BUILD_DIR/kernel-release"
        [[ "$(readlink -f "$DISK")" == "$(readlink -f "$CANDIDATE_DEST/disk.ext4")" ]] || exit 1
        echo "Linos is the primary Server kernel: $CANDIDATE_RELEASE"
        echo "Previous boot files retained under $FALLBACK"
    else
        [[ "${LINOS_KERNEL_PRIMARY}" == 0 ]] || {
            echo "LINOS_KERNEL_PRIMARY must be 0 or 1" >&2
            exit 2
        }
        echo "Linos remains an isolated candidate; default image unchanged."
    fi
fi

# Record the kernel used by the final bootable image, not a build-time
# aspiration. This is consumed by the interactive QEMU launcher.
BOOTED_KERNEL="$(readlink -f -- "$BOOT_DIR/kernel")"
BOOTED_INITRAMFS="$(readlink -f -- "$BOOT_DIR/initramfs")"
[[ -s "$BOOTED_KERNEL" && -s "$BOOTED_INITRAMFS" ]] || {
    echo "Missing final kernel or initramfs" >&2
    exit 1
}
FINAL_KERNEL_RELEASE="$KERNEL_VERSION"
FINAL_KERNEL_MODE="$KERNEL_MODE"
if [[ -s "$BUILD_DIR/kernel-release" ]]; then
    FINAL_KERNEL_RELEASE="$(cat "$BUILD_DIR/kernel-release")"
    FINAL_KERNEL_MODE=linos
fi
if [[ "$FINAL_KERNEL_MODE" == linos && "$FINAL_KERNEL_RELEASE" != *-linos ]]; then
    echo "Linos image has a non-Linos kernel release: $FINAL_KERNEL_RELEASE" >&2
    exit 1
fi
{
    printf 'mode=%s\n' "$FINAL_KERNEL_MODE"
    printf 'release=%s\n' "$FINAL_KERNEL_RELEASE"
    printf 'kernel_sha256=%s\n' "$(sha256sum "$BOOTED_KERNEL" | cut -d' ' -f1)"
    printf 'initramfs_sha256=%s\n' "$(sha256sum "$BOOTED_INITRAMFS" | cut -d' ' -f1)"
} > "$BUILD_DIR/kernel-manifest"
echo "Image kernel policy: $FINAL_KERNEL_MODE ($FINAL_KERNEL_RELEASE)"

)

minimal_rootfs() (
set -Eeuo pipefail

PROJECT_ROOT="$(
    cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." &&
    pwd
)"

ARCH="${LINOS_ARCH:-$(dpkg --print-architecture)}"
SUITE="${LINOS_SUITE:-trixie}"

OUTPUT_DIR="$PROJECT_ROOT/images/minimal"
OUTPUT="$OUTPUT_DIR/linos-rootfs-${ARCH}.tar"

PACKAGES=(
    systemd-sysv
    dbus
    iproute2
    procps
    ca-certificates
    openssh-server
)

if [[ -e "$OUTPUT" ]]; then
    printf 'Existing image: %s\n' "$OUTPUT"
    printf 'Move it aside before rebuilding.\n'
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

PACKAGE_LIST="$(
    IFS=,
    printf '%s' "${PACKAGES[*]}"
)"

printf 'Linos Linux Build\n'
printf 'Architecture: %s\n' "$ARCH"
printf 'Debian suite: %s\n' "$SUITE"
printf 'Output: %s\n' "$OUTPUT"

mmdebstrap \
    --variant=minbase \
    --architectures="$ARCH" \
    --include="$PACKAGE_LIST" \
    "$SUITE" \
    "$OUTPUT"

printf '\nBuild completed successfully.\n'
sha256sum "$OUTPUT"
)

next_build() (
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="${1:-${LINOS_ARCH:-$(dpkg --print-architecture)}}"
case "$ARCH" in
    amd64|arm64) ;;
    *) printf 'Unsupported Linos architecture: %s\n' "$ARCH" >&2; exit 2 ;;
esac

RELEASE="system/release/usr/lib/linos/release"
[[ -f "$RELEASE" ]] || { echo "Missing release metadata: $RELEASE" >&2; exit 1; }

DATE="$(date +%Y%m%d)"
[[ "$DATE" =~ ^[0-9]{8}$ ]] || { echo "Invalid system date" >&2; exit 1; }

# Preserve old builds. An ID must be unused for both architectures to avoid
# collisions with CI or cross-device development.
number=1
while :; do
    BUILD_ID="$DATE.$number"
    if [[ ! -e "images/server/builds/amd64/$BUILD_ID" &&
          ! -e "images/server/builds/arm64/$BUILD_ID" &&
          ! -e "images/recovery/builds/amd64/$BUILD_ID" &&
          ! -e "images/recovery/builds/arm64/$BUILD_ID" ]]; then
        break
    fi
    ((number += 1))
done

current="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$RELEASE")"
[[ -n "$current" ]] || { echo "Missing BUILD_ID in $RELEASE" >&2; exit 1; }

# GNU sed and date are already part of the Debian development environment.
sed -i "s/^BUILD_ID=\"[^\"]*\"$/BUILD_ID=\"$BUILD_ID\"/" "$RELEASE"
printf 'Linos 13 %s build: %s\n' "$ARCH" "$BUILD_ID"
printf 'Next: ./build/package.sh && ./build/image.sh && ./tests/server.sh smoke %s\n' "$ARCH"
)

case "${1:-}" in
  rootfs) shift; minimal_rootfs "$@" ;;
  next) shift; next_build "$@" ;;
  image) shift; assemble_image "$@" ;;
  *) assemble_image "$@" ;;
esac
