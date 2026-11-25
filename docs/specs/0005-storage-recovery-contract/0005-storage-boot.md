# Ext4 storage, boot and offline Recovery

## Summary

Use ext4 for the Linos volume and keep complete system images inside it. Mount
the selected system image as read only, then attach current configuration and
persistent data at ordinary Linux paths. EFI and Recovery remain separate so
Recovery can start when the Linos volume cannot mount.

## Disk geometry and filesystems

The initial implementation target is a new disposable AMD64 UEFI virtual disk.
It is not a minimum hardware claim and does not enable physical installation.
All boundaries use 1 MiB alignment and leave space for both GPT copies.

| Partition | GPT name | Size | Filesystem and mounting |
| --- | --- | --- | --- |
| 1 | `EFI` | 1024 MiB | FAT32 ESP, mounted root privately at `/efi`, ordinary access root only |
| 2 | `Recovery` | 4096 MiB | Independent ext4 Recovery root, not mounted during ordinary operation |
| 3 | `Linos` | Remaining complete aligned space | Ext4 volume, optionally inside LUKS2 only after encryption qualification |

Require at least 32 GiB disk capacity and reserve 16 MiB in geometry calculations
for metadata/alignment. Growing a target grows Linos only. A smaller disk is a
bounded refusal. Names are user labels; actual selection uses UUIDs and verified
common disk ancestry. Duplicate labels do not select a different volume.

The private runtime volume mount is `/run/linos/volume`, referred to as `V` in
the ownership child. Its root contains:

```text
System/deployments/<deployment-id>/
    system.img
    kernel
    initramfs
    manifest
    SHA256SUMS
    SHA256SUMS.sig
    inventory
    applications
    services
    accounts
    extensions
Applications/catalog/
Users/<name>/
Users/root/
State/config/<generation>/
State/var/
State/local/
State/srv/
State/tmp/{tmp,var-tmp}/
Snapshots/<deployment-id>/record
```

Deployment IDs are SHA256 digests of the exact signed deployment manifest bytes.
History records reference deployment IDs rather than copying complete images
again. Neither mutable configuration nor service data is placed in `system.img`.
The private volume root is root owned with mode 0700. Expose Users and the selected
mounts separately; ordinary users cannot edit manifests or images through `V`.

Each system image is a clean ext4 filesystem, at most 4 GiB, mounted on a read only
loop device with `ro,noload`. A needs recovery flag or invalid filesystem is a
refusal, not permission to replay its journal and invalidate the authenticated
image. Staging may use a writable image, but it is unmounted, checked, sealed,
hashed and synced before publication. The selected image cannot be modified in place.

## Early boot sequence

Retain systemd boot and Debian initramfs tools. Rust owns record parsing, identity
validation and selection policy. Shell hooks only orchestrate the native helpers
and existing Linux utilities. Include the Rust boot helper and its required runtime
libraries, loop/ext4 support and native mount tools in the generated initramfs.
Do not depend on binaries from the not yet mounted root.

Normal installed entries pass `root=/dev/linos-system ro rootflags=noload`, plus
`linos.layout=2`, `linos.volume=<Linos-PARTUUID>`,
`linos.deployment=<manifest-sha256>` and `linos.transaction=<u64>`.
The local top hook prepares the selected loop device and `/dev/linos-system`.
The local bottom hook performs persistent mounts before switching to the real root.
Treat `/dev/linos-system` as a helper created reference, not a caller supplied path.

Bind source identity to the observed firmware ESP. In USB Recovery mode its loop
backing file must be the fixed Recovery image on that ESP; in installed Recovery
mode its direct root must be the layout's Recovery PARTUUID on the same disk as
the observed ESP. Refuse missing, ambiguous or duplicate ancestry; labels or an
unchecked command line device path do not establish boot-media identity.

1. Parse bounded unique selectors. Resolve the EFI boot device and Linos PARTUUID,
   then validate the installed layout, filesystem UUID and common physical parent.
   Initial plain mode mounts the outer ext4 volume; encrypted mode must unlock
   the recorded LUKS2 UUID first and validate its actual backing partition.
2. Lock the volume journal. Resolve only a canonical deployment directory under
   `System/deployments/`, rejecting symlinks, duplicate records and unsafe owners.
   Check its manifest signature with an independently provisioned trusted key,
   source/architecture/target constraints, lengths and file hashes.
3. Validate the selected transaction and the observed firmware entry. Check
   configuration and service compatibility before any user service can write.
   Prepare the configuration generation from the current source as specified in
   the ownership child. Publish its durable selection and boot receipt.
4. Open the verified image without following links, attach that exact file to a
   read only loop device, and retain its backing file identity for validation.
   The ordinary initramfs root mount uses that device. Do not find an arbitrary
   ext4 filesystem by label or fall back to another root image opportunistically.
