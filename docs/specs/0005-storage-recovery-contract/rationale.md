# Storage and Recovery rationale

## Context

The owner selected USB `EFI/` plus `recovery.pherus.org/`, installed `EFI`,
`Recovery`, `Linos` partitions and five logical Linos roles. Current development
updates depend on separate ext4 System A/B and Data partitions. Their tested
authentication, durable state and fallback properties must survive a redesign.
Stage 1 verified artifact transport, not a replacement storage layout.

This record covers Stage 2 design at source
`a0a667a`, after native development run `45553431687`. No storage implementation,
kernel configuration, package installation, disk write or VM boot occurred during
this design pass. OS security and physical qualification remain separate open gates.

## Confirmed owner decisions

On 25 November 2026, the owner selected:

* Keep current personal and service data on OS rollback. Block incompatible
  rollback until a safe service migration or explicit restore path exists.
* Apply administrator APT changes through a managed deployment transaction.
* Define optional Linos volume encryption; qualify the plain VM path first.

The owner then selected ext4 deployment images and confirmed the five record model.

## Current implementation inventory

| Area | Source | Consequence for the replacement |
| --- | --- | --- |
| Five partition proposal and 16 GiB minimum | `system/src/recovery/layout.rs`, `docs/architecture/installation-layout.md` | These are existing provisional geometry, not an approved three partition sizing contract |
| Separate System A/B, Data and EFI identity | `system/src/recovery/update/health.rs` | `Layout` binds four distinct UUIDs and validates writable whole filesystem mounts on one parent disk; directories or loop roots will not pass unchanged |
| Durable update state | `system/src/recovery/update/journal.rs` | Preserve locking, phase/generation compare, file sync, rename, directory sync and uncertain outcome handling |
| Authenticated system images | `system/src/recovery/update/artifact.rs`, `image.rs` | Current format 2 binds system/kernel/initramfs, target and size; image limit is 4 GiB, boot artifacts 128 MiB each |
| Counted fallback and blessing | `system/src/recovery/update.rs`, `health.rs`, `tests/update.sh` | Two trials, exhausted fallback reconciliation, native blessing and sentinel preservation are baseline acceptance behavior |
| Account creation and setup | `system/src/setup/main.rs`, `login.rs` | Native useradd/passwd and setup/login markers modify `/etc`; merely sharing `/var/lib/linos` cannot preserve full account state |
| Device ID and managed hostname | `system/src/init.rs` | Required State absence must remain fatal; a new layout must not regenerate identity on a system image |
| Independent Recovery | `build/recovery.sh`, `tests/boot.sh` | Recovery image exists; physical installation remains disabled. Independence of the new three partition design still needs proof |
| Offline payload | `system/src/recovery/payload.rs`, `build/recovery.sh` | Format 1 is an archive payload, not the format 2 updater image. Do not silently reinterpret either |
| Store operations | `system/src/console/store.rs` | Read only inventory and simulations currently suggest direct native APT; a managed model must change both the message and enforcement |
| Compiled filesystem support | Actual AMD64 kernel package from run `44213747067`, `/boot/config-6.18.63-linos` | `EXT4_FS`, `BLK_DEV_LOOP`, `DM_CRYPT` and `OVERLAY_FS` are enabled; `BTRFS_FS` is not enabled. Common policy alone did not prove loop support |

## Options considered

| Option | Strengths | Costs and limitations |
| --- | --- | --- |
| Keep five partitions and improve in place | Smallest change to the demonstrated update path | Does not meet the selected three partition target; useful only as the retained baseline |
| Ext4 Linos volume containing complete deployment images | Reuses signed ext4 image payloads and the verified kernel; image identities remain explicit | Needs new early loop mount, ancestry checks and space planning; retains complete images rather than cheap filesystem snapshots |
| Btrfs Linos volume with deployment subvolumes | Native extent sharing and fast system snapshots; each deployment can be selected explicitly | Requires Btrfs kernel support, userspace/Recovery tools, new boot and validation paths, and space/metadata behavior qualification |
| LVM inside Linos with logical system/data volumes | Internal volume boundaries and block snapshots can meet three physical partitions | Adds volume management and recovery dependencies; does not by itself solve package, account or service data compatibility |

## Rationale

Use the selected ext4 deployment images for the first migration. They preserve
more of the authenticated image model and reuse the available qualified kernel.
This recommendation does not claim better runtime performance or lower storage
use. Btrfs is the runner up when extent sharing justifies the extra kernel and
Recovery qualification. No filesystem performance ranking is claimed.

The replacement should be introduced as a separate versioned layout, keeping the
working development format until equivalent VM tests pass. There is no current
physical installation to justify an automatic in place repartitioning operation.
An eventual import must be explicit and must preserve numeric account ownership,
configuration and current data. A format mismatch is a refusal, not a guess.

## Primary documentation checked

The Linux loop documentation confirms that a regular image file can back a block
device. The actual Linos package configuration was inspected separately; upstream
capability alone does not prove Linos enables it.

Btrfs documents that nested subvolumes are not recursively included in a snapshot.
Therefore Users/State boundaries need explicit ownership and snapshot policy even
when the filesystem supports snapshots. Copying an OS subvolume cannot establish
database consistency or preserve current credentials by itself.

