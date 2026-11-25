# Linos 13 Server installation layout v1 (provisional)

This remains the working development baseline. The owner accepted the separate
[layout 2 contract](../specs/0005-storage-recovery-contract/index.md). Its first
implementation slice adds only a pure geometry planner, available through
`cargo run --locked --bin recovery -- plan-v2 34359738368 512`. That command accepts
capacity/sector byte counts, never a device, and cannot apply the plan. Its GPT
fixture is `bash tests/layout-v2.sh`; file based validation does not qualify an installer.

The installer is not enabled. This specification defines a testable layout for
an AMD64 UEFI Server image and can change before the first installable alpha.

| GPT partition | Nominal size | Intended purpose |
| --- | ---: | --- |
| `linos-efi` | 512 MiB | UEFI boot binaries |
| `linos-recovery` | 2 GiB | Independent recovery environment |
| `linos-system-a` | 4 GiB | Active/standby system slot |
| `linos-system-b` | 4 GiB | Other system slot |
| `linos-data` | Remaining | Persistent configuration, home and service data |

The development minimum is 16 GiB, leaving a margin for alignment and GPT
metadata. Sizes are not yet final. Filesystems, disk encryption, shared data
mounts, kernel activation and A/B rollback semantics require further design.

`system/src/recovery/layout.rs` calculates a **proposal only**. The recovery
CLI can print it for a target that passes preliminary inspection, but always
prints **Installation status: DISABLED**.

`tests/installation.sh layout` creates a disposable sparse file under `/tmp`, lays
out GPT partitions, validates the table and then removes the file. It accepts
no device names or output-path arguments. Its sole purpose is validating
geometry and types without accessing physical block devices.

Before a real installer: verify boot-media identity, existing data, mount
dependencies, filesystem signatures, supported hardware, payload integrity,
user confirmation and recoverability. Do not reuse the disposable GPT test as
an installation command.
