#!/usr/bin/env bash
set -Eeuo pipefail
# Real firmware qualification of the Rust image backend. No target arguments,
# mounts, loop devices or physical disks. All output is freshly created here.
export PATH="/usr/sbin:/sbin:$PATH"
[[ $# -eq 0 ]] || { echo 'Usage: tests/update.sh (no arguments)' >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
for utility in openssl sha256sum sgdisk mkfs.vfat mkfs.ext4 debugfs e2fsck fakeroot mmd mcopy mdir qemu-system-x86_64 timeout; do
    command -v "$utility" >/dev/null || { echo "Missing $utility" >&2; exit 2; }
done
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' system/release/usr/lib/linos/release)"
SOURCE="images/server/builds/amd64/$BUILD_ID"
SYSTEM_IMAGE="${LINOS_UPDATE_ENROLLED_DISK:-$SOURCE/disk.ext4}"
[[ -f "$SYSTEM_IMAGE" && ! -L "$SYSTEM_IMAGE" ]] || { echo 'Expected a regular disposable system image' >&2; exit 2; }
for artifact in disk.ext4 boot/kernel boot/initramfs; do
    [[ -s "$SOURCE/$artifact" ]] || { echo "Missing Server artifact: $SOURCE/$artifact" >&2; exit 2; }
done
CODE=/usr/share/OVMF/OVMF_CODE_4M.fd
VARS=/usr/share/OVMF/OVMF_VARS_4M.fd
BOOTLOADER=/usr/lib/systemd/boot/efi/systemd-bootx64.efi
for artifact in "$CODE" "$VARS" "$BOOTLOADER"; do [[ -r "$artifact" ]] || exit 2; done
mkdir -p target
WORK="$(mktemp -d "$ROOT/target/update-qemu.XXXXXXXX")"
PID=''
cleanup() {
    if [[ -n "$PID" ]]; then kill -TERM "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi
    local logs="$ROOT/target/update-test-logs/$(basename "$WORK")"
    mkdir -p "$logs"
    for log in "$WORK"/*.log; do [[ ! -f "$log" ]] || cp "$log" "$logs/"; done
    if [[ "${LINOS_KEEP_UPDATE_TEST:-0}" == 1 ]]; then echo "Update test artifacts: $WORK"; else rm -r -- "$WORK"; fi
}
trap cleanup EXIT
cargo build --locked --release --bin init --bin linos-update-image --bin linos-update-health
# Ask Cargo for its actual target directory, since this machine shares its cache.
CARGO_OUTPUT="$(cargo metadata --no-deps --format-version 1)"
# This metadata string contains only an absolute Cargo output path.
TARGET_DIR="$(sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' <<<"$CARGO_OUTPUT")"
[[ -n "$TARGET_DIR" && -x "$TARGET_DIR/release/linos-update-image" ]] || exit 2
TOOL="$TARGET_DIR/release/linos-update-image"
HEALTH="$TARGET_DIR/release/linos-update-health"
INIT="$TARGET_DIR/release/init"
DISK="$WORK/installed.raw"
truncate -s 16G "$DISK"
# Same provisional five-partition geometry as the installation test.
sgdisk --clear \
    --new=1:2048:+512M --typecode=1:ef00 --change-name=1:linos-efi \
    --new=2:0:+2G --typecode=2:8300 --change-name=2:linos-recovery \
    --new=3:0:+4G --typecode=3:8300 --change-name=3:linos-system-a \
    --new=4:0:+4G --typecode=4:8300 --change-name=4:linos-system-b \
    --new=5:0:-1M --typecode=5:8300 --change-name=5:linos-data "$DISK" >/dev/null
sgdisk --verify "$DISK" | grep -q 'No problems found'
part_field() { sgdisk --info="$1" "$DISK" | sed -n "$2"; }
ROOT_A="$(part_field 3 's/^Partition unique GUID: //p' | tr '[:upper:]' '[:lower:]')"
ROOT_B="$(part_field 4 's/^Partition unique GUID: //p' | tr '[:upper:]' '[:lower:]')"
DATA_UUID="$(part_field 5 's/^Partition unique GUID: //p' | tr '[:upper:]' '[:lower:]')"
ESP_UUID="$(part_field 1 's/^Partition unique GUID: //p' | tr '[:upper:]' '[:lower:]')"
ESP_START="$(part_field 1 's/^First sector: \([0-9]*\).*$/\1/p')"
A_START="$(part_field 3 's/^First sector: \([0-9]*\).*$/\1/p')"
B_START="$(part_field 4 's/^First sector: \([0-9]*\).*$/\1/p')"
DATA_START="$(part_field 5 's/^First sector: \([0-9]*\).*$/\1/p')"
"$TOOL" layout "$ROOT_A" "$ROOT_B" "$DATA_UUID" "$ESP_UUID" > "$WORK/update-layout"
printf '%s\n' \
    "PARTUUID=$DATA_UUID /var/lib/linos ext4 rw,nosuid,nodev,noexec 0 2" \
    "PARTUUID=$ESP_UUID /efi vfat rw,nosuid,nodev,noexec,umask=0077 0 2" > "$WORK/fstab"
# Current images include Debian's helper. Older local compatibility artifacts
# may need a separately extracted matching native package, never a host install.
debugfs -R "dump /usr/lib/systemd/systemd-bless-boot $WORK/systemd-bless-boot" "$SOURCE/disk.ext4" >/dev/null 2>&1
if [[ ! -s "$WORK/systemd-bless-boot" ]]; then
    NATIVE="$ROOT/target/update-native-tools/extracted/usr/lib/systemd/systemd-bless-boot"
    [[ -s "$NATIVE" ]] || { echo 'Source image lacks the blessing helper; rebuild the image or extract its matching Debian systemd-boot package under target/update-native-tools.' >&2; exit 2; }
    cp "$NATIVE" "$WORK/systemd-bless-boot"
fi
openssl genpkey -algorithm ED25519 -out "$WORK/private.pem" 2>/dev/null
openssl pkey -in "$WORK/private.pem" -pubout -out "$WORK/public.pem" 2>/dev/null
make_payload() {
    local path="$1" release="$2"
    mkdir "$path"
    cp --reflink=auto --sparse=always "$SYSTEM_IMAGE" "$path/system.img"
    cp "$SOURCE/boot/kernel" "$path/kernel"
    cp "$SOURCE/boot/initramfs" "$path/initramfs"
    cat > "$WORK/inject-guest.commands" <<COMMANDS
mkdir /efi
mkdir /etc/systemd/system/linos-first-boot.service.d
write $WORK/fstab /etc/fstab
write $WORK/update-layout /usr/lib/linos/update-layout
rm /usr/lib/linos/linos-init
write $INIT /usr/lib/linos/linos-init
set_inode_field /usr/lib/linos/linos-init mode 0100755
write $HEALTH /usr/lib/linos/linos-update-health
set_inode_field /usr/lib/linos/linos-update-health mode 0100755
write $WORK/systemd-bless-boot /usr/lib/systemd/systemd-bless-boot
set_inode_field /usr/lib/systemd/systemd-bless-boot mode 0100755
write $ROOT/system/services/linos-update-health.service /etc/systemd/system/linos-update-health.service
write $ROOT/system/services/linos-first-boot-data.conf /etc/systemd/system/linos-first-boot.service.d/data.conf
symlink /etc/systemd/system/multi-user.target.wants/linos-update-health.service /etc/systemd/system/linos-update-health.service
COMMANDS
    # Replace the empty source fstab in a disposable copy, never in SOURCE.
    debugfs -w -R 'rm /etc/fstab' "$path/system.img" >/dev/null 2>&1
    debugfs -w -f "$WORK/inject-guest.commands" "$path/system.img" > "$WORK/inject-$release.log" 2>&1
    printf '%s\n' 'format=2' 'edition=server' 'architecture=amd64' \
        'hardware_profile=qemu-amd64-uefi' "release=$release" "build=$BUILD_ID-test-$release" \
        "system_bytes=$(stat -c %s "$path/system.img")" \
        "kernel_bytes=$(stat -c %s "$path/kernel")" \
        "initramfs_bytes=$(stat -c %s "$path/initramfs")" > "$path/manifest"
    (cd "$path"; sha256sum system.img kernel initramfs manifest > SHA256SUMS)
    openssl pkeyutl -sign -rawin -inkey "$WORK/private.pem" \
        -in "$path/SHA256SUMS" -out "$path/SHA256SUMS.sig"
}
make_payload "$WORK/baseline" 1
make_payload "$WORK/candidate" 2
mkdir "$WORK/store"
"$TOOL" initialize "$WORK/store" "$WORK/baseline" "$WORK/public.pem" "$ROOT_A" "$ROOT_B"
ACTIVE_HASH="$(sha256sum "$WORK/store/slots/a/system.img" | cut -d' ' -f1)"
"$TOOL" stage "$WORK/store" "$WORK/candidate" "$WORK/public.pem"
[[ ! -e "$WORK/store/efi/loader/entries/linos-2-b+2.conf" ]]
[[ "$(sha256sum "$WORK/store/slots/a/system.img" | cut -d' ' -f1)" == "$ACTIVE_HASH" ]]
"$TOOL" activate "$WORK/store" "$WORK/public.pem"
"$TOOL" activate "$WORK/store" "$WORK/public.pem"
ESP="$WORK/efi.fat"
truncate -s 512M "$ESP"
mkfs.vfat -F 32 -n LINOS_EFI "$ESP" >/dev/null
mmd -i "$ESP" ::/EFI ::/EFI/BOOT
mcopy -i "$ESP" "$BOOTLOADER" ::/EFI/BOOT/BOOTX64.EFI
mcopy -s -i "$ESP" "$WORK/store/efi/linos" ::/
mcopy -s -i "$ESP" "$WORK/store/efi/loader" ::/
dd if="$ESP" of="$DISK" bs=512 seek="$ESP_START" conv=notrunc,sparse status=none
dd if="$WORK/store/slots/a/system.img" of="$DISK" bs=1M seek="$((A_START / 2048))" conv=notrunc,sparse status=none
dd if="$WORK/store/slots/b/system.img" of="$DISK" bs=1M seek="$((B_START / 2048))" conv=notrunc,sparse status=none
# A real Data filesystem with a sentinel must remain byte-for-byte unchanged.
mkdir "$WORK/data-files"
printf '%s\n' 'linos update must preserve user data' > "$WORK/data-files/sentinel"
if [[ -n "${LINOS_UPDATE_ENROLLED_DISK:-}" ]]; then
    # Reuse enrollment proven by tests/enrollment.sh in both disposable slots.
    # This fixture exercises kernel readiness, not account migration.
    for name in device-id provisioning-state owner; do
        debugfs -R "dump /var/lib/linos/$name $WORK/data-files/$name" "$SYSTEM_IMAGE" >/dev/null 2>&1
        [[ -s "$WORK/data-files/$name" ]] || { echo "Missing enrolled fixture state: $name" >&2; exit 2; }
        chmod 600 "$WORK/data-files/$name"
    done
    debugfs -R "dump /etc/hostname $WORK/data-files/hostname" "$SYSTEM_IMAGE" >/dev/null 2>&1
    [[ -s "$WORK/data-files/hostname" ]] || exit 2
    chmod 600 "$WORK/data-files/hostname"
fi
mkdir "$WORK/data-files/updates"
"$TOOL" export-guest "$WORK/store" "$WORK/data-files/updates" "$WORK/public.pem" "$DATA_UUID" "$ESP_UUID"
truncate -s 64M "$WORK/data.ext4"
fakeroot -- sh -c 'chown -R 0:0 "$1"; /usr/sbin/mkfs.ext4 -F -q -d "$1" "$2"' sh "$WORK/data-files" "$WORK/data.ext4"
dd if="$WORK/data.ext4" of="$DISK" bs=1M seek="$((DATA_START / 2048))" conv=notrunc,sparse status=none
cp --reflink=auto --sparse=always "$DISK" "$WORK/success.raw"
# Fault injection affects only the candidate in this disposable failure branch.
sed 's/systemd.unit=/systemd.mask=linos-update-health.service systemd.unit=/' \
    "$WORK/store/efi/loader/entries/linos-2-b+2.conf" > "$WORK/withheld-entry.conf"
mcopy -o -i "$DISK@@$((ESP_START * 512))" "$WORK/withheld-entry.conf" ::/loader/entries/linos-2-b+2.conf
cp "$VARS" "$WORK/vars.fd"
boot() {
    local number="$1" uuid="$2" slot="$3" full="$4" outcome="${5:-}" log="$WORK/boot-$1.log"
    : > "$log"
    timeout --signal=TERM --kill-after=5s 150s qemu-system-x86_64 \
        -machine "q35,accel=${LINOS_QEMU_ACCEL:-tcg}" -cpu qemu64 -smp "${LINOS_QEMU_CPUS:-2}" -m "${LINOS_QEMU_MEMORY:-1536}" \
        -display none -no-reboot -monitor none -net none \
        -drive "if=pflash,format=raw,readonly=on,file=$CODE" \
        -drive "if=pflash,format=raw,file=$WORK/vars.fd" \
        -drive "file=$DISK,if=none,format=raw,id=installed" \
        -device virtio-blk-pci,drive=installed \
        -chardev "file,id=serial,path=$log" -serial chardev:serial \
        > "$WORK/qemu-$number.log" 2>&1 &
    PID=$!
    while kill -0 "$PID" 2>/dev/null; do
        if grep -Fq "root=PARTUUID=$uuid" "$log" && grep -Fq "linos.slot=$slot" "$log"; then
            if { [[ "$full" == no ]] || grep -Eq 'Linos 13.*Sign in|[[:space:]]login:' "$log"; } && \
                { [[ -z "$outcome" ]] || grep -Fq "Linos development update health: $outcome" "$log"; }; then
                kill -TERM "$PID" 2>/dev/null || true
                break
            fi
        fi
        sleep 2
    done
    wait "$PID" 2>/dev/null || true; PID=''
    if ! grep -Fq "root=PARTUUID=$uuid" "$log" || ! grep -Fq "linos.slot=$slot" "$log"; then
        tail -n 35 "$log" >&2; cat "$WORK/qemu-$number.log" >&2; return 1
    fi
    if [[ "$full" == yes ]] && ! grep -Eq 'Linos 13.*Sign in|[[:space:]]login:' "$log"; then
        tail -n 35 "$log" >&2; return 1
    fi
    if [[ -n "$outcome" ]] && ! grep -Fq "Linos development update health: $outcome" "$log"; then
        tail -n 45 "$log" >&2; return 1
    fi
    echo "PASS: firmware boot $number selected System ${slot^^}"
}
read_data() {
    local label="$1" expected_slot="$2" generation="$3" phase="${4:-healthy}" code=0
    dd if="$DISK" of="$WORK/data-$label.ext4" bs=1M skip="$((DATA_START / 2048))" count=64 status=none
    e2fsck -E journal_only -p "$WORK/data-$label.ext4" > "$WORK/replay-$label.log" 2>&1 || code=$?
    [[ "$code" -le 1 ]] || return 1
    debugfs -R 'cat /sentinel' "$WORK/data-$label.ext4" 2>/dev/null | grep -q 'linos update must preserve user data'
    debugfs -R 'cat /updates/update-state' "$WORK/data-$label.ext4" 2>/dev/null > "$WORK/journal-$label.log"
    grep -qx "phase=$phase" "$WORK/journal-$label.log"
    grep -qx "active=$expected_slot" "$WORK/journal-$label.log"
    grep -qx "generation=$generation" "$WORK/journal-$label.log"
    debugfs -R 'cat /device-id' "$WORK/data-$label.ext4" 2>/dev/null > "$WORK/identity-$label.log"
    grep -Eq '^[0-9a-f-]{36}$' "$WORK/identity-$label.log"
    debugfs -R 'cat /hostname' "$WORK/data-$label.ext4" 2>/dev/null > "$WORK/hostname-$label.log"
    grep -Eq '^[a-z0-9][a-z0-9-]{0,62}$' "$WORK/hostname-$label.log"
}
boot 1 "$ROOT_B" b yes
read_data first A 2 trial
# First candidate boot consumes one count, even without userspace approval.
mdir -i "$DISK@@$((ESP_START * 512))" ::/loader/entries | grep -q 'linos-2-b+1-1'
boot 2 "$ROOT_B" b no
mdir -i "$DISK@@$((ESP_START * 512))" ::/loader/entries | grep -q 'linos-2-b+0-2'
boot 3 "$ROOT_A" a yes RolledBack
read_data rollback A 5
cmp "$WORK/identity-first.log" "$WORK/identity-rollback.log"
cmp "$WORK/hostname-first.log" "$WORK/hostname-rollback.log"
DISK="$WORK/success.raw"
cp "$VARS" "$WORK/vars.fd"
boot 4 "$ROOT_B" b yes Committed
mdir -i "$DISK@@$((ESP_START * 512))" ::/loader/entries | grep -q 'linos-2-b.conf'
read_data committed B 3
boot 5 "$ROOT_B" b yes Healthy
boot 6 "$ROOT_B" b yes Healthy
read_data rebooted B 3
cmp "$WORK/identity-committed.log" "$WORK/identity-rebooted.log"
cmp "$WORK/hostname-committed.log" "$WORK/hostname-rebooted.log"
echo 'PASS: persistent Data, native health blessing, B reboots, exhausted A fallback and sentinel preservation'
echo 'This qualifies development image boot health, not enrollment, account migration or physical installation.'
