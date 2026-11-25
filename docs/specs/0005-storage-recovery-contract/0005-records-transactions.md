# Record formats and managed transaction interfaces

## Summary

Extend the existing strict text record approach with explicit kinds and versions.
Bind each boot to a complete deployment and each mutation to one journal generation.
Keep official candidate signatures distinct from owner authorized local package
changes. This is the wire and interface contract for Stage 3, not implemented APIs.

## Parsing and identities

Records use UTF8 ASCII subset `key=value` lines, one final newline, no comments,
duplicate keys, unknown keys, NULs or embedded newlines in values. Keys have a
fixed order when hashed or signed. Scalar records are at most 64 KiB, values at
most 4096 bytes. Integers are canonical decimal without a sign or leading zeros,
except the literal zero. Counters use u64, schemas u32, hashes lowercase SHA256,
UUIDs normalized lowercase. Reject overflow before any mutation.

Indexed lists use a declared `count` and contiguous four digit indices, starting
at `0000`, with no missing or surplus entries. A service list is limited to 128
records. A package plan is limited to 10,000 resolved package records and 16 MiB;
it is a separate bounded file, not squeezed into a scalar record. IDs used in
paths are hashes, decimal generations or identifiers matching
`[a-z0-9][a-z0-9._-]{0,63}`. Paths are derived under fixed managed roots, not read
as arbitrary absolute paths from manifests.

## Record schema

Every field below is required. `none` is an explicit tagged absence, not an
empty field or parser default. Canonical key order follows the order listed.

Store layout at `V/State/var/lib/linos/storage/layout`, the new journal at
`V/State/var/lib/linos/updates/v3/journal`, configuration records under
`V/State/var/lib/linos/configuration/<generation>.record`, and service records
under `V/State/var/lib/linos/services/<id>.record`. Receipts live beneath the v3
journal directory. Preserve old paths unchanged. The journal alone selects the
authoritative configuration; receipt phase markers are reconciled to it, not
treated as competing selection pointers.

Generate `volume_uuid` from the kernel random UUID interface during installation.
Disk GUID, PARTUUIDs and filesystem UUIDs come from the new GPT/filesystems and
must match observed devices before the layout is accepted.

Keep a public bootstrap copy of the layout at `/efi/EFI/Linos/volume/layout` so
independent Recovery can validate its own root and ESP without mounting Linos.
It contains identifiers/public key digests only. Persist and verify both copies
before publishing installation; normal OS operation requires them to agree with
the observed devices. This is consistency metadata, not Secure Boot attestation.

| Kind and version | Fields after `kind`, `format` |
| --- | --- |
| `linos-layout`, 2 | `volume_uuid`, `disk_guid`, `esp_partuuid`, `recovery_partuuid`, `linos_partuuid`, `filesystem=ext4`, `filesystem_uuid`, `encryption=none\|luks2`, `luks_uuid=none\|UUID`, `device_key_sha256`, `baseline_manifest_sha256` |
| `linos-deployment`, 3 | `origin=ci\|local`, `base_manifest_sha256=none\|hash`, `plan_sha256=none\|hash`, `source_sha`, `build_id`, `release`, `architecture`, `hardware_profile`, `kernel_release`, `config_schema_min`, `config_schema_max`, then bytes/hash pairs for `system`, `kernel`, `initramfs`, `inventory`, `applications`, `services`, `accounts`, and `extensions` |
| `linos-configuration`, 1 | `generation`, `deployment_id`, `parent_generation=none\|u64`, `schema`, `merge_receipt_sha256`, `phase=prepared\|selected\|retired` |
| `linos-service-state`, 1 | `service_id`, `schema`, `attachment=active\|dormant`, `contract_sha256`, `migration_receipt_sha256=none\|hash` |
| `linos-transaction`, 3 | `generation`, `transaction`, `operation=install\|update\|packages\|rollback\|repair`, `phase`, `healthy_id`, `healthy_entry_transaction`, `previous_healthy_id=none\|hash`, `previous_entry_transaction=none\|u64`, `candidate_id=none\|hash`, `candidate_entry_transaction=none\|u64`, `selected_deployment_id`, `source_config_generation`, `selected_config_generation`, `trial_budget=2`, `publication_receipt_sha256=none\|hash`, `health_receipt_sha256=none\|hash` |

