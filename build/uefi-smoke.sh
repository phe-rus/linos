#!/usr/bin/env bash
set -Eeuo pipefail
export PATH="/usr/sbin:/sbin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
BUILD_ID="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' packages/release/usr/lib/linos/release)"
BUILD_DIR="images/server/builds/amd64/$BUILD_ID"
DISK="$BUILD_DIR/disk-uefi.raw"
LOG="$BUILD_DIR/uefi-smoke.log"
QEMU_LOG="$BUILD_DIR/uefi-qemu.log"
[[ -s "$DISK" ]] || { echo "Missing UEFI disk; run ./build/uefi.sh" >&2; exit 2; }

CODE=""
VARS=""
for candidate in /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [[ -r "$candidate" ]] && { CODE="$candidate"; break; }
done
for candidate in /usr/share/OVMF/OVMF_VARS_4M.fd /usr/share/OVMF/OVMF_VARS.fd; do
    [[ -r "$candidate" ]] && { VARS="$candidate"; break; }
done
[[ -n "$CODE" && -n "$VARS" ]] || { echo "OVMF firmware not found (install ovmf)" >&2; exit 2; }

WORK="$(mktemp -d /tmp/linos-uefi-test.XXXXXXXX)"
trap 'rm -r -- "$WORK"' EXIT
cp "$VARS" "$WORK/vars.fd"
: > "$LOG"
echo "UEFI boot-testing Linos 13 Server (amd64)"
timeout --signal=TERM --kill-after=5s 180s \
    qemu-system-x86_64 \
    -machine q35,accel=tcg -cpu qemu64 -smp 2 -m 2048 \
    -display none -no-reboot -monitor none -net none -snapshot \
    -drive "if=pflash,format=raw,readonly=on,file=$CODE" \
    -drive "if=pflash,format=raw,file=$WORK/vars.fd" \
    -drive "file=$DISK,if=none,format=raw,id=linos_disk" \
    -device virtio-blk-pci,drive=linos_disk \
    -chardev "file,id=linos_serial,path=$LOG" -serial chardev:linos_serial \
    >"$QEMU_LOG" 2>&1 &
PID=$!
while kill -0 "$PID" 2>/dev/null; do
    if grep -Eq '(^|[[:space:]])[^[:space:]]+[[:space:]]login:' "$LOG" &&
       grep -q 'Provisioning state: unclaimed' "$LOG"; then
        kill -TERM "$PID" 2>/dev/null || true
        break
    fi
    sleep 2
done
wait "$PID" || true
if grep -Eq '(^|[[:space:]])[^[:space:]]+[[:space:]]login:' "$LOG" &&
   grep -q 'Provisioning state: unclaimed' "$LOG"; then
    echo "PASS: firmware -> EFI bootloader -> kernel -> Linos identity -> login"
else
    echo "FAIL: UEFI boot or Linos initialization did not complete" >&2
    tail -n 50 "$LOG" >&2
    tail -n 30 "$QEMU_LOG" >&2
    exit 1
fi