The systemd boot documentation places the normal Type 1 kernel and initramfs on
the ESP or XBOOTLDR. The selected design has no fourth XBOOTLDR partition, so the
final EFI capacity/preflight rules must cover retained complete boot sets and
independent Recovery. No new bootloader is required just for editable folders.

Verified sources:

* [Linux loop device interface](https://man7.org/linux/man-pages/man4/loop.4.html).
* [Btrfs subvolumes and snapshot boundaries](https://btrfs.readthedocs.io/en/stable/btrfs-subvolume.html).
* [systemd boot entries and artifact locations](https://www.freedesktop.org/software/systemd/man/latest/systemd-boot.html).
* [systemd encrypted volume boot integration](https://www.freedesktop.org/software/systemd/man/latest/systemd-cryptsetup-generator.html).
* [Debian initramfs hook phases](https://manpages.debian.org/trixie/initramfs-tools-core/initramfs-tools.7.en.html).
* [Debian APT configuration](https://manpages.debian.org/trixie/apt/apt.conf.5.en.html).
* [Unified kernel image components](https://www.freedesktop.org/software/systemd/man/latest/ukify.html), format reference only; native tools compose the image, with no Python builder.
* [Counted trials and blessing](https://systemd.io/AUTOMATIC_BOOT_ASSESSMENT/).

## Stage 2 review boundary

The children now fix ext4 geometry and loop mounts, independent Recovery, offline
USB chunking, EFI transfer, package isolation, versioned records, signing domains,
history/space policy, state transitions and failure outcomes. These are proposed
engineering decisions, not behavior observed in a new image. Review and acceptance
of the assembled spec close the design gate. Stage 3 must implement and qualify it.
Optional encryption activation, key rotation, unreviewed package adapters, native
app packaging and device extension loading remain disabled or deferred until their
own contracts and tests exist.

## Independent cross check, 25 November 2026

A separate read only model reviewed all six spec files and relevant updater source.
Initial disposition: revise before accepting implementation. The ext4 three
partition direction remains viable. The owner approved applying all six proposed
resolutions, and the normative children/index have been revised accordingly.
The original findings and applied resolutions are retained below for traceability.
The independent closure check confirmed the six fixes were represented, identified
five cross-document alignment issues, and then confirmed those five were resolved
with no direct contradiction among the revised rules. This is not owner acceptance
of Stage 2 or runtime qualification.

1. **Trial and fallback authority.** The journal needs retained fallback and stable
   boot entry identities after candidate approval, plus source/selected config
   bindings during trial. Recommend explicit durable pointers/receipts, numeric
   BLS ordering, masked independent blessing, exact counter reconciliation and
   refusal of premature manual fallback before writable State is mounted. Locks
   protect individual steps; phase/generation rules serialize across reboots.
2. **Service state consistency.** A service could change its format before a
   matching registry write; removed services retain data but lose their deployment
   contracts. Recommend schema preserving writers only initially, registration
   before first start, and dormant retained records on removal. Reattachment checks
   current schema; changing it requires a separate durable migration protocol.
3. **Account and configuration merge.** Whole file copying cannot add required
   service accounts, while a generic merge can conflict with unrelated password
   changes. Recommend keyed native account merges preserving current credentials,
   stable IDs and memberships; explicitly create new locked service accounts and
   retain removed account ownership. Define initial vendor seeding, deletion/type/
   metadata rules and selected generation reuse after interruption.
4. **Recovery trust and installation order.** Existing deployment signatures do
   not cover the proposed Recovery root or transport manifest. Recommend distinct
   signed Recovery/transport records with explicit external key sources, a target
   confirmation bound to observed disk/source ancestry, verified Recovery and
   initialized identity/configuration before durable InstallPending, then OS entry
   and discoverable loader publication. Retries resume the same identified target
   without automatic reformatting.
5. **Owner package choices across official upgrades.** A new official image must
   not silently remove locally selected packages or discard required provenance.
   Recommend reviewed re-resolution of the desired catalog on the new official
   base, refusal of unresolved selections, active deployment/plan binding and
   protection of referenced base manifests/plans from collection.
6. **Package files beneath writable mounts.** State `/var` can hide required
   package files, while copying the worker's `/var` could overwrite live data.
   Recommend adapter classification of every affected writable path: immutable
   exposure or approved create-if-absent initialization, never wholesale copying
   over existing data. Refuse unclassified files/scripts/triggers. Specify private
   mount propagation and capability bounding for the worker.

Stage 3 tests should additionally prove open file identity through loop attachment,
immutable bind sources after covering `/var`, nested mount shutdown, independent
Recovery with unreadable Linos and disconnected USB, and duplicate disk labels.
These are qualification obligations, not a reason to claim the new layout works now.

### Closure of revised contracts

The owner approved the six recommended fixes and reiterated proceeding during
revision. The final narrow independent check closed these five resulting alignment
points: signed Recovery loader and ESP verification files; signed supplementary
service memberships; independent dormant contract/ownership retention; journal
selected deployment as catalog authority; and counted trials for owner rollback.
No additional model review or runtime behavior was claimed. The owner accepted the assembled specification on 25 November 2026 and
authorized proceeding. Stage 2 is complete; runtime implementation remains Stage 3.