Phase validity follows the lifecycle child; not every field combination is valid.
For first installation, `healthy_id=none` is allowed only for operation `install`
before its baseline is verified. Phase `InstallPending` follows durable copy and
precedes first boot verification. Publish one initial stable entry with independent
Recovery available. First boot verifies image/mount identity and the unclaimed
setup service before recording Healthy. Failure requires Recovery, not an invented
healthy fallback. Once a healthy baseline exists it cannot become absent.
Subsequent update trials require enrolled identity.

An entry identity is its deployment ID plus entry transaction, not a deployment ID
alone. Derive the stable basename from that pair. `healthy_entry_transaction=none`
is allowed only before the first baseline exists. Previous deployment and entry
fields are either both absent or both present. First installation seeds config
generation 1 before InstallPending; its source and selected generation are both 1.

On candidate approval, atomically set previous healthy/entry to the old healthy
pair, set healthy/entry to the observed candidate pair, retain selected current
configuration, clear candidate and commit Healthy. Do not lose the old pair during
blessing reconciliation. After failed trial fallback, keep the original healthy
pair and its older retained fallback; retire the failed candidate only after the
rollback receipt. An explicit later rollback trials the retained target under a
new entry transaction, and on success makes the formerly current healthy pair
the new previous pair. It never changes the target's signed manifest.

Publication receipts bind transaction, source journal generation, source config
generation, healthy/candidate entry pairs, exact canonical entry bytes and boot
file digests. Health receipts additionally bind the observed firmware counted or
stable path, selected config generation and transition decision. Files live under
`updates/v3/receipts/<transaction>/`; hash exact canonical record bytes in the
journal. Receipt field names follow these bindings with the same strict parsing
rules. No receipt for another transaction or entry can complete reconciliation.

`selected_deployment_id` is the authority for current software and desired catalog.
Update it atomically with `selected_config_generation` before real init; that
configuration record must name the same deployment. Initially both refer to the
baseline/generation 1. During staging and before a candidate boot they remain on
the running healthy deployment. A trial boot selects the candidate pair; exhausted
fallback selects the healthy deployment with a newly merged current configuration.
An aborted unpublished transaction leaves selection unchanged. No mutable catalog
directory can override this selector. The original plan remains retained if a
trial fails, but its unaccepted desired changes do not silently become active.

The immutable `services` file contains indexed service IDs, persistent path
registrations, schema reader min/max and writer schema. The immutable `accounts`
file contains required system service names, stable numeric UID/GID assignments
and exact supplementary group names/GIDs/memberships for those accounts. Each
indexed account declares its primary group plus a bounded indexed supplementary
membership list; reject missing/duplicate/conflicting name or numeric assignments.
It contains no passwords or owner account databases. `applications` records
the desired catalog selection and resolved package versions. `extensions` is a
canonical empty set in the first implementation. `inventory` is the exact dpkg
installed inventory. Bound these signed metadata files to 16 MiB each.

`source_sha` remains the last official source commit for a local composition;
`origin=local`, the base manifest and plan identity prevent representing it as
official CI bytes. The package plan also records repository metadata identities
and exact package digests. `release` is the official base release sequence, retained
by a local composition. A separate increasing local transaction controls boot
entry ordering; never edit a signed official manifest to advance a device counter.
Official upgrades advance the current official base release; explicit rollback
may select an older retained eligible deployment through a new authorized transaction.

## Authentication and signing domains

Canonical `SHA256SUMS` lists manifest, system image, kernel, initramfs and all five
metadata files in that order using fixed relative names. Verify an Ed25519
signature over `LINOS-DEPLOYMENT-V3` followed by byte LF and those exact checksum
bytes before trusting their hashes or manifest fields. The prefix applies only to
the new format; existing format 2 authentication remains unchanged.
The signature is 64 bytes. Signatures never authenticate a key shipped alongside
the same untrusted payload by default.

Official CI candidates require the independently supplied development/release
public key. Production signing and lifecycle remain the existing separate gate.
Owner package compositions cannot obtain or use a CI private key.

At explicit installation, generate a device Ed25519 key using native OpenSSL.
Store its private material root only, mode 0600, under State trust storage and
cover it with Linos encryption when that variant is supported. The public key
digest is bound in the installed layout, with its public bootstrap copy on EFI
and a receipt retaining the independently verified official baseline manifest.
Do not put a private key in EFI, USB payloads or CI artifacts.

A local deployment must verify under this installed device key, declare
`origin=local`, and link an existing trusted base and reviewed plan. No arbitrary
payload key becomes trusted through `stage`. This is administrative authorization
on the installed device, not a hardware trust root or release certification.
Editable unsigned EFI and plaintext State are not protected against a physical
attacker by this design. Rotating the device key requires an explicit Recovery
operation with current key authorization or independent owner recovery credentials;
it is unavailable until separately specified and tested.

