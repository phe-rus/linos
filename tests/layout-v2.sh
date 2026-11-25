#!/usr/bin/env bash
set -Eeuo pipefail
export PATH="/usr/sbin:/sbin:$PATH"
export CARGO_BUILD_JOBS=1
[[ $# -eq 0 ]] || { echo 'Usage: tests/layout-v2.sh (no arguments)' >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
command -v sgdisk >/dev/null || { echo 'gdisk is required' >&2; exit 2; }
OUT="$ROOT/target/layout-v2-tests"
mkdir -p "$OUT"
WORK="$(mktemp -d "$OUT/fixture.XXXXXXXX")"
trap 'rm -r -- "$WORK"' EXIT

cargo build --locked --bin recovery
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
[[ -n "$TARGET_DIR" ]] || { echo 'Cargo output directory not found' >&2; exit 2; }
TOOL="$TARGET_DIR/debug/recovery"
BYTES=$((32 * 1024 * 1024 * 1024))
"$TOOL" plan-v2 "$BYTES" 512 > "$OUT/plan-32g.txt"
grep -Fxq 'Installation status: DISABLED' "$OUT/plan-32g.txt"

# The sole target is a new regular file in this disk backed workspace.
DISK="$WORK/disk.raw"
truncate -s "$BYTES" "$DISK"
args=(--clear)
count=0
while read -r number name first last type; do
    [[ "$number" =~ ^[1-3]$ && "$first" =~ ^[0-9]+$ && "$last" =~ ^[0-9]+$ ]]
    args+=("--new=$number:$first:$last" "--typecode=$number:$type" "--change-name=$number:$name")
    ((count += 1))
done < <(awk '$1 ~ /^[1-3]$/ {print}' "$OUT/plan-32g.txt")
[[ "$count" == 3 ]]
sgdisk "${args[@]}" "$DISK" > "$OUT/create.log"
sgdisk --verify "$DISK" > "$OUT/verify.log"
grep -Fq 'No problems found.' "$OUT/verify.log"
sgdisk --print "$DISK" > "$OUT/table.txt"
[[ "$(awk '$1 ~ /^[0-9]+$/ {n++} END {print n+0}' "$OUT/table.txt")" == 3 ]]
while read -r number name first last type; do
    sgdisk --info="$number" "$DISK" > "$OUT/partition-$number.txt"
    actual_first="$(awk '/^First sector:/ {print $3}' "$OUT/partition-$number.txt")"
    actual_last="$(awk '/^Last sector:/ {print $3}' "$OUT/partition-$number.txt")"
    actual_type="$(awk -v part="$number" '$1 == part {print tolower($6)}' "$OUT/table.txt")"
    [[ "$first" == "$actual_first" && "$last" == "$actual_last" && "$type" == "$actual_type" ]]
    grep -Fxq "Partition name: '$name'" "$OUT/partition-$number.txt"
done < <(awk '$1 ~ /^[1-3]$/ {print}' "$OUT/plan-32g.txt")

status=0
"$TOOL" plan-v2 /dev/vda 512 > "$OUT/rejected-device-input.txt" 2>&1 || status=$?
[[ "$status" == 2 ]] || { echo 'Geometry command accepted a device path' >&2; exit 1; }

# Synthetic metadata exercises syntax only. No filesystem or observed mount
# identity is inferred from these values.
RECORD="$WORK/layout"
cat > "$RECORD" <<'RECORD_EOF'
kind=linos-layout
format=2
volume_uuid=11111111-1111-1111-1111-111111111111
disk_guid=22222222-2222-2222-2222-222222222222
esp_partuuid=33333333-3333-3333-3333-333333333333
recovery_partuuid=44444444-4444-4444-4444-444444444444
linos_partuuid=55555555-5555-5555-5555-555555555555
filesystem=ext4
filesystem_uuid=66666666-6666-6666-6666-666666666666
encryption=none
luks_uuid=none
device_key_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
baseline_manifest_sha256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
RECORD_EOF
"$TOOL" check-layout-v2 "$RECORD" > "$OUT/record-valid.txt"
head -n 13 "$OUT/record-valid.txt" | cmp - "$RECORD"
grep -Fq 'not authentication' "$OUT/record-valid.txt"
grep -Fq 'have not been verified' "$OUT/record-valid.txt"
grep -Fxq 'Installation status: DISABLED' "$OUT/record-valid.txt"

refuse_record() {
    local name="$1" path="$2" result=0
    "$TOOL" check-layout-v2 "$path" > "$OUT/record-rejected-$name.txt" 2>&1 || result=$?
    [[ "$result" == 2 ]] || { echo "Layout command did not refuse $name" >&2; exit 1; }
    # A refused record must never be printed as accepted metadata.
    if grep -Fq 'kind=linos-layout' "$OUT/record-rejected-$name.txt"; then
        echo "Layout command printed refused $name metadata" >&2
        exit 1
    fi
}
ln -s "$RECORD" "$WORK/layout-link"
refuse_record symlink "$WORK/layout-link"
mkfifo "$WORK/layout-fifo"
refuse_record fifo "$WORK/layout-fifo"
sed 's/encryption=none/encryption=luks2/; s/luks_uuid=none/luks_uuid=77777777-7777-7777-7777-777777777777/' \
    "$RECORD" > "$WORK/layout-encrypted"
refuse_record encryption "$WORK/layout-encrypted"
cp "$RECORD" "$WORK/layout-duplicate"
echo 'format=2' >> "$WORK/layout-duplicate"
refuse_record duplicate "$WORK/layout-duplicate"
echo 'PASS: layout 2 GPT has exactly EFI, Recovery and Linos with the planned bounds/types'
echo 'PASS: strict layout metadata inspection; duplicate, encryption, symlink and FIFO inputs refused'
echo 'PASS: device path input refused; no host block device was opened or changed'
echo "Geometry evidence: $OUT"
