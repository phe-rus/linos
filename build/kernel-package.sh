#!/usr/bin/env bash
set -Eeuo pipefail
# Package a compiled, verified Linos kernel. Never install it on this host.
[[ $# -eq 2 ]] || { echo "Usage: $0 <kernel-artifact-dir> <empty-output-dir>" >&2; exit 2; }
SRC="$(realpath -- "$1")"
OUT="$(realpath -m -- "$2")"
[[ "$OUT" != "$SRC" && "$OUT" != "$SRC/"* ]] || exit 2
[[ ! -e "$OUT" || -z "$(ls -A "$OUT")" ]] || { echo "Nonempty output" >&2; exit 2; }
[[ -s "$SRC/provenance.txt" && -s "$SRC/SHA256SUMS" ]] || exit 2
ARCH="$(sed -n 's/^architecture=//p' "$SRC/provenance.txt")"
REL="$(sed -n 's/^kernel_release=//p' "$SRC/provenance.txt")"
VERSION="$(sed -n 's/^linos_kernel_version=//p' "$SRC/provenance.txt")"
case "$ARCH" in amd64|arm64) ;; *) echo "Unsupported architecture" >&2; exit 2;; esac
[[ "$REL" =~ ^[0-9][A-Za-z0-9.+~-]*-linos$ && "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
[[ -s "$SRC/boot/kernel" && -s "$SRC/boot/config-$REL" &&
   -s "$SRC/boot/System.map-$REL" && -d "$SRC/lib/modules/$REL" ]] || exit 1
(cd "$SRC" && sha256sum -c SHA256SUMS)
WORK="$(mktemp -d /tmp/linos-kernel-deb.XXXXXXXX)"
trap 'rm -rf -- "$WORK"' EXIT
PKG="$WORK/package"
mkdir -p "$PKG/DEBIAN" "$PKG/boot" "$PKG/usr/lib/modules/$REL" "$PKG/usr/share/doc/linos-kernel"
install -m 644 "$SRC/boot/kernel" "$PKG/boot/vmlinuz-$REL"
install -m 644 "$SRC/boot/config-$REL" "$PKG/boot/config-$REL"
install -m 644 "$SRC/boot/System.map-$REL" "$PKG/boot/System.map-$REL"
cp -a "$SRC/lib/modules/$REL/." "$PKG/usr/lib/modules/$REL/"
install -m 644 "$SRC/provenance.txt" "$PKG/usr/share/doc/linos-kernel/provenance.txt"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: linos-kernel
Version: $VERSION
Section: kernel
Priority: optional
Architecture: $ARCH
Maintainer: Pherus <pherus@pherus.org>
Depends: kmod, initramfs-tools
Description: Linos kernel $REL
 Linos kernel and corresponding modules for Linos operating systems.
CONTROL
cat > "$PKG/DEBIAN/postinst" <<POSTINST
#!/bin/sh
set -e
if [ "\$1" = configure ]; then
  depmod -a '$REL'
  if [ -e '/boot/initrd.img-$REL' ]; then
    update-initramfs -u -k '$REL'
  else
    update-initramfs -c -k '$REL'
  fi
fi
POSTINST
chmod 755 "$PKG/DEBIAN/postinst"
mkdir -p "$OUT"
FILE="$OUT/linos-kernel_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "$PKG" "$FILE"
[[ "$(dpkg-deb -f "$FILE" Package)" == linos-kernel &&
   "$(dpkg-deb -f "$FILE" Architecture)" == "$ARCH" ]] || exit 1
dpkg-deb --fsys-tarfile "$FILE" | tar -tf - | grep -Fx "./boot/vmlinuz-$REL" >/dev/null
printf '%s\n' "$REL" > "$OUT/kernel-release"
(cd "$OUT" && sha256sum "$(basename "$FILE")" > SHA256SUMS)
echo "Linos kernel package: $FILE"
