#!/usr/bin/env bash
set -Eeuo pipefail
export PATH="/usr/sbin:/sbin:$PATH"
export CARGO_BUILD_JOBS=1
[[ "${GITHUB_ACTIONS:-}" == true && "$(id -u)" == 0 ]] || { echo 'Layout 2 OS construction runs in CI as root.' >&2; exit 2; }
[[ $# == 4 ]] || { echo 'Usage: layout2-deployment.sh BASE_CANDIDATE BASE_SHA OUTPUT PRIVATE_DEVELOPMENT_KEY' >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
BASE="$(realpath "$1")" BASE_SHA="$2" OUTPUT="$(realpath -m "$3")" SIGNING_KEY="$(realpath "$4")"
[[ ! -e "$OUTPUT" && -s "$SIGNING_KEY" && "$BASE_SHA" =~ ^[a-f0-9]{40}$ ]] || exit 2
bash build/candidate.sh verify "$BASE" amd64 "$BASE_SHA"
[[ "$(sed -n 's/^mode=//p' "$BASE/kernel-manifest")" == linos ]] || exit 2
VERSION="$(sed -n 's/^release=//p' "$BASE/kernel-manifest")"
[[ "$VERSION" =~ ^[a-zA-Z0-9._+-]+$ ]] || exit 2
SOURCE_SHA="${GITHUB_SHA:?CI source identity}"
[[ "$SOURCE_SHA" =~ ^[a-f0-9]{40}$ ]] || exit 2
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' system/release/usr/lib/linos/release)"
SEQUENCE="$(sed -n 's/^LINOS_SEQUENCE=//p' system/release/usr/lib/linos/release)"
[[ "$BUILD_ID" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ && "$SEQUENCE" =~ ^[1-9][0-9]*$ ]] || exit 2
mkdir -p "$OUTPUT/trust" "$OUTPUT/deployment"
chmod 700 "$OUTPUT" "$OUTPUT/deployment"
WORK="$(mktemp -d /var/tmp/linos-layout2.XXXXXXXX)"
trap 'rm -rf -- "$WORK"' EXIT
STAGE="$WORK/root"
mkdir -p "$STAGE" "$WORK/deployment"
tar --extract --same-owner --preserve-permissions --file="$BASE/rootfs.tar" --directory="$STAGE" \
    --exclude='./dev/*' --exclude='./proc/*' --exclude='./sys/*' --exclude='./run/*'
[[ ! -e "$STAGE/var/lib/linos/owner" && ! -e "$STAGE/etc/linos/setup-complete" && ! -d "$STAGE/root/.ssh" ]] || { echo 'An enrolled root cannot become a generic deployment.' >&2; exit 2; }
[[ -z "$(find "$STAGE/home" -mindepth 1 -maxdepth 1 -print -quit)" ]] || exit 2
openssl pkey -in "$SIGNING_KEY" -pubout -out "$WORK/deployment.pem"
install -m 644 "$WORK/deployment.pem" "$OUTPUT/trust/deployment.pem"
install -d -m 755 "$STAGE/usr/lib/linos/trust"
install -m 644 "$OUTPUT/trust/deployment.pem" "$STAGE/usr/lib/linos/trust/deployment.pem"
for spec in 'null 1 3' 'zero 1 5' 'random 1 8' 'urandom 1 9'; do
    read -r name major minor <<< "$spec"
    [[ -e "$STAGE/dev/$name" ]] || mknod -m 666 "$STAGE/dev/$name" c "$major" "$minor"
done
cp -a --no-dereference "$STAGE/etc/resolv.conf" "$WORK/resolv.original"
cp --remove-destination /etc/resolv.conf "$STAGE/etc/resolv.conf"
chroot "$STAGE" apt-get update
chroot "$STAGE" env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends gdisk openssl util-linux
rm -f -- "$STAGE/etc/resolv.conf"
cp -a --no-dereference "$WORK/resolv.original" "$STAGE/etc/resolv.conf"
PACKAGE="$(bash build/package.sh --print-path)"
cp "$PACKAGE" "$STAGE/tmp/linos.deb"
chroot "$STAGE" dpkg --install /tmp/linos.deb
rm -f -- "$STAGE/tmp/linos.deb"
printf 'layout=2\n' > "$STAGE/usr/lib/linos/storage-layout"
printf '/dev/linos-system / ext4 ro,noload 0 0\n' > "$STAGE/etc/fstab"
mkdir -p "$STAGE/efi" "$STAGE/usr/local" "$STAGE/srv" "$STAGE/var/lib/apt"
[[ -f "$STAGE/var/lib/apt/extended_states" ]] || : > "$STAGE/var/lib/apt/extended_states"
ln -sfn /dev/null "$STAGE/etc/systemd/system/tmp.mount"
# Machine identity, entropy and SSH host credentials belong to the installed
# device, never to a generic image or CI's private signing key workspace.
: > "$STAGE/etc/machine-id"
rm -f -- "$STAGE"/etc/ssh/ssh_host_* "$STAGE"/etc/ssl/private/* "$STAGE/var/lib/systemd/random-seed"
chroot "$STAGE" update-initramfs -u -k "$VERSION"
cmp "$BASE/boot/kernel" "$STAGE/boot/vmlinuz-$VERSION"
grep -Fxq CONFIG_BLK_DEV_LOOP=y "$STAGE/boot/config-$VERSION"
lsinitramfs "$STAGE/boot/initrd.img-$VERSION" > "$WORK/initramfs-files"
for member in usr/lib/linos/linos-layout2 usr/lib/linos/trust/deployment.pem scripts/local-top/linos-layout2 scripts/local-bottom/linos-layout2; do
    grep -Fxq "$member" "$WORK/initramfs-files" || { echo "Missing initramfs member: $member" >&2; exit 1; }
done
TARGET_DIR="${LINOS_TARGET_DIR:?verified userspace output directory}"
TOOL="$TARGET_DIR/release/recovery"
"$TOOL" accounts-v1 "$STAGE/etc" > "$WORK/deployment/accounts"
printf 'kind=linos-services\nformat=1\ncount=0\n' > "$WORK/deployment/services"
printf 'kind=linos-applications\nformat=1\ncount=0\n' > "$WORK/deployment/applications"
printf 'kind=linos-extensions\nformat=1\ncount=0\n' > "$WORK/deployment/extensions"
chroot "$STAGE" dpkg-query -W -f='${db:Status-Abbrev}\t${binary:Package}\t${Version}\t${Architecture}\n' \
    | awk '$1 == "ii" {print $2 "\t" $3 "\t" $4}' | LC_ALL=C sort > "$WORK/deployment/inventory"
cp "$BASE/boot/kernel" "$WORK/deployment/kernel"
cp "$STAGE/boot/initrd.img-$VERSION" "$WORK/deployment/initramfs"
truncate -s 3G "$WORK/deployment/system.img"
mkfs.ext4 -F -q -L linos-system -d "$STAGE" "$WORK/deployment/system.img"
{
    printf 'kind=linos-deployment\nformat=3\norigin=ci\nbase_manifest_sha256=none\nplan_sha256=none\n'
    printf 'source_sha=%s\nbuild_id=%s\nrelease=%s\narchitecture=amd64\nhardware_profile=qemu-amd64-uefi\nkernel_release=%s\n' "$SOURCE_SHA" "$BUILD_ID" "$SEQUENCE" "$VERSION"
    printf 'config_schema_min=1\nconfig_schema_max=1\n'
    for field in system kernel initramfs inventory applications services accounts extensions; do
        name="$field"; [[ "$field" != system ]] || name=system.img
        printf '%s_bytes=%s\n%s_sha256=%s\n' "$field" "$(stat -c %s "$WORK/deployment/$name")" "$field" "$(sha256sum "$WORK/deployment/$name" | cut -d' ' -f1)"
    done
} > "$WORK/deployment/manifest"
(
    cd "$WORK/deployment"
    sha256sum manifest system.img kernel initramfs inventory applications services accounts extensions > SHA256SUMS
)
{ printf 'LINOS-DEPLOYMENT-V3\n'; cat "$WORK/deployment/SHA256SUMS"; } > "$WORK/signing-input"
openssl pkeyutl -sign -rawin -inkey "$SIGNING_KEY" -in "$WORK/signing-input" -out "$WORK/deployment/SHA256SUMS.sig"
ID="$(sha256sum "$WORK/deployment/manifest" | cut -d' ' -f1)"
mkdir -m 700 "$WORK/verified"
mv "$WORK/deployment" "$WORK/verified/$ID"
chmod 700 "$WORK/verified/$ID"
"$TOOL" check-deployment-v3 "$WORK/verified/$ID" "$WORK/deployment.pem"
mv "$WORK/verified/$ID" "$OUTPUT/deployment/$ID"
{
    printf 'kind=linos-development-candidate\nformat=1\nsource_sha=%s\nbase_source_sha=%s\ndeployment_id=%s\n' "$SOURCE_SHA" "$BASE_SHA" "$ID"
    printf 'package_sha256=%s\nkernel_sha256=%s\n' "$(sha256sum "$PACKAGE" | cut -d' ' -f1)" "$(sha256sum "$BASE/boot/kernel" | cut -d' ' -f1)"
    printf 'state=development\ninstallation=unqualified\n'
} > "$OUTPUT/candidate-manifest"
printf '%s\n' "$ID" > "$OUTPUT/deployment-id"
tar -cf "$ROOT/target/layout2-scan-rootfs.tar" -C "$STAGE" .
echo "Layout 2 signed deployment: $ID"