### Recovery and offline signing domains

`recovery-manifest` is a canonical `kind=linos-recovery`, `format=1` record with
`source_sha`, `build_id`, `architecture`, `hardware_profile`, `kernel_release`,
and bytes/hash pairs for `recovery_img`, `kernel`, `initramfs`, `usb_uki`, `efi_loader` in that
order. `recovery-manifest.sig` signs the exact bytes `LINOS-RECOVERY-V1`, byte LF,
and that manifest. Root length is at most 3 GiB; kernel and initramfs are each at
most 128 MiB and the USB UKI at most 384 MiB. The manifest is outside the artifacts
it hashes, so no self hash is required.

The EFI loader is the exact authenticated `EFI/BOOT/BOOTX64.EFI` copied from USB
to the internal ESP, bounded at 16 MiB. Store the unchanged signed Recovery manifest
and signature at `/efi/EFI/Linos/recovery/recovery-manifest` and
`/efi/EFI/Linos/recovery/recovery-manifest.sig`, in addition to installation
receipts. Installed Recovery uses these ESP copies and its embedded key, without
requiring readable Linos. USB verification checks every declared source artifact;
installed verification checks loader, kernel/initramfs and signed root prefix.
The unused USB UKI is provenance, not an executable internal entry.

The offline `manifest` uses `kind=linos-offline`, `format=3`, then `architecture`,
`hardware_profile`, `deployment_id`, `recovery_manifest_sha256`,
`system_bytes`, `system_sha256`, `count`, and indexed part `bytes`/`sha256` pairs.
Next come bytes/hash pairs for deployment manifest, its checksums/signature,
kernel, initramfs and the five deployment metadata files in canonical order.
`manifest.sig` signs `LINOS-OFFLINE-V3`, byte LF, and the exact manifest. Part names
are derived from their indices; count is 1 through 4. Verify both the independent
transport signature and original deployment signature before reconstruction or
installation. A matching whole image hash alone cannot authorize either record.
The deployment ID must equal the referenced deployment manifest hash, and the
Recovery manifest digest must match the independently verified running Recovery.

Recovery and transport accept only independently provisioned CI/development keys
with those roles, never the device composition key or a key from the payload.
The test harness supplies the development public key outside the untrusted
payload and pins it into the built Recovery initramfs and Recovery root. The USB
early verifier uses that embedded key for Recovery image checks; Recovery userspace
uses its embedded key for offline manifest and official deployment checks.
Installed Recovery retains the same verifier/key through the authenticated copied
artifacts. Public keys read from USB/EFI cannot expand this role allowlist.

Verify the source root/kernel/initramfs/UKI against the Recovery signature before
copying. Recheck hashes from the target Recovery partition's first signed root
length and the installed EFI boot files, then record their identities before any
boot publication. Keep the signed manifests/signatures in the installation receipt
set. Recovery integrity checks do not authenticate execution against a physical
attacker without separately qualified Secure Boot.

## Commands and value sources

Extend existing tools through explicit layout 2 entrypoints. The host image
backend continues to accept only private regular file workspaces. Only the
Recovery process inside the test guest may target an inspected virtual disk.

| Surface | Inputs | Outputs and their sources | Authorization/refusal |
| --- | --- | --- | --- |
| `recovery inspect` | Observed devices, boot ancestry | Layout/identity/space from GPT, superblocks and filesystem statistics | Inspection only, never writes |
| `recovery install` | Verified format 3 payload, separately trusted key, inspected guest target and confirmation token | Volume UUIDs from newly created filesystems/GPT, receipt hashes from copied bytes | Root in explicit disposable guest mode only; physical install disabled |
| `linos system packages plan` | Operation, package IDs, active inventory, trusted repository metadata | Exact versions/digests from native APT resolution; space from the storage formula; schema constraints from package adapters | Privileged review for private metadata, unsupported package/script refused |
| `linos system packages stage` | Plan identity, expected journal generation | Local deployment identity from sealed manifest bytes; receipt from verified staged content | Root with native administrator authorization and one lock |
| `linos-update-image stage-v2` | Workspace, signed deployment, independent key and expected generation | Staged ID from verified manifest; unchanged healthy ID from journal | Regular file development backend only; old format remains distinct |
| `linos-update-image activate-v2` | Workspace and staged ID | Trial identity and entry from journal/manifest/recorded UUIDs | No nested trial or implicit attempt reset |
| `linos-update-health` | Actual kernel parameters, firmware boot entry, observed mounts and journal | Health/rollback receipt bound to actual transaction and configuration | Root; no caller identity or path overrides |
| `recovery rollback` | Explicit retained ID, current state registry, journal generation | Eligibility from current config/service compatibility; new selection receipt | Refuse incompatible data; no automatic data restore |
| `recovery repair` | Inspected failed record/artifact and independently verified replacement | Revalidated artifact and explicit repair receipt | No implicit reformat or overwriting current Users/State |
| Status | Observed volume and bounded records | Selected/healthy/candidate IDs, phase and blockers from the journal and observed artifacts | Mark missing information; do not invent healthy state |