5. Before real init, bind the selected configuration at `/etc`, Users at `/home`
   and `/root`, State var/local/srv/temp at the specified paths, and the selected
   immutable package database over the State var package paths. Keep a private
   readonly reference to the image root before covering `/var`, so these sources
   remain the image's directories rather than State. Mount `/efi`
   from the recorded ESP. Preserve the outer mount beneath the moved runtime
   `/run` mount across the root switch; losing it is a boot failure.
6. Real init verifies a root owned boot receipt and observed mounts again. Record
   the loop backing inode/device and its parent volume, root mount read only
   status, exact bind sources, filesystem UUID and ESP ancestry. The old rule
   requiring three distinct writable whole filesystems is not applicable to layout 2.
7. Only then run identity, account enrollment or existing service startup. A new
   install starts unclaimed with one stable baseline. Managed update trials are
   enabled only after enrollment; no password or owner is invented for a health check.

On validation failure, do not create substitute State directories on the system
image. Boot a known eligible fallback through its normal counted selection or
instruct the owner to select Recovery. Do not run a diagnostic shell as a substitute
for authentication in an ordinary installed boot.

Place a generic `layout=2` capability marker in the immutable system image; keep
device specific UUIDs in the installed layout record and validated runtime boot
receipt, not inside a generic signed image. Adapt the initializer's old managed
layout detection explicitly. Generated fstab/remount units must keep the image
root read only and not recreate old separate Data mounts or remount the loop writable.
Conflicting administrator mount configuration blocks activation. Stage 3 must
verify both initramfs to real init mount survival and systemd remount behavior.

## Installed EFI

The installed partition normally remains hidden. Its filesystem contains
`EFI/BOOT/BOOTX64.EFI`, `EFI/Linos/deployments/<id>/{kernel,initramfs}`,
`EFI/Linos/recovery/{kernel,initramfs}`, and the existing systemd
`loader/loader.conf` plus `loader/entries/` configuration. The Recovery partition
name is not a directory that the bootloader must understand.

The Recovery directory also holds its signed `recovery-manifest` and
`recovery-manifest.sig` for verification without Linos access. The Recovery
signature binds the copied EFI loader as well as root/kernel/initramfs/USB UKI.

Normal Type 1 entries retain the working two attempt filename scheme and exact
boot assessment receipts. Name them `linos-<transaction>-<deployment-id>+2.conf`,
using the local transaction as the entry version/order rather than modifying a
signed official release. Include a stable `recovery.conf` pointing to the Recovery
PARTUUID. Set the normal default pattern to managed OS entries and expose Recovery
in a visible boot menu. No extra top level disk partition is introduced.

Every OS entry has explicit `sort-key linos` and `version <transaction>` fields;
filenames alone do not order versions. Only recorded current/fallback/candidate
entry pairs are eligible. Stable names remove the counter suffix. Accept only
the known `+2`, `+1-1`, `+0-2` counter forms for that same pair, and mask
`systemd-bless-boot.service` in each trial's command line. Unknown entry or counter
identities are an inspection error. Qualification must cover numeric 9 to 10,
two compositions of the same official release, exhausted fallback, manual old
selection before exhaustion and attempted independent blessing.

Kernel/initramfs are copied from one authenticated deployment; module trees in
the root must report the same kernel release. The first extension set is empty.

Recovery uses the independently built root image and its matching kernel/initramfs,
not the selected Server deployment. Its root is read only. Runtime sockets use
bounded `/run`; large repair/download/staging work uses disk storage explicitly,
never an image sized tmpfs. When Linos cannot mount, offline reinstall can read
directly from USB rather than attempting to hold images in RAM.

The 1 GiB EFI size is not a promise that every maximum sized boot artifact set
fits. Admission measures the actual retained/candidate/Recovery bytes plus the
reserve below and refuses a set that cannot fit. It never evicts a required
fallback or independent Recovery to admit a candidate.

## USB tree and offline transport

The initial USB image has one FAT32 ESP of at least 16 GiB, sized upward when the
verified payload plus reserve needs more space. Its root has the two owner selected
directories only. This is separate from installed disk geometry.

```text
EFI/BOOT/BOOTX64.EFI
EFI/Linux/linos-recovery.efi
recovery.pherus.org/recovery.img
recovery.pherus.org/recovery-manifest
recovery.pherus.org/recovery-manifest.sig
recovery.pherus.org/{kernel,initramfs}
recovery.pherus.org/payload/{manifest,manifest.sig,...}
```

Use a Recovery unified kernel image, a single EFI executable containing the kernel,
initramfs and command line, for automatic discovery under `EFI/Linux`. This avoids
adding a third USB root directory for Type 1 loader configuration. Compose it in
CI using the native systemd EFI stub and native `objcopy`, orchestrated by shell
or Rust. No Python image builder is permitted. Record and verify the embedded
kernel/initramfs sections against the Recovery artifact inputs. Composition does
not rebuild Linux. Packaging a UKI does not claim Secure Boot signing.

