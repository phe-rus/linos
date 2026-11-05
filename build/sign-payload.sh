#!/usr/bin/env bash
set -Eeuo pipefail

PAYLOAD_DIR="${1:?Usage: LINOS_PAYLOAD_SIGNING_KEY=/path/to/private.pem ./build/sign-payload.sh /path/to/payload}"
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
