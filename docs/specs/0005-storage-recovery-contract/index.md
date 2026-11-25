# 0005. Storage ownership and unified Recovery

**Date**: 2026-11-25
**Status**: Proposed
**Design approval**: Accepted by the owner on 25 November 2026. Implementation has not started.

## Summary

Linos will use the agreed EFI, Recovery and Linos partitions while preserving
ordinary Linux paths. System history will record complete software deployments,
while personal files, credentials and service data remain current. Recovery will
install, repair and select a compatible deployment. This is the accepted Stage 2 design contract. Runtime implementation and
qualification remain Stage 3 work.

## Structure

* [Storage and boot](0005-storage-boot.md) fixes ext4 image deployment, geometry,
  early mounts, offline Recovery and space admission.
* [Data and package ownership](0005-data-ownership.md) defines the persistent path
  owners, configuration generations, managed package transactions and rollback rules.
* [Deployment lifecycle](0005-deployment-lifecycle.md) defines transaction states,
  retention protections, Recovery ownership and the failure matrix.
* [Records and transactions](0005-records-transactions.md) fixes record schemas,
  signing domains, command values and worker isolation.
* [Rationale and source inventory](rationale.md) records the current implementation,
  the storage alternatives and the decisions already confirmed by the owner.

These children form the owner accepted Stage 3 build contract. The status tracks
implementation, so design acceptance alone does not mark the feature built.

## Requirements

* **AC-1**: Installed media has three GPT partitions named `EFI`, `Recovery` and
  `Linos`. System, Applications, Users, State and Snapshots are logical roles
  within Linos. No visible System A/B or Installer volume is added.
* **AC-2**: Rolling back system code preserves current personal files, account
  credentials and service data. An incompatible data reader prevents activation
  or requires Recovery; it never silently restores old data.
* **AC-3**: Administrator package changes use managed deployment transactions.
  No package transaction partially modifies the running managed system.
* **AC-4**: Filesystem, mount ancestry, deployment identity, configuration and
  boot assets are validated before mounting persistent data or approving health.
* **AC-5**: The booted kernel, initramfs, system, application selection and enabled
  extension set belong to one recorded compatible deployment.
* **AC-6**: Interrupted staging or activation preserves the last healthy option.
  Retries cannot reset a published trial's two boot attempts.
* **AC-7**: Recovery boots independently of a healthy Linos filesystem, offers
  offline installation and transfers the working USB EFI configuration to the
  installed EFI through validated, recoverable copying.
* **AC-8**: Retention and space checks protect the current and required fallback
  deployments. Full storage produces a bounded refusal and recovery guidance.
* **AC-9**: Encryption is optional for the Linos volume. The plain VM path is
  qualified first; an unimplemented encryption choice cannot be presented as supported.
* **AC-10**: The replacement meets the existing update, identity and recovery
  evidence on disposable virtual disks before retiring the working baseline.
  Physical installation remains disabled.

## Decision

Confirmed owner requirements in this conversation:

1. Preserve current data on OS rollback; block incompatible rollback until the
   service has a safe migration or restore path.
2. Apply administrator APT changes through a managed deployment transaction.
3. Define optional Linos volume encryption, qualifying the plain VM path first.

4. Use ext4 with immutable deployment images inside the Linos volume.
5. Use the Volume, Deployment, Configuration generation, Service state and
   Transaction ownership model below.

The proposed geometry is 1 GiB EFI, 4 GiB Recovery and the remainder Linos, with
a 32 GiB minimum for the initial VM qualification. Reuse the existing kernel only
after input provenance verification; the new initramfs/layout still need CI.
Btrfs and LVM are deferred alternatives, never implicit validation fallbacks.

The existing specs `0001` and `0002` continue to govern the working five partition
development path. This draft does not supersede their implementation or change
spec `0004` security maintenance policy.

## Feature design

The owner confirmed this ownership model. The records child fixes its encodings.

| Record | Identity and required values | Relationships |
| --- | --- | --- |
| Volume | Unique volume UUID, three distinct PARTUUIDs, filesystem/encryption format and format version | One volume has many deployments, configurations and transactions |
| Deployment | Unique content identity, source/build/architecture, signed software hashes, application selection, extension set, supported configuration and service schemas | Belongs to one volume; many configuration generations may be prepared for it |
| Configuration generation | Monotonic ID, owner deployment, parent generation, directory, migration receipt and durable selection state | Parent references the previous authoritative generation; at most one selected generation per volume |
| Service state | Unique service ID, data paths, numeric schema, accepted reader/writer contracts and migration receipt | Data is current across deployments; deployment manifests name compatible contracts |
| Transaction | Monotonic ID, journal generation, operation, source/target deployment, config generation, phase, trial and health receipt | At most one active writer per volume; references an existing healthy fallback |

