# Boot and Recovery architecture

Linos 13 is a Linux-based operating-system platform. Debian 13 supplies first-stage
packages, not the architecture or product identity of Linos.

## What works today

- `build/image.sh` produces a tested ext4 root filesystem and extracted kernel/initramfs
  for AMD64 and ARM64 QEMU direct-kernel boots.
- `build/uefi.sh` wraps the **AMD64 server prototype** in a GPT disk with a FAT32
  EFI System Partition, systemd-boot, and a root partition. It does **not**
  implement the recovery application or installer.
- `build/uefi-smoke.sh` boots that disk through OVMF firmware and verifies
  Linos first-boot initialization and a serial login.
- ARM64 QEMU and board-specific firmware support are separate; ARM64 direct-kernel
  smoke tests remain in place. Do not treat QEMU ARM64 as Orange Pi hardware validation.

## AMD64 firmware boot image

```text
disk-uefi.raw (4 GiB, GPT)
├── EFI System Partition (FAT32, 512 MiB)
│   ├── EFI/BOOT/BOOTX64.EFI       systemd-boot
│   ├── loader/loader.conf
│   ├── loader/entries/linos.conf
│   └── linos/{kernel,initramfs}
└── Linos System (ext4, 3 GiB)
    └── Current server root filesystem
```

`disk.ext4` remains the existing non-GPT root filesystem artifact. The UEFI
image is a new artifact, not a replacement.

## Target recovery-led installation (not yet implemented)

1. Firmware loads the Linos boot environment.
2. Linos Recovery starts independently of the installed system.
3. Recovery provides installation, disk selection, diagnostics and repair.
4. The installer asks for explicit confirmation before touching any target disk.
5. After installation, Linos Setup handles language, network and local account.
6. Passid (`passid`) and passme are **optional**, never requirements for local use.

An eventual installed disk may contain an EFI partition, system A/system B,
recovery and persistent data, but the actual A/B layout, filesystem,
mount policies, encryption and rollback semantics are **not finalized**.

## Build and test

On an AMD64 Linux builder, after a successful `./build/image.sh`:

```bash
sudo apt-get install gdisk dosfstools mtools systemd-boot-efi ovmf
./build/uefi.sh amd64
./build/uefi-smoke.sh
```

The scripts only create files within `images/server/builds/` and temporary
files. They do not partition host disks or write removable media.

## Non-goals of this first stage

- No EFI Secure Boot signing or verified UKI yet.
- No GUI installer, recovery menu, encryption, or rollback.
- No firmware packaging or DTB selection for specific ARM boards.
- No permission to flash development machines automatically.
