# Linos Server Recovery boot prototype

The recovery image is a separate bootable artifact, built independently of
the installed Server root filesystem. The media edition is **server** at
build time, and Recovery does not allow selecting a different edition.

## Build artifacts

```text
images/
├── server/builds/amd64/<build>/
│   ├── disk.ext4
│   └── disk-uefi.raw          normal Server boot
└── recovery/builds/amd64/<build>/
    ├── rootfs.tar            independent minimal Debian-based userspace
    ├── boot/{kernel,initramfs}
    ├── disk.ext4
    └── disk-uefi.raw          boots directly to Linos Recovery CLI
```

The recovery image has no Linos owner-enrollment service and no SSH server.
It includes the Rust `linos-recovery` console, which is **read-only**:
status and disk inspection are implemented; installation, repair, formatting
and mounting are not.

## Recovery UEFI boot

`build/recovery-image.sh` creates a separate minimal userspace and packages
it into a GPT image with FAT32 EFI and systemd-boot using `build/uefi.sh`'s
recovery profile. `build/recovery-smoke.sh` verifies firmware boot and the
interactive Rust console using QEMU + OVMF.

Only the AMD64 UEFI recovery image is supported at this stage. Physical ARM
boards require device-specific firmware, DTBs and validation; Desktop Recovery
will use a graphical frontend when that edition exists.

## Safety

Neither builder opens, partitions or formats a physical disk. Every output is
a fresh image file under `images/`; existing outputs cannot be overwritten
without an explicit cleanup. The Rust console has no disk-write commands.
