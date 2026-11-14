#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CONTROL="packages/release/DEBIAN/control"
RELEASE="packages/release/usr/lib/linos/release"

PACKAGE_VERSION="$(awk '/^Version:/ { print $2 }' "$CONTROL")"
PRODUCT_VERSION="$(sed -n 's/^VERSION_ID="\([^"]*\)"$/\1/p' "$RELEASE")"
BUILD_CHANNEL="$(sed -n 's/^BUILD_CHANNEL=\(.*\)$/\1/p' "$RELEASE")"

case "$BUILD_CHANNEL" in
    development|dev) CHANNEL=dev ;;
    alpha|beta|release) CHANNEL="$BUILD_CHANNEL" ;;
    *) echo "Unsupported Linos build channel: $BUILD_CHANNEL" >&2; exit 2 ;;
esac

if [[ ! "$PRODUCT_VERSION" =~ ^[0-9]+$ ]] ||
   [[ ! "$PACKAGE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Invalid release or package version" >&2
    exit 2
fi

OUTPUT="packages/dist/linos${PRODUCT_VERSION}_${CHANNEL}_v${PACKAGE_VERSION}.deb"

if [[ "${1:-}" == "--print-path" ]]; then
    printf '%s\n' "$OUTPUT"
    exit 0
fi

if [[ $# -ne 0 ]]; then
    echo "Usage: ./build/package.sh [--print-path]" >&2
    exit 2
fi

# The initializer is a native executable, so this is an architecture-specific
# package. Reject cross-builds until a dedicated cross-compilation toolchain exists.
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
case "$HOST_TRIPLE" in
    x86_64-*) ARCH=amd64 ;;
    aarch64-*) ARCH=arm64 ;;
    *) echo "Unsupported Rust host: $HOST_TRIPLE" >&2; exit 2 ;;
esac

if [[ "${LINOS_ARCH:-$ARCH}" != "$ARCH" ]]; then
    echo "Host architecture and requested image architecture differ; cross-compilation is not configured" >&2
    exit 2
fi

cargo build --release -p init -p enroll -p linos-console

STAGING_DIR="$(mktemp -d /tmp/linos-package.XXXXXXXX)"
trap 'rm -rf -- "$STAGING_DIR"' EXIT
mkdir -p "$STAGING_DIR/package"
cp -a packages/release/. "$STAGING_DIR/package/"

sed -i "s/^Architecture: all$/Architecture: $ARCH/" "$STAGING_DIR/package/DEBIAN/control"
install -m 755 target/release/init "$STAGING_DIR/package/usr/lib/linos/linos-init"
install -m 755 target/release/enroll "$STAGING_DIR/package/usr/lib/linos/linos-enroll"
mkdir -p "$STAGING_DIR/package/usr/bin"
install -m 755 target/release/linos "$STAGING_DIR/package/usr/bin/linos"
mkdir -p "$STAGING_DIR/package/etc/profile.d"
install -m 644 system/base/identity/linos-welcome.sh "$STAGING_DIR/package/etc/profile.d/linos-welcome.sh"

UNIT_DIR="$STAGING_DIR/package/usr/lib/systemd/system"
WANTS_DIR="$STAGING_DIR/package/etc/systemd/system/multi-user.target.wants"
mkdir -p "$UNIT_DIR" "$WANTS_DIR"
install -m 644 system/services/linos-first-boot.service "$UNIT_DIR/linos-first-boot.service"
install -m 644 system/services/linos-enroll.service "$UNIT_DIR/linos-enroll.service"

# SSH remains unavailable until local enrollment creates an owner record.
for unit in ssh.service ssh.socket; do
    DROPIN_DIR="$UNIT_DIR/$unit.d"
    mkdir -p "$DROPIN_DIR"
    install -m 644 system/services/ssh-enrollment.conf "$DROPIN_DIR/linos-enrollment.conf"
done
ln -s /usr/lib/systemd/system/linos-first-boot.service "$WANTS_DIR/linos-first-boot.service"
# A getty must not compete for the physical console while enrollment owns it.
for unit in getty@tty1.service serial-getty@ttyS0.service serial-getty@ttyAMA0.service; do
    DROPIN_DIR="$UNIT_DIR/$unit.d"
    mkdir -p "$DROPIN_DIR"
    install -m 644 system/services/linos-getty-after-enrollment.conf "$DROPIN_DIR/linos-enrollment-order.conf"
done
ln -s /usr/lib/systemd/system/linos-enroll.service "$WANTS_DIR/linos-enroll.service"

mkdir -p packages/dist
dpkg-deb --root-owner-group --build "$STAGING_DIR/package" "$OUTPUT"
