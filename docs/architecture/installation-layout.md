# Linos 13 Server installation layout v1 (provisional)

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

`crates/recovery/src/layout.rs` calculates a **proposal only**. The recovery
CLI can print it for a target that passes preliminary inspection, but always
prints **Installation status: DISABLED**.

`build/layout-test.sh` creates a disposable sparse file under `/tmp`, lays
out GPT partitions, validates the table and then removes the file. It accepts
no device names or output-path arguments. Its sole purpose is validating
geometry and types without accessing physical block devices.

Before a real installer: verify boot-media identity, existing data, mount
dependencies, filesystem signatures, supported hardware, payload integrity,
user confirmation and recoverability. Do not reuse the disposable GPT test as
an installation command.