The USB Recovery initramfs locates its own ESP using firmware boot ancestry and
mounts the fixed `recovery.pherus.org/recovery.img` read only through a loop device.
`recovery.img` is at most 3 GiB and is checked against the independently trusted
Recovery manifest. Do not look for a folder of that name on the installed Recovery
partition. Installed Recovery is the same clean root image copied into its 4 GiB
partition, with unused tail space; it boots directly by PARTUUID.

New offline transport uses an explicit `kind=linos-offline`, `format=3` manifest,
separate from the existing format 1 tar and format 2 updater image. Reuse the
authenticated deployment files, splitting only `system.img` into ordered parts
of at most 1 GiB named `system.part.0000` through `system.part.0003`. A maximum
4 GiB image requires at most four parts; each part fits the FAT32 file limit.
Manifest fields bind part count, each exact length and digest, concatenated system
length/digest, deployment manifest digest and all other payload file lengths/digests.
Copy parts sequentially into the private target image, hashing while copying;
publish it only after the final whole image hash matches. Do not reconstruct it
in RAM. New transport never silently enables unsigned installation.

The records child fixes the exact signing prefixes, key sources and manifests.
The transport contains the original deployment manifest, checksums and signature
under fixed names `deployment-manifest`, `deployment-SHA256SUMS` and
`deployment-SHA256SUMS.sig`, plus its kernel/initramfs and five metadata files.
After authentication and image reconstruction, map these names to the canonical
deployment directory names; do not replace its original signature with a transport
signature or trust a payload supplied key.

USB to internal transfer preserves the working firmware loader and compatible
configuration. Location specific Recovery and OS selectors are regenerated for
the target. The USB Recovery UKI has USB specific discovery behavior and must not
remain an auto discovered internal entry: preserve its source inventory outside
`EFI/Linux` if needed, and publish the installed `recovery.conf` instead. Do not
copy opaque source paths as executable target policy. Stage 4 later defines the
recognized device extension copy/enable contract.

## Capacity and retention policy

Units below are binary. Default history is the current healthy deployment and one
healthy fallback; at most one further candidate exists. No duplicate image is
created merely to make a Snapshots record. Retained root images remain protected
until references and trial receipts permit collection.

For Linos, `required_free = candidate_logical_bytes + uncached_download_bytes +
configuration_copy_budget + metadata_margin + operational_reserve`.
The initial configuration copy budget is the current configuration's measured
logical size plus the signed candidate vendor configuration size. Reject a
configuration tree beyond that measured bound if it grows before activation.
Metadata margin is 256 MiB and operational reserve is max(1 GiB, 5% of Linos capacity).
The writable staging image is sealed in place after unmount; no second final image
copy is budgeted. Any implementation that needs another copy must count it before
admission rather than silently exceeding this formula.

EFI admission requires free bytes for the new complete boot set, temporary selector
publication and 64 MiB reserve while keeping current/fallback/Recovery intact.
USB admission counts all files, filesystem overhead and 256 MiB reserve. Measure
allocated and available space with filesystem statistics; signed logical maximums
bound images even when their current sparse allocation is smaller.

Preallocate a root private 64 MiB emergency file under State at installation.
It is not ordinary transaction capacity. Recovery may release it after stopping
writers to make a repair/journal decision durable, then recreate it before normal
operation. If enough durable write space still cannot be obtained, preserve the
current selection and enter Recovery inspection. Space races are handled as
write failures; admission is not a guarantee against an administrator filling the disk.

## Optional encryption

Record `encryption=none` for the initial plain path. The encrypted variant is
LUKS2 around partition 3 with ext4 inside; EFI and Recovery remain plaintext and
hold no volume unlock secret. A wrong key cannot fall back to formatting or
initializing another volume. Initramfs and Recovery need the same unlock policy.
Cipher, key derivation resource limits, secret provisioning and header backup
must be a separate encryption activation specification before that variant is
enabled. No implementation choice is left hidden behind an already enabled option.

## Qualification and migration

Introduce `layout=2` alongside the existing development path and reuse the exact
Stage 1 kernel package only if its recorded input provenance still matches.
The new initramfs and userspace require a new OS candidate in CI. Qualify the
unmodified candidate, USB boot, virtual install, USB removal/internal boot,
independent Recovery, two failed trial boots/fallback and successful approval.
Also prove real enrolled accounts and current service/configuration state survive.

No in place disk conversion is supported initially. Import is a future explicit
copy to a new target after quiescence, backup and identity checks. Existing images
remain unchanged. Host tests create regular disk files; actual guest writes are
confined to disposable virtual disks. No physical target is enabled.

This contract satisfies **AC-1**, **AC-4**, **AC-5**, **AC-7**, **AC-8**, **AC-9**
and contributes to **AC-10**. The lifecycle child owns interruption outcomes.
