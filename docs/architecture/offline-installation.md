# Offline Server installation prototype

The **first installable Linos 13 Server alpha is not yet complete**. This
milestone proves that our existing Server root filesystem can be packaged
offline, placed on the proposed GPT layout, and booted through UEFI in QEMU.

## Offline development payload

`build/payload.sh amd64` creates:

```text
images/install/payloads/amd64/<build>/
├── rootfs.tar.gz
├── kernel
├── initramfs
├── manifest
└── SHA256SUMS
```

The payload records image edition and architecture. SHA-256 checks integrity, while the signed-media CI path uses an Ed25519
signature and a separately supplied public key at build time. Production
release keys and a trust anchor independent of replaceable installation media
are still required. The present development media is not a fully trusted
production installer.

## Disposable installation simulation

`build/install-simulation.sh` takes **no arguments**, creates a temporary
16 GiB sparse disk, partitions it according to the Linos Server layout,
extracts the packaged userspace into System A, adds EFI boot files, and
boots the resulting disk under QEMU/OVMF. The test verifies Linos first-boot
initialization and the serial login prompt.

System B, Recovery and Data partitions are currently placeholders in this
test. Their lifecycle, mount policies and rollback mechanics remain to be
implemented.

The entire disk is created under a fresh `/tmp/linos-install-simulation.*`
directory that is removed when the test ends. It never accepts host disk
paths or runs any write operation on a physical block device.

## Remaining for a real installer

- Establish a production release-key and trusted-key distribution policy (the signed Recovery media CI path is already working).
- Move the deployment engine behind an authenticated, explicitly authorized
  Rust recovery installer, with robust target identity and source-media checks.
- Implement System B, Recovery and persistent Data provisioning.
- Install recovery boot entries, boot counting, integrity verification,
  first-run setup, and hardware-specific firmware policy.
- Test upgrades, rollback, power loss, encryption and data preservation.

Do **not** treat the simulation as a safe general-purpose installer.
