# Recovery installation planning safety

The Linos 13 Server Recovery console offers a read-only
`linos-recovery plan /dev/DEVICE` inspection report. It is **not** an installer
and has no filesystem-formatting, partitioning or deployment backend.

## Current safety checks

- Accept only a direct whole-disk candidate under `/dev/` whose name is
  discoverable under `/sys/block`.
- Inspect capacity, Linux read-only flag, and removable-media flag.
- Resolve mounted filesystems using `/proc/self/mountinfo` major:minor values
  and `/sys/dev/block` ancestry.
- Reject targets with mounted partitions, read-only devices, or provisional
  capacity below 16 GiB.
- If the booted root filesystem cannot be resolved to a recognizable physical
  parent (such as with unsupported mapped/RAID/container setups), fail closed.
- Never call partitioning, formatting, mount, install or flash operations.

An `InspectionOnly` result means the metadata passes these preliminary checks.
It **does not** mean the disk is safe to install onto. All reports continue to
show `Installation status: DISABLED`.

## Before enabling installation

- Correctly resolve full storage stacks (dm-crypt, LVM, md RAID, loop-backed
  boot media, multipath), EFI boot partitions and attached install media.
- Implement signed payload verification and trusted hardware-profile binding.
- Detect existing data, encryption and filesystem signatures.
- Define the exact installation partition policy and minimum capacity by edition.
- Provide an explicit disk identity, loss-of-data summary and separate
  destructive-operation approval.
- Test power loss and failures before and after every persistent change.
- Test migrations and recovery of existing installations.

The present 16 GiB threshold is only a development-stage rejection rule, not
a final product minimum or a committed A/B partition layout.