Text output is human readable; exit codes are 0 success, 2 invalid/unsupported
input or format, 3 busy/stale generation, 4 insufficient space, 5 integrity/trust
failure, 6 data/config incompatibility, 7 I/O outcome requiring inspection.
Sensitive file contents, passwords and keys never enter receipts or terminal logs.

## Offline package worker

Initial package support is limited to reviewed package adapters and their exact
dependency closure. An adapter declares conffiles, allowed maintainer scripts,
service accounts and State schema requirements. Unsupported packages fail during
planning. This is deliberately narrower than claiming universal Debian package
compatibility. Reuse native APT dependency resolution and dpkg installation.

The plan binds the active deployment, journal generation, desired catalog digest,
selected base manifest, repository metadata, all resolved package digests, adapter
digests and expected service/account changes. Any changed binding makes it stale.
Official upgrades re-resolve the current desired catalog on the new official base.
If choices cannot be preserved, refuse with the exact conflict; do not drop them.
An accepted recomposition is device signed with origin local and the new official
base/source identities. An empty unmodified catalog may consume the official image
directly. Retain manifests, signatures, key bindings, plans and adapter/repository
evidence transitively needed by every referenced deployment, even if an old base
image itself is collected. Verify that chain before staging or activation.

The worker mounts a private writable staging image, creates private mount, PID,
UTS, IPC and network namespaces, pivots to the staging root, closes inherited
host descriptors and exposes only minimal device nodes and its own proc view.
Set mount propagation recursively private before creating any child mounts, so
neither mounting nor teardown can propagate to the running system. After setup,
retain only CHOWN, DAC_OVERRIDE, FOWNER, FSETID, SETUID, SETGID and SETFCAP capabilities;
drop all others from effective, permitted, inheritable, bounding and ambient sets.
A package needing more privilege is unsupported. No host root, block devices,
live State or active software image is writable
inside the worker. This is an isolation contract to test, not a promise to safely
execute arbitrary hostile root code against a shared kernel.

Download and verify the exact reviewed package set first. The offline worker has
no external network. Suppress daemon starts using policy helpers and reviewed
script adapters. Package database and files change only together inside staging.

Review the entire resolved closure, including dependency maintainer scripts,
triggers, alternatives, diversions and initramfs hooks, not only the requested
package's scripts. The adapter digest fixes the supported closure; a changed
script/trigger or unexpected path effect aborts before publication.

Each adapter classifies every package supplied path hidden by a writable mount,
including `/var`, `/usr/local` and `/srv`. `immutable` paths are exposed from the
selected image via validated readonly binds; `seed` paths use a declared
create-if-absent initializer with exact owner/mode/content; `state` paths are
registered service data. Existing seeds/state are never overwritten by copying
the worker's tree. An immutable/state collision or unclassified path is a refusal.
Create all bind targets before exposing the package database, and validate the
complete map before a service starts. Only explicitly required declared immutable
files can mask State; never mask a whole database or populated state subtree.

Use vendor configuration for composition, plus reserved account IDs and sanitized
account placeholders where needed. Never put current shadow contents, SSH host
keys or personal configuration into the sealed system image. Generate new vendor
defaults and system service account requirements from the reviewed adapter output,
then merge latest live configuration separately at activation. A package requiring
unknown configuration side effects or real database access is unsupported until
its adapter is designed and qualified.

Before a managed activation, require clean filesystem, coherent dpkg inventory,
matching kernel modules, valid schema contracts, complete signature and adequate
space. Direct running system APT mutations are rejected by an early policy check
before hooks/scripts, backed by read only package/system mounts. Test both
`apt-get` and direct `dpkg` mutation paths. Merely changing a Store hint does not
meet this contract.

This contract supplies the record and interface values required by **AC-2** through
**AC-6** and supports the Stage 3 migration/qualification obligations of **AC-10**.
