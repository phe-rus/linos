# Verified image update staging and boot selection

Status: Implemented and verified for development images only. See
[qualification evidence](../architecture/image-updates.md#observed-qualification-22-november-2026).

The owner authorized persistent A/B recovery and then inactive slot installation
and boot selection. This step implements those operations for ordinary image
files. Physical installation, persistent guest Data mounting, health approval
and production key provisioning remain separate qualification work.

## Contract

- `linos-update-image` owns only a caller-owned directory of regular image files.
  Initialization requires an empty directory. Device nodes, symlinks and unsafe
  directories are refused. It never mounts or opens a block device.
- Initialization takes a separately supplied Ed25519 public key, an authenticated
  baseline payload and explicit System A/B GPT partition UUIDs from the disposable
  disk builder. Edition is Server, architecture AMD64 and hardware profile is
  `qemu-amd64-uefi`. No hardware support is inferred.
- Payload format 2 contains `system.img`, `kernel`, `initramfs`, `manifest`,
  `SHA256SUMS` and its Ed25519 signature. The signature covers exact checksum
  bytes, which cover the other four files. The manifest provides edition,
  architecture, hardware profile, increasing release number, build identifier
  and exact artifact sizes. No unsigned or payload-supplied trust key is accepted.
- Copying is bounded by signed sizes. Hashes are checked after copying into the
  private destination, so source mutation cannot change installed bytes after
  verification. Kernel and initramfs share the same signed artifact set.
- The journal lock serializes operations. Staging requires Healthy, compares
  release numbers against the active signed manifest, replaces only the inactive
  slot and records Staged only after durable images and boot assets exist.
- Boot entries use the supplied partition UUID, fixed serial-console options,
  a release-specific kernel/initramfs directory and two systemd-boot trials.
  Activation persists Trial before publishing the entry. The existing healthy
  entry remains and the default glob selects Linos entries. EFI variable overrides
  and automatic success blessing are excluded from the disposable test setup.
- Recovery after an interruption before publication keeps the old boot choice.
  Retrying activation in Trial must not reset the attempt count or create a
  second trial entry. Uncertain outcomes require inspection before retrying.
- The image workspace journal is not the guest Data partition. This step does
  not claim guest health commit, installed rollback, physical power-loss safety,
  Secure Boot, expiry/key rotation or production release readiness.

## Verification

Rust tests cover signature failure, payload mutation, wrong target, downgrade,
active-slot preservation, stale state and activation retries. A disposable GPT
and OVMF test assembles the resulting images and boot entries, verifies candidate
selection and preserves a Data sentinel. Real image tests require existing
Server image artifacts and are separate from fast unit tests.

Sources: existing installation-layout.md and kernel-platform-updates-proposal.md;
https://systemd.io/AUTOMATIC_BOOT_ASSESSMENT/ and
https://www.freedesktop.org/software/systemd/man/latest/loader.conf.html.
