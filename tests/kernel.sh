#!/usr/bin/env bash
set -Eeuo pipefail

# Boot a disposable Linos Server image with a research kernel. No host-device writes.
# Usage: bash tests/kernel.sh <assembled-image-directory>
[[ $# -eq 1 ]] || { echo "Usage: $0 <assembled-image-directory>" >&2; exit 2; }
DEST="$(realpath -- "$1")"
for file in kernel initramfs disk.ext4 kernel-release; do
  [[ -s "$DEST/$file" ]] || { echo "Missing $DEST/$file" >&2; exit 2; }
done
command -v qemu-system-x86_64 >/dev/null || { echo "QEMU amd64 missing" >&2; exit 2; }
command -v dumpe2fs >/dev/null || { echo "dumpe2fs missing" >&2; exit 2; }
dumpe2fs -h "$DEST/disk.ext4" >/dev/null 2>&1 || { echo "Invalid ext4 image" >&2; exit 2; }
RELEASE="$(cat "$DEST/kernel-release")"
[[ "$RELEASE" =~ ^[A-Za-z0-9._+-]+$ && "$RELEASE" == *-linos ]] || exit 2

LOG="$DEST/server-smoke.log"
QLOG="$DEST/server-qemu.log"
: > "$LOG"
timeout --signal=TERM --kill-after=5s 240s \
  qemu-system-x86_64 -machine pc,accel=tcg -cpu qemu64 -smp 2 -m 2048 \
  -display none -monitor none -no-reboot -snapshot -net none \
  -chardev "file,id=serial,path=$LOG" -serial chardev:serial \
  -kernel "$DEST/kernel" -initrd "$DEST/initramfs" \
  -drive "file=$DEST/disk.ext4,if=none,format=raw,id=rootfs" \
  -device virtio-blk-pci,drive=rootfs \
  -append "root=/dev/vda rootwait rw console=ttyS0,115200n8 loglevel=7 systemd.show_status=yes rd.systemd.show_status=yes systemd.mask=linos-enroll.service systemd.unit=multi-user.target" \
  >"$QLOG" 2>&1 &
PID=$!
while kill -0 "$PID" 2>/dev/null; do
  if grep -Fq 'Linos 13 · Sign in' "$LOG" &&
     grep -Fq 'Provisioning state: unclaimed' "$LOG" &&
     grep -Fq "$RELEASE" "$LOG"; then
    kill -TERM "$PID" 2>/dev/null || true
    break
  fi
  sleep 2
done
wait "$PID" || true
for pattern in 'Linos 13 · Sign in' 'Provisioning state: unclaimed' "$RELEASE"; do
  if ! grep -Fq "$pattern" "$LOG"; then
    echo "FAIL: research kernel did not reach verified Linos Server sign-in ($pattern)" >&2
    tail -n 80 "$LOG" >&2
    tail -n 30 "$QLOG" >&2
    exit 1
  fi
done
echo "PASS: Linos Server sign-in under research kernel $RELEASE"