All values are required unless explicitly marked absent by a format discriminator.
For example, plain volumes have `encryption=none` and no cryptographic key slot
metadata. The records child fixes all wire schemas and explicit absent values.

## Value sourcing

| Produced value | Source |
| --- | --- |
| Geometry, mount paths, retained count | Constants in storage/boot and lifecycle children |
| Observed volume identity and free bytes | GPT, superblocks, sysfs/loop ancestry and filesystem statistics compared with layout |
| Deployment ID and code/boot hashes | Authenticated format 3 manifest and exact content digests |
| Official source/kernel identity | Stage 1 candidate provenance, checked kernel package and signed manifest |
| Local package versions and origin | Reviewed plan, APT repository/package digests, device signed composition and trusted base |
| Account/configuration contents | Latest journal selected configuration generation |
| Data schema and rollback eligibility | Current State registry and selected service contracts |
| Trial count and durable phase | Journal generation and observed firmware entry, reconciled under lifecycle rules |
| Health decision | Actual selectors/mounts, native readiness and identity/config/schema checks |
| Installed Recovery selectors | Inspected target UUIDs and trusted Recovery artifacts, not USB source selectors |
| Encryption availability | Implemented/qualified capability; the initial plain path reports encryption unavailable |

## Build plan

Stage 2 design is accepted. Stage 3 implements these slices:

1. Add strict layout 2/deployment 3 parsers, geometry planning, signatures,
   loop/bind ancestry and immutable image checks with negative fixtures,
   satisfies **AC-1**, **AC-4**, **AC-5**.
2. Build a plain AMD64 candidate in CI. Boot USB Recovery, install on a new virtual
   disk, remove USB and boot internal Server and independent Recovery, satisfies
   **AC-1**, **AC-7**, **AC-9**.
3. Integrate configuration generations, enrolled account preservation, State
   mounts and service schema preflight, satisfies **AC-2**, **AC-4**.
4. Adapt updater/health to complete new boot sets. Exercise trial exhaustion,
   fallback, interrupted blessing and data preservation, satisfies **AC-2**,
   **AC-5**, **AC-6**.
5. Add reviewed package plans, the isolated worker, device signing and mutation
   guards. Test staging/removal without active root changes or data deletion,
   satisfies **AC-3**, **AC-5**.
6. Test retention/full storage and every failure matrix row. Retain candidate
   diagnostics and compare baseline guarantees before retirement, satisfies
   **AC-8**, **AC-10** and **AC-6**.

Review driven acceptance cases also include: stable fallback references after
blessing; source/selected config on retry; numeric boot ordering 9 to 10 and local
compositions of one base release; blocked early manual fallback and independent
blessing; dormant service removal/reattachment; keyed password/service account and
metadata/deletion merges; Recovery/transport signature role substitution; every
initial install publication boundary; catalog re-resolution and provenance GC;
and package seed/immutable path classification under writable mounts.

## Consequences

Managed package changes may require a reboot. Retaining a healthy software fallback
does not guarantee every future database schema can be rolled back. The filesystem
choice cannot remove that compatibility requirement. Configuration migration adds
work beyond changing GPT geometry.

## Migration plan

**Strategy**: Versioned replacement on fresh disposable virtual disks. Keep the
existing implementation and tests until equal and additional qualification passes.
No automatic in place repartitioning or physical installation.

**Rollback**: Original images and layout remain intact. New format errors refuse
rather than reinterpret an old journal. Unknown old state requires explicit import.

**Risks**: Loop root boot, configuration merge, service contracts and package
worker isolation are new work. AMD64 installation evidence cannot qualify ARM64
installation or physical devices. Keep architecture specific qualification.

## Follow-up

* Implement and qualify Stage 3 against this accepted contract.
* A separate encryption activation spec must fix cipher/key derivation limits,
  provisioning, header backup and tests before enabling the optional encrypted path.
* Device extension loading, native app packaging and production signing retain
  their separate stages/gates. No package backport policy is approved here.

## Rationale

Reasoning, alternatives and source evidence: [rationale.md](rationale.md).
