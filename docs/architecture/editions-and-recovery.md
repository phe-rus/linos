# Linos image editions, hardware profiles, and recovery

## Product invariants

- **Linos 13** is the display name; internal software and package identifiers are lowercase `linos`.
- The edition belongs to the **bootable image at build time**. Recovery never presents an
  edition picker and must not download a different edition behind the user's back.
- Server media boots a **CLI recovery**; Desktop media will boot a **GUI recovery**.
  Both use the same Rust recovery domain model and safe storage operations.
- A hardware profile selects firmware, kernel build/configuration, device trees,
  drivers, boot transport and supported recovery interface. CPU architecture alone
  is insufficient to identify a supported physical board.
- Mobile/TV/wearable images may require a different partition, firmware, recovery
  and security architecture. Do not force a PC GPT/UEFI design onto those devices.

## Edition and hardware compatibility

| Media | Recovery UI | Deployment | Status |
| --- | --- | --- | --- |
| `server/amd64-uefi` | CLI | PCs and VMs with x86-64 UEFI | Firmware boot proof |
| `server/arm64-qemu` | CLI | QEMU `virt` | Direct kernel boot proof |
| `server/rk3588-orangepi5plus` | CLI | Orange Pi 5 Plus | Hardware verification pending |
| `desktop/amd64-uefi` | GUI | Compatible PCs | Planned |
| `mobile/<device>` | Device-specific | Individual phones | Future |

A Server installation may **add** Desktop capabilities when the hardware supports
them. Removing Desktop and reverting to Server is a separate, verified migration;
never silently discard packages, user data, GUI sessions or services. A PC can add
a Desktop interface, but downgrade to a minimal Server profile is not automatically
supported. Each transition needs a compatibility manifest, migration plan,
reversal option if feasible, and user confirmation.

## Recovery responsibilities

1. Identify the image's **embedded edition and hardware profile**.
2. Discover storage and platform requirements without modifying anything.
3. Verify installer payload signatures and supported target hardware.
4. Present available installation targets and an explicit destructive-operation
   confirmation, with safeguards against selecting the boot media.
5. Install a matching edition, then restart into Linos Setup.
6. Offer independent repair and recovery without cloud accounts.

The current `crates/recovery` program is a **read-only Server CLI prototype**.
It offers status and disk inspection. It cannot install, repair, mount or format.

## Security and independence

Local installation, login, and recovery work offline. Passid (`passid`) and
passme are optional services offered *after* local setup, never prerequisites.
Passwords, passcodes and passkeys have distinct meanings and identifiers.

## Next implementation stages

- [ ] Build a separate, minimal Linos Recovery root filesystem and firmware boot entry.
- [ ] Introduce a versioned signed hardware profile manifest and image manifest.
- [ ] Exercise Recovery boot under AMD64 OVMF and ARM64 QEMU.
- [ ] Add Rust storage inspection and installation plans, still without write permissions.
- [ ] Add an explicitly authorized disk-writing installer with rollback/recovery rules.
- [ ] Build Desktop Recovery GUI atop the shared domain API.
