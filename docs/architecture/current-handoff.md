# Linos current handoff

Updated 25 November 2026. Start here and check the exact CI runs before making a release claim. This supersedes the 23 November handoff.

## Commit checkpoint and path to local layout 2 VM testing

The owner authorized committing the accumulated Stage 3 work. Before this
checkpoint, all 113 library, Recovery and update integration tests passed with
one compiler job and sequential tests. Strict library/Recovery Clippy including
tests, formatting, diff checks, shell syntax and the disposable GPT/metadata CLI
fixture also passed. No CI dispatch or local VM boot was performed for this
checkpoint; the new layout is not yet assembled into a bootable candidate.

To reach the first meaningful local layout 2 VM test, complete these dependencies:

1. Immutable service/account metadata validation, current configuration/account
   checks and the configuration selection/merge path.
2. Initial installation and exhausted fallback receipts/reconciliation, plus
   native health and blessing coordination around the existing journal APIs.
3. Rust boot helper and initramfs integration for the loop root, persistent binds,
   `/run` survival, layout dispatch and independent Recovery ancestry.
4. CI assembly of the signed format 3 Server/Recovery/offline artifacts and the
   three partition disposable guest installer. Reuse verified kernel artifacts.
5. Download the exact CI candidate and test sequentially here: USB Recovery,
   fresh virtual disk installation, USB removal, internal Server and independent
   Recovery, then enrollment, reboot, trial/fallback and data preservation.

An existing layout 1 VM cannot establish those layout 2 guarantees. Managed APT
composition, retention/full storage and the remaining failure matrix follow the
first bootable installation milestone. Release/security and physical support
qualification stay separate. No push was requested for this commit checkpoint.

## Planning checkpoint for the next conversation

### Active implementation continuation after commit 7730bc1

The owner explicitly requested completing the four prerequisites for local layout
2 VM qualification, not merely another planning/status pass. Work after the
commit is ongoing and uncommitted. Keep building toward that journey, with
bounded focused checks while building and broader checks at integration.

Current continuation has reached a compilable native early boot helper and CI
generic deployment builder. `linos-layout2` implements fixed-path local-top
partition/layout/signature preflight, exact read only loop attachment,
local-bottom persistent binds and real-init receipt/mount verification. The
package installs conditional initramfs hooks and a sysinit gate; a legacy image
without the explicit layout marker/trust key keeps its existing path. A candidate
needing a new configuration selection or fallback reconciliation still refuses
in the top-level helper; the generation/publication coordinator is unfinished.
Do not call those integration paths implemented just because the libraries exist.

Recovery and offline signing-role authenticators and bounded sequential chunk
reconstruction now compile; real ephemeral Ed25519 tests reject substituted
domains/keys and malformed signed metadata. New native service registry loading
checks retained signed source metadata, exact descriptor hashes and observed path
ownership. Prepared generation composition joins merge/account planning and native
offline account creation, but durable tree/record publication remains to be wired.

`build/layout2-deployment.sh` runs OS construction only in CI. It restores the
verified native base at source `717b45066d47c7509e1054f2b2d232e5aff5ceac`, installs
the current userspace package, provisions only an independent development public
key, regenerates initramfs, removes factory machine/SSH/TLS secrets, seals/signs a
format 3 image and authenticates its metadata and ext4 bytes. No device private key
or installed machine state is put into this generic artifact. New workflow
`layout2.yml` supports development branch `layout2-vm` with marker `layout2:vm`;
it preserves the strict OS vulnerability gate after retaining development bytes.
The source archive is still available, API digest
`sha256:e3985e5aff59fb53801f23838d888e5497920e36fabd3ea136389e77ef27417a`.
Local checks passed 142 library/init/Recovery/update tests, strict Clippy including
tests, formatting, shell syntax and Actionlint. No VM or new OS build ran locally.
CI deployment construction is the next execution step, not VM readiness or a
completed USB/disposable target installer. Keep working toward that owner request.

Development source checkpoint `163a0e4` was committed and pushed to `layout2-vm`.
Its first CI run [45620611338](https://github.com/phe-rus/linos/actions/runs/45620611338)
failed before image construction: runner Clippy 1.100 requires `as_chunks` for two
constant UTF16 chunk iterators that local Clippy 1.96 accepted. Both were corrected
without changing the bounded decoding rules. Prepared configuration tree/receipt
publication now has a real filesystem test proving source preservation, durable
publication without premature journal selection, idempotent retry and stale writer
refusal. It passed locally. Candidate/fallback top-level boot coordination still
needs to call this new publication API; no local VM qualification is implied.

Current additions: strict signed account/service list parsers and authenticated
metadata reads; native passwd/shadow/group/gshadow cross checks; collision and
reserved identity planning; offline native groupadd/useradd creation with locked
new service credentials; three input configuration merge and a descriptor based
private tree writer preserving current credentials, keys, metadata and deletions.
Seven focused metadata/merge tests passed. Unsupported ACLs/xattrs/special files
refuse rather than disappear during copying. Native account tool calls have not
been exercised in a guest. The root directory mode is included in tree snapshots,
so a selected /etc does not inherit the private workspace's mode 0700.

Fallback receipt, durable fallback intent, native verification, commit and
exhausted-entry retirement/retry APIs now compile. Their new integration cases
still need focused tests. Approval also needs receipt backed retirement of the
older previous entry after a subsequent successful trial; otherwise the strict
boot inspector would encounter an unreferenced managed entry. Resolve this before
claiming repeated update/boot readiness. These helpers remain disconnected from
live boot/installation services.

Actual retained candidate evidence supersedes the scout's config-only concern:
`images/server/builds/amd64/20261124.1/rootfs.tar` contains
`boot/config-6.18.63-linos` with `CONFIG_BLK_DEV_LOOP=y`, ext4, FAT/VFAT and EFI stub
built in. Loop is inherited from defconfig, not explicitly listed in the overlay.
Do not rebuild Linux solely for missing overlay text. Existing candidate/source
and package provenance still must be verified when reusing artifacts in CI.

Next implementation dependencies: prepared configuration generation publication;
native boot/health coordination and initial installation receipts; Rust helper
with initramfs local-top/local-bottom hooks; generic layout marker and real-init
mount verification; Recovery/transport format 3 signing, verification/chunking;
CI assembly plus disposable target installer and local sequential QEMU journey.
Debian's retained `/usr/share/initramfs-tools/init` already moves `/run` and its
child mounts into the real root after local-bottom, which can preserve the volume.
Use native operations/Rust policy, shell orchestration only, and no Python.

Read [the platform review and staged execution plan](../plans/2026-11-25-platform-contracts-and-execution.md)
before implementation. The owner requested review, planning and staging, then a conversation clear.
The agreed direction is USB `EFI/` plus `recovery.pherus.org/`, and installed `EFI`, `Recovery`,
and `Linos` partitions. Linos has logical System, Applications, Users, State and Snapshots roles.
These are target contracts, not implemented layout changes. Filesystem, extension format and
rollback/data mappings still need design. Recovery owns install, repair and rollback.

Kernel and full OS builds belong in CI; reuse verified artifacts for local VM acceptance.
Use focused bounded local tests and sequential VMs, and do not duplicate expensive CI work.
Publication must promote qualified bytes separately from test runs. No builds, boots, workflow
dispatch, platform implementation or publication were performed for this planning checkpoint.

The owner currently has no physical test machine. Continue VM development while retaining the
physical qualification boundary. Start next with artifact provenance/reuse and execution routing,
then the storage/recovery contract, following the staged plan. Server remains the first edition.

## Stage 1 execution gate passed, security gate remains open

Stage 1 artifact provenance and execution routing passed its development review.
See [candidate artifacts and execution](candidate-artifacts.md) for the implemented
contract, exact candidate identities and evidence. Kernel selection uses `kernel/candidate.conf`
or an explicit manual run/source pair, with successful boot and artifact digest
verification. Server candidates are captured before tests. Installer media consumes
the captured OS archive instead of rebuilding Server. Compiler and VM caps are explicit.

Local archive/API rejection tests, shell syntax, Actionlint, six kernel tooling tests
and 42 library/Recovery tests pass. A real AMD64 kernel download passed artifact and
package checksums and the existing provenance verifier against this checkout.
Implementation and planning documents are committed and pushed at
`717b45066d47c7509e1054f2b2d232e5aff5ceac`. Native run
[45553431687](https://github.com/phe-rus/linos/actions/runs/45553431687) completed:
both architectures passed functional checks and failed only final OS vulnerability
enforcement. AMD64 passed enrollment, firmware boot, six update/fallback boots and
independent Recovery. The separate downstream development job restored the original
AMD64 archive, built its offline payload and UEFI media without rebuilding Server,
and booted it to setup. The overall run remains failed. Kernel compilation and
publication were skipped. Repository security run
[45553413542](https://github.com/phe-rus/linos/actions/runs/45553413542) passed.

The AMD64 archive was downloaded using repository/run/source/API digest verification,
restored with all internal hashes checked, and smoke booted locally with one vCPU,
1024 MiB and no external network. This passed sign-in/first-boot identity checks.
Artifacts and logs are in `target/stage1-ci/`; restored bytes are under
`images/server/builds/amd64/20261124.1/`. This is development execution evidence,
not final security, interactive local enrollment or physical qualification.

Fresh scans retain the same 26 advisory IDs: AMD64 has 85 package matches (82 HIGH,
three CRITICAL), ARM64 83 (80 HIGH, three CRITICAL). No fixed package versions are
reported. The existing security maintenance decision remains unresolved; no gate
was relaxed. Stage 2 storage/recovery design is next, before any layout changes.

## Stage 2 accepted storage and Recovery contract

The [assembled Stage 2 specification](../specs/0005-storage-recovery-contract/index.md)
has owner design approval, with four implementation contracts and a source/alternatives
record. The owner explicitly selected ext4 deployment images, current data preservation
with incompatible rollback blocked, managed APT deployment transactions, optional
Linos volume encryption with the plain path first, and the five record ownership model.

The proposal specifies 1 GiB EFI, 4 GiB Recovery and the rest Linos on a 32 GiB
minimum development virtual disk, read only loop mounted system images, current
configuration generations, service schema checks, versioned signed records and
owner local package provenance. It defines USB Recovery without a third root folder,
offline image chunking, EFI transfer, retention/space reserves and an interruption
matrix. These details are design, not behavior implemented in the existing images.

The independent cross check completed and found six gaps: durable fallback/trial
authority, service registry consistency, keyed account/configuration merging,
Recovery/transport trust and installation ordering, owner packages across official
upgrades, and package files hidden by writable mounts. The owner approved all six
recommended fixes. The four child contracts and index include them. An independent
closure check identified five alignment issues, which were resolved and checked
again with no direct contradiction found among the revised rules. The rationale
retains the findings/resolutions. The owner accepted the assembled spec on
25 November 2026 and authorized proceeding. Stage 2 design is complete; the spec
status tracks the still unbuilt Stage 3 implementation.
No storage code, kernel input, disk image or VM was changed or
built for this design. The five partition development baseline stays active.
Security maintenance and physical qualification remain open. Begin Stage 3 only
from the reviewed specification and retain the current baseline until parity passes.

## Stage 3 first slice: layout 2 geometry

The accepted Stage 2 design is committed and pushed at `3b1ba72`.
Stage 3 implementation has begun with `system/src/recovery/layout/v2.rs` and the
read only `recovery plan-v2 DISK_BYTES SECTOR_BYTES` command. The planner computes
three aligned inclusive sector ranges for 1 GiB EFI, 4 GiB Recovery and the Linos
remainder, retaining 16 MiB for alignment/GPT at a 32 GiB development minimum.
Geometry supports 512/4096 byte logical sectors; neither proves installation support.
The old `plan /dev/DEVICE` and five partition calculation remain unchanged.

Nineteen Recovery Rust tests pass, including four new boundary/overflow/alignment
tests. Strict Recovery Clippy, formatting and shell syntax pass. The new
`tests/layout-v2.sh` creates a fresh sparse regular file under disk backed `target/`,
uses the Rust plan to write GPT with `sgdisk`, validates both GPT copies and all
three names/ranges/type codes, and rejects a device path as numeric plan input.
Evidence is retained at `target/layout-v2-tests/`; the sparse disk is removed.
Actionlint passes, and the lightweight `kernel:validate` workflow now runs this fixture.

Source checkpoint `eed46c2474fedd874c18e6b5452a364400dfde4d` is committed and pushed.
Lightweight validation run
[45563935326](https://github.com/phe-rus/linos/actions/runs/45563935326) passed the
artifact checks, disposable GPT fixture and 77 focused Rust tests. Kernel build,
boot, OS integration and publication jobs were skipped. Logs are retained locally
at `target/layout-v2-tests/ci-validation.log`. Repository security run
[45563935348](https://github.com/phe-rus/linos/actions/runs/45563935348) passed both
Rust advisories and repository scanning. This is separate from the existing failed
OS package security gate.

This is geometry evidence only. No filesystems, boot helper, layout 2 record
parser, installation or VM boot is implemented by this slice. No host block device
was written; no full kernel or OS image build ran locally. Build step 1 remains
partial: strict records/signatures and observed loop/bind ancestry are next.
Security, physical installation and release gates remain open.

## Stage 3 second slice: strict layout 2 records

The working tree now includes `system/src/recovery/records.rs`, exposed through
the library and `recovery check-layout-v2 RECORD`. It validates the accepted
13 field layout schema in canonical order, bounded ASCII text, lowercase UUIDs
and SHA256 digests, three distinct partition UUIDs and explicit encryption fields.
The bounded file reader refuses final symlinks, hard linked files, directories,
FIFOs and oversized records. It checks file identity across open; it does not
validate parent directories as trusted storage or authenticate mutable metadata.

Plain records can be inspected. LUKS2 metadata is recognized by the parser but
refused by the command because encrypted deployment remains unqualified. Output
explicitly distinguishes syntax from authentication and reports that observed
disk identity, the EFI copy and mount ancestry have not been checked.

Local verification passed with one compiler job: 60 library, Recovery and update
integration tests, including eight new record tests; strict library/Recovery
Clippy; formatting; shell syntax; and the expanded disposable GPT/CLI fixture.
The fixture verifies exact canonical output and exit code 2 for duplicate fields,
encrypted metadata, a symlink and a FIFO. Logs remain under
`target/layout-v2-tests/record-*.txt`. Existing lightweight validation already
runs this fixture and the library tests. No new CI run was dispatched for this
slice, and these source changes are not yet committed or pushed.

Build step 1 remains partial. Next implement deployment 3 records and signing
domain verification, then observed loop/bind ancestry and immutable image checks.
Layout metadata is consistency information, not a signed deployment or proof of
mount identity. The five partition baseline and disabled installation remain in
place. No kernel/OS image build, VM boot or host block device write was performed.

## Stage 3 third slice: deployment 3 metadata and official signatures

`system/src/recovery/records/deployment.rs` now provides the strict 29 field
deployment parser and `OfficialManifest::authenticate`. The parser enforces
canonical unsigned numbers, overflow and schema ranges, source/target identities,
origin/provenance combinations and the accepted size/hash bounds for all eight
artifacts. Its target comparison requires architecture, hardware profile and
kernel release to match caller supplied expectations.

The official verifier takes bounded manifest, checksum, signature and independently
trusted public key bytes. Native OpenSSL verifies Ed25519 over the exact
`LINOS-DEPLOYMENT-V3` prefix plus LF and checksum bytes before trusting metadata.
It then requires the nine fixed checksum names/order, verifies the exact manifest
SHA256 identity and compares every artifact digest with the parsed manifest.
Small sealed memory files provide seekable snapshots to OpenSSL, without a
caller controlled scratch directory. No system image is loaded into memory.

Local provenance can be parsed, but this official verifier refuses local origins.
Device key binding and retained base/plan chain authentication remain unimplemented.
This slice is a library interface, not a staging, installation or activation
command. The caller remains responsible for independently provisioning its trust
key. No payload key discovery or trust expansion is implemented.

Local verification passed with one compiler job: 70 library, Recovery and update
integration tests, including ten new parser/authentication tests; strict Clippy
including tests; formatting; and diff checks. Tests use real ephemeral Ed25519
keys/signatures and reject other signing domains, legacy signatures, wrong keys,
wrong algorithms, altered metadata, malformed even correctly signed checksum
lists and conflicting origins. The previous format 2 update tests still pass.
Existing lightweight CI includes these library tests, but no new CI run was
dispatched. Changes remain uncommitted and unpushed.

Build step 1 remains partial. Next add actual artifact length/hash and immutable
ext4 checks, then observed loop/bind ancestry against layout 2. Authenticated
metadata alone does not prove artifact content, filesystem or mount identity,
service/account metadata validity or boot readiness. Keep the five partition
baseline and installation disabled. No kernel/OS image build, VM boot or host
block device write was performed in this slice.

## Stage 3 fourth slice: actual artifact and ext4 validation

`OfficialManifest::verify_artifacts` now opens all eight fixed artifacts relative
to a supplied directory descriptor, without following final symlinks or blocking
on FIFOs. It requires a caller owned private directory and standalone regular
files owned by the caller without other writers, then checks actual lengths and
SHA256 hashes against the authenticated manifest. Hashing streams bounded input
through native OpenSSL; growth cannot turn it into an unlimited read. Images
stay on disk, never in an in memory snapshot.

The returned `VerifiedArtifacts` retains the exact file descriptors and deployment
identity. A replaced pathname does not substitute another file. `revalidate`
checks those retained identities again; owner writes remain possible and must
be excluded by the future volume lock/activation path. This API does not validate
directory ancestry, the volume layout or selected transaction. Runtime root callers
require root ownership; development fixtures run under the current UID.

The system file must probe as ext4. Native `dumpe2fs` under the C locale must
report clean state without needs recovery, an external journal or an unidentified
journal. Native `e2fsck -f -n` then checks its structure without repairs. A second
hash verifies that checking did not change authenticated bytes. Retained file
descriptors are passed to the native tools through procfs, never reopened through
the original payload names. No loop attachment or mount is performed here.

Local verification passed: 78 focused library, Recovery and update integration
tests with one compiler job and sequential test execution; strict Clippy including
tests; formatting/diff checks; and Actionlint for both changed workflows. Eight new
artifact tests include all artifact tampering, missing/linked/FIFO/directory inputs,
other writers, pathname substitution, owner mutation, ext2 rejection, genuinely
unclean/needs recovery images and a clean header with a corrupt allocation bitmap.
The last cases are signed with their actual damaged content hashes, so refusal
exercises filesystem checks rather than only digest disagreement. Tests compare
before/after hashes to prove no repair occurred. A targeted follow up also passed
the added sparse overlimit hash refusal assertion. Fresh 32 MiB fixture images
live under disk backed `target/deployment-artifact-tests/` and are removed afterward.

Lightweight and full CI now explicitly install native storage/OpenSSL test tools
before tests and set `RUST_TEST_THREADS=1`. These workflow changes passed local
linting; no new CI run was dispatched. All four record/artifact slices remain
uncommitted and unpushed.

Build step 1 remains partial. Next implement observed loop/bind ancestry against
layout 2, then integrate these checks with the boot helper, journal selection and
metadata schema validation. Current checks do not make owner writable files
permanently immutable or establish actual read only loop/mount state. Keep the
five partition baseline and installation disabled. No full kernel/OS build, VM
boot, mount or host block device write was performed for this slice.

## Stage 3 fifth slice: loop and bind mount ancestry inspection

`system/src/recovery/mounts.rs` now implements the separate layout 2 inspection
path. It parses bounded `/proc/self/mountinfo`, including kernel path escapes,
mount IDs/parents, filesystem roots and both mount/superblock access modes.
Required mounts must be unique and visible beneath the expected parent. It
checks the whole read only ext4 root with `noload`/`norecovery`, the whole writable
Linos and EFI mounts, the selected configuration generation and every fixed
Users/State/package database bind from the accepted ownership map.

The native collector opens only the fixed helper reference `/dev/linos-system`
after confirming the expected root mount shape. Its read only Linux
`LOOP_GET_STATUS64` query must match the root device and retained image's backing
device/inode, with no offset, size limit or loop encryption and with the loop's
read only flag set. It resolves the recorded Linos/ESP PARTUUID aliases to block
device numbers for comparison; this is not independent GPT identity validation.
Actual target path device numbers must agree with mountinfo. The loop is queried
again and the mount table reread to refuse changes during inspection.

State cannot supply the immutable dpkg/APT installed state. Nested mounts that
hide configuration, package metadata, EFI, private volume sources or immutable
system code are refused. The declared mutable `/usr/local` and service/runtime
mounts outside protected sources retain their distinct ownership policy.
The caller must supply the configuration generation from its locked journal.
This library interface does not create mounts, attach loops or approve health.

Local verification passed with one compiler job and sequential tests: 86 focused
library, Recovery and update integration tests; strict Clippy including tests;
formatting and diff checks. Eight new tests cover complete synthetic mount maps,
every missing/stacked/wrong source managed mount, parent ambiguity, protected
overmounts, writable root/loop, journal replay, wrong image identity, stale config,
State package substitution, escapes, malformed/oversized input and Linux UAPI
structure size/offsets. The host's actual mountinfo parses, and a regular file is
refused as a loop device. Successful loop ioctl and bind inspection against a
real layout 2 guest have not been exercised. No existing host loop was inspected
or modified, and no mount or device write was performed.

The parser follows [kernel mountinfo semantics](https://www.kernel.org/doc/html/v6.15/filesystems/proc.html#proc-pid-mountinfo-information-about-mounts),
the [ext4 journal replay rules](https://www.kernel.org/doc/html/next/admin-guide/ext4.html)
and the Linux `linux/loop.h` UAPI. Existing lightweight CI includes these library
tests; no new CI run was dispatched. Changes remain uncommitted and unpushed.

Build step 1 remains partial. Next validate actual GPT identities/UUID uniqueness,
filesystem UUID/common disk ancestry and observed firmware source, then connect
the inspectors to boot/journal selection and metadata schema validation. Native
loop/bind behavior still needs disposable guest evidence. Keep layout 1 active
and installation disabled. No full kernel/OS build or VM boot ran for this slice.

## Stage 3 sixth slice: native disk, UUID and loader ESP consistency

`system/src/recovery/mounts/identity.rs` now collects a bounded native block
inventory from fixed sysfs paths. It opens the matching native block nodes read
only, verifies their device numbers and probes retained descriptors with native
`blkid -p`, without a lookup cache. Sysfs partition numbers and parent device
numbers must agree with the native GPT entry metadata. Layout selection requires
unique partition UUIDs for exactly EFI, Recovery and Linos on one whole GPT disk,
the correct partition numbers/types/names and FAT/ext4 filesystems, a unique
matching disk GUID and a unique matching outer Linos filesystem UUID.

The selected whole disk also passes native read only `sgdisk --verify`; diagnostics
or degraded primary/backup GPT copies are a refusal. All retained inventory
devices are re-probed before return to detect changed identifiers, not merely
changed device names. Sysfs inventory names and loader variable bytes must remain
unchanged during inspection. Inventory/output limits and unreadable or ambiguous
identity data produce a refusal rather than an arbitrary device choice.

The fixed systemd `LoaderDevicePartUUID` EFI variable is decoded as one bounded
UTF16 GUID after the efivarfs attribute header and must name the same unique ESP.
This follows the [systemd Boot Loader Interface](https://systemd.io/BOOT_LOADER_INTERFACE/).
It is loader source consistency, not Secure Boot or physical attacker attestation.
No UEFI BootCurrent path, selected deployment entry or counted boot receipt is
authenticated by this slice.

The existing mount inspector now uses the independently probed identities rather
than trusting `/dev/disk/by-partuuid` aliases. It checks both the State layout and
the public EFI bootstrap copy against the supplied expected layout before and
after inspection. The mounted ESP/Linos devices must match the native inventory.
The runtime API still applies only to normal installed Server root/bind mounts;
USB and independent installed Recovery ancestry need their own integration.

Local verification passed with one compiler job and sequential tests: 94 focused
library, Recovery and update integration tests; strict Clippy including tests;
formatting/diff checks; and workflow Actionlint. Eight new tests cover duplicate
UUIDs, cross disk ancestry, missing/wrong partition roles, extra partitions, stale
disk/filesystem/loader IDs, malformed EFI/export data and bounded inventories.
Native tests probe fresh 16 MiB sparse GPT and 32 MiB ext4 regular files under
disk backed `target/disk-identity-tests/`, including damage to each GPT copy and
actual filesystem UUID replacement. Before/after hashes verify that the inspectors
do not repair or rewrite them. A final identity test rerun and Clippy passed after
adding the retained device re-probe guard. Disposable files are removed afterward.

Block/firmware integration is still fixture evidence: the native sysfs collector
and EFI source check have not run in a real layout 2 guest. No host block device
was probed or written and no EFI variable was read or modified during these tests.
Full CI now explicitly installs gdisk before Rust tests; lightweight CI already
does so. No CI dispatch, full kernel/OS build, VM boot, commit or push occurred.

Next implement configuration/service/account metadata validation and durable
journal/boot entry selection, then wire the accepted checks into the boot helper
and initramfs. Actual layout geometry, loop/bind survival and native identity
collection require disposable guest acceptance before enabling installation.
Build step 1 and Stage 3 remain in progress. Keep layout 1 active, installation
disabled and the existing security/release gates unchanged.

## Stage 3 seventh slice: configuration and service State records

`system/src/recovery/records/state.rs` implements the specified configuration 1
and service state 1 scalar records using the existing strict ordered parser.
Configuration generations are positive, generation 1 has no parent, and later
generations reference an earlier positive generation. Parsing checks canonical
numbers, u32 schema bounds, hashes and explicit phase/attachment discriminators.
Service identifiers follow the specified bounded lowercase path identifier grammar.

Configuration selection checks the caller supplied journal generation/deployment
pair, selected phase and deployment configuration schema range. The caller must
provide the reconciled journal and authenticated manifest/digest pair. Service
schema preflight requires a reader range containing the durable current schema
and a writer equal to it, including dormant reattachment. These are separate
library checks, not startup authorization or activation commands.

Local verification passed with one compiler job and sequential tests: 99 library,
Recovery and update integration tests, including five new State record tests;
strict library/Recovery Clippy including tests; formatting and diff checks.
The new cases cover lineage, selection/phase mismatches, number overflow,
malformed records, dormant reattachment and incompatible readers/writers.

This slice does not validate live account databases, merge receipts, retained
service descriptors/path ownership or complete registry consistency. Immutable
service/account list validation, durable journal/boot selection and boot helper
integration remain next. The existing files were preserved; all current Stage 3
slices after geometry remain uncommitted and unpushed. No CI dispatch, full
kernel/OS build, VM boot or host block device write occurred in this slice.
Layout 1 stays active and installation remains disabled. Stage 3, security,
physical qualification and release gates remain open.

## Stage 3 eighth slice: format 3 journal and boot decision model

The owner requested faster implementation with broader testing deferred.
`system/src/recovery/records/transaction.rs` now parses/encodes the strict
transaction 3 schema and checks entry pairs, phase/selection consistency,
configuration lineage counters and the fixed trial budget. Typed transitions
cover installation approval, preparation, staging, trial intent/publication,
candidate configuration selection, approval, exhausted fallback, unpublished
abort and Recovery refusal. Approval retains the previous healthy entry pair;
fallback preserves both healthy pairs and requires a new configuration derived
from the latest selected generation. Retrying trial publication does not reset
the two attempts; matching receipt/config selection retries are idempotent.

The separate `records/transaction/journal.rs` backend uses a trusted directory
descriptor, an exclusive nonblocking lock, bounded standalone private regular
files, full expected snapshot comparison and typed events. Writes sync a private
pending file, rename, then sync the directory. Pending files are never authority;
uncertain write outcomes require rereading. It reuses the existing trusted
directory opener without changing the layout 1 journal format or behavior.
This library backend is not connected to the live updater or boot services.

Boot preflight distinguishes initial setup, stable healthy, counted candidate
and exhausted fallback decisions using exact deployment/entry transaction pairs.
Early manual healthy selection during Trial, unrecognized counters and a missing
publication receipt refuse. Firmware evidence is a typed caller input here, not
yet a native collector or authenticated receipt. A separate helper binds selected
configuration/schema checks to the authenticated official manifest identity.

`recovery check-transaction-v3 DIRECTORY` offers read only locked inspection,
canonical output and explicit trust/firmware/mount verification limitations.
There is no new CLI mutation command. The installer/health coordinator still must
verify artifacts, configuration/account contents, schemas, firmware entries and
receipt bindings before requesting transitions. Recovery repair transitions,
native entry filename/counter reconciliation, receipt formats/validators and
boot helper/initramfs integration remain unfinished.

Five focused transaction/journal tests pass, covering durable reopen, stale and
competing writers, pending files/symlink refusal, parser rejection, counters,
trial retries, fallback configuration and retained healthy pairs. Strict
library/Recovery Clippy including tests, formatting and diff checks pass. Broader
regression, process interruption and guest acceptance are deferred, not claimed.
All builds used one compiler job. No CI dispatch, full kernel/OS build, VM boot,
host block device write, commit or push occurred. Layout 1 stays active and
installation remains disabled. Next connect native boot/receipt reconciliation
and finish immutable service/account metadata and live configuration validation.

## Stage 3 ninth slice: native loader and entry preflight

`records/transaction/boot.rs` adds bounded kernel selector parsing, canonical
Type 1 entry rendering and native read only boot preflight. Selectors require
the specified read only loop root, layout 2, canonical volume/deployment/entry
transaction and unique managed fields. Entry validation checks exact deployment
kernel/initramfs paths, numeric version and sort key, selectors and the trial
blessing mask. Only stable, `+2`, `+1-1` and `+0-2` filenames are recognized.

Native inspection reads fixed `/proc/cmdline`, `LoaderEntrySelected` and
`LoaderBootCountPath` sources and protected files from `/efi/loader/entries`.
The selected identifier is the counterless name; the counter path must identify
the exact post-decrement entry. This was checked against the systemd boot source
and its boot assessment/interface documentation, not guessed from old filenames.
See [the source](https://github.com/systemd/systemd/blob/main/src/boot/boot.c),
[assessment](https://systemd.io/AUTOMATIC_BOOT_ASSESSMENT/) and
[interface](https://systemd.io/BOOT_LOADER_INTERFACE/).

The inspector rejects unknown managed entries/counters, duplicate stable/count
variants, missing healthy/previous paths, exposed unpublished candidates, early
manual fallback, a running unconsumed trial, wrong firmware identity and missing
trial masks. Stable candidates during unresolved Trial deliberately refuse pending
durable blessing receipt reconciliation. Inputs are read twice; directory identity
is checked across reopen. The journal's `inspect_boot` wrapper holds its lock and
checks its snapshot around collection. Observations bind the journal generation.

Five focused fixture tests pass, including numeric 9 to 10, exact selector/asset
bindings, firmware identity/counter conflicts, exhausted fallback, duplicate and
missing paths, independent blessing refusal and strict UTF16 variable decoding.
Strict library/Recovery Clippy including tests, formatting and diff checks pass.
Compiler jobs remained one. Broader regression and VM/native runtime acceptance
remain deferred under the owner's faster implementation direction.

This is a library collector and locked preflight interface, not active initramfs
or live health integration. Tests supplied firmware bytes and in memory entry
inventories; they did not read host EFI variables or probe host block devices.
The caller still must prove the ESP/layout ancestry, signed artifact contents,
configuration/service/account compatibility and receipts before mounting data or
blessing. Next implement durable receipt reconciliation, then connect the boot
helper/initramfs once remaining metadata/configuration checks are complete.
No CI dispatch, full image build, VM boot, commit or push occurred. Installation
remains disabled and layout 1 remains the working baseline.

## Stage 3 tenth slice: durable trial receipts and approval reconciliation

`records/transaction/receipts.rs` adds canonical, bounded trial publication and
approval intent receipts. Publication binds the original journal generation,
source configuration, healthy/candidate entry pairs, canonical entry bytes and
authenticated kernel/initramfs digests. Approval additionally binds its source
journal generation, selected configuration, publication receipt digest, exact
observed counted entry path and explicit approval decision. Parsing rejects
unknown versions/decisions, malformed identities and conflicting counters.
These local records provide consistency, not signatures or physical attestation.

`records/transaction/journal/receipts.rs` stores immutable digest named records
under `updates/v3/receipts/<transaction>/<digest>.record`. Private descriptor based
directories, bounded standalone files, file/directory sync and no replace rename
protect publication. Retry verifies existing exact bytes; pending files cannot
supply authority. Receipt bytes are synced before their journal pointer changes.
Health intent is now an explicit idempotent Trial transition before native blessing;
conflicting approval digests refuse rather than replacing that intent.

The journal approval recovery API loads the journal referenced publication and
health receipts, verifies the publication against the authenticated official
manifest and requires the exact health intent generation/selection. Native boot
inspection can then recognize that precise stable candidate as interrupted
approval. It verifies unchanged entry bytes, selectors, blessing mask and observed
firmware identity, plus actual EFI kernel/initramfs lengths and hashes. It syncs
EFI, reinspects and commits Healthy while retaining the previous healthy pair.
An unrelated or unapproved stable candidate still refuses. Both an original
counted firmware path and an absent volatile path after reboot are handled.

Four new focused tests cover canonical/stale/malformed receipts, private durable
storage and tampering, conflicting intents and interrupted blessing before/after
reboot with unchanged entry bytes. All 14 transaction/boot/receipt/journal tests
pass; strict library/Recovery Clippy including tests, formatting and diff checks
pass. Compilation used one job. Broader regression and guest acceptance remain
deferred. Native EFI hashing/sync/recovery methods compiled but were not executed
against the host or a guest; fixtures supply observed firmware and entry bytes.

This slice supports trial publication and approval receipts only. Initial install,
rollback/fallback receipts, complete native health checks, service/account and
live configuration validation, native blessing orchestration and initramfs/live
updater integration remain unfinished. The low level transition backend still
expects callers to supply verified evidence; the receipt coordinator adds checked
entrypoints but does not itself certify current accounts/services or root ancestry.
No CI dispatch, full image build, VM boot, commit or push occurred. Installation
remains disabled and layout 1 stays active. Next complete fallback reconciliation
and remaining metadata checks before enabling layout 2 boot services.

## Owner direction and sequence

Server only. First assess Linos against its product promise, Apple's published quality principles and established server platforms, then improve the demonstrated gaps. Run security and image qualification in CI, fix and rerun. Only after those gates pass, qualify a fresh native image locally, complete the beta support/recovery gates and publish an explicitly supported Server beta. Desktop and broader Assets/Store work are deferred until afterward.

The owner wants physical PCs, homelab machines, servers, Raspberry Pi and Orange Pi families as well as VMs. No exact physical test configuration has been provided or qualified. Do not silently narrow the requested beta to VM only or call an arbitrary ARM board supported because ARM64 QEMU boots.

## Assessment and implementation

- [Measured Server baseline and gaps](../reviews/2026-11-24-server-baseline.md): footprint, interaction timing, architecture, hardware awareness and audience suitability.
- [Primary source platform comparison](../reviews/2026-11-24-server-platform-comparison.md): Apple principles and Debian/Ubuntu/RHEL/Proxmox/TrueNAS reference points. This is not a measured competitor performance ranking.
- [Support matrix](server-support-matrix.md): exact qualification limits and required physical evidence.
- [Status and response spec](../specs/0003-server-status-response.md): bounded input batching, faster polling only while work is pending, and shared CLI/Home status checks.

The implementation preserves the existing visual direction, shell compatibility and authorization. Status now reports memory/root headroom, CPU/memory/I/O pressure and failed services, marking unavailable checks. A neutral Home greeting replaces the unconditional readiness claim. Advice does not mutate resources or claim failure prediction. Hardware identification remains distinct from qualification.

One 1 vCPU, 1024 MiB native AMD64 guest baseline showed Home idle RSS about 3.2 MiB and no idle Home CPU-counter change over the sample window. Installed root use was about 290 MiB; the sparse base image is logically 3 GiB. The improved package used about 3.35 MiB idle RSS. In the two observed runs, loaded Services navigation tail fell from 5.4 seconds to 217 ms, and idle simple-command median from 254 to 108 ms. Typing tail did not improve and single startup was slower. These are small serial-terminal samples, not physical display, minimum-RAM, production or comparative OS guarantees.

## Local evidence

Application checkpoint `d54889c12421cea7f0c312687b7238b6eac3e400` is committed and pushed. CI repair checkpoint is `307defdc4abcbeefc2d98683fcbf18e4720903ce`.

- Workspace: 135 passing Rust tests, four explicit guest/benchmark cases ignored in the ordinary run.
- Formatting, strict all-target Clippy and shell syntax checks pass.
- The rebuilt application passed the full Home interaction, ordinary administrator authorization and representative workload guest tests, each against a fresh disposable copy of native `20261123.4`.
- A separate repeat benchmark detected a deliberately failed systemd service through the new status CLI running as an ordinary user.
- All local VMs ran sequentially with one vCPU, 1024 MiB and guest loopback/no external network. Copies remain under disk-backed `target/`. Existing user VMs were preserved. No Python was used.

Artifacts: `target/server-benchmark/`, `target/console-qualification/guest-benchmark/`, and the three guest snapshot directories. Original benchmark snapshots are archived at `target/server-benchmark/baseline-evidence/`. The original timing data has summary distributions; the improved run also retains individual timing samples. `host-manifest.txt` records host/tool details after the runs. Unexposed host cgroup limits were not established.

The development package was rebuilt after selecting new build identity `20261124.1`, but a fresh native candidate image has not been assembled or qualified. Keep the owner's CI-first ordering.

| Prepared artifact | SHA-256 |
| --- | --- |
| `target/release/linos` | `b10fc363fc116bee513361106f512ddaf9156a2342362f840c8788729ec1ff6a` |
| `system/dist/linos13_dev_v1.0.0.deb` | `47097fa4486036c077dfe6373b90cac7d8b9ff11e67842d625e0101ff0d8ba8d` |

These are local development artifacts, not immutable release certification. Package version remains development 1.0.0, so use the build identity and hashes.

## CI repair and security blocker

The old full scan failed while tar attempted unprivileged device-node extraction; Trivy had not run. Extraction now excludes virtual runtime trees while preserving OS/package metadata. Native AMD64 qualification had passed enrollment, firmware boot and six update/fallback boots, then failed Recovery assembly because an unshared user could not traverse GitHub's private temporary directory. Its temporary storage now uses disk-backed `/var/tmp`.

| Run | Verified result |
| --- | --- |
| [45476639614](https://github.com/phe-rus/linos/actions/runs/45476639614), native installed kernel at `307defd` | Both AMD64 and ARM64 passed, including AMD64 Recovery |
| [45477917782](https://github.com/phe-rus/linos/actions/runs/45477917782), repository security at `d54889c` | Passed |
| [45476628685](https://github.com/phe-rus/linos/actions/runs/45476628685), full compatibility image CI at `307defd` | Both architectures completed functional checks and failed the OS vulnerability gate; downstream Recovery/installer media jobs were skipped |
| [45479117710](https://github.com/phe-rus/linos/actions/runs/45479117710), final full compatibility image CI at `3996db4` | Refreshed 25 November: both architectures failed final OS vulnerability enforcement; downstream Recovery/installer media jobs skipped |
| [45479141286](https://github.com/phe-rus/linos/actions/runs/45479141286), final native image and package-security qualification at `3996db4` | Refreshed 25 November: both architectures failed final native OS vulnerability enforcement |

The repaired AMD64 scan produced 85 package/CVE matches, 82 HIGH and three CRITICAL rows, covering 26 unique identifiers. All lack a fixed stable package version in the report. The three CRITICAL rows are the same OpenSSH source advisory, not three independent remotely exploitable server flaws. Exact binary reachability needs review. The image already includes stable updates and security repositories; this is not solved by adding the missing security repository.

Raw report: `target/server-benchmark/ci-security-amd64/os-vulnerabilities.json`, SHA-256 `35ef43f1902364710782ddfee358e9ea4059d7d4c5afea3d3073a521d1243668`. `security-findings.psv` holds the per-row summary. Use [the vendor triage](../reviews/2026-11-24-server-security-triage.md) and [the proposed maintenance decision](../specs/0004-server-security-maintenance.md). Normal stable refresh cannot currently clear this strict gate. No advisory was suppressed, no ignore-unfixed option was added and no base distribution was changed.

Native-kernel image qualification now includes the same OS package scan and retained reports, since full CI scans explicit compatibility images. The final run reached and failed the enforcement step on both architectures. It does not establish advisory coverage of the custom kernel itself. Functional tests still run after vulnerability findings, then the gate fails.

## Remaining work

1. Settle security maintenance: reviewed reproducible backports and candidate-specific applicability evidence, or wait for stable vendor fixes. The maintenance spec is Proposed. Do not silently start owning a new package supply chain or relax the gate.
2. Rerun CI at the final source and triage every finding/error. The qualification path must never use the `kernel:release` publication marker just to run tests.
3. After the security gates are satisfied, build fresh native `20261124.1` with its new package/dependency set. Qualify actual enrollment/PAM login, SSH and noninteractive SSH behavior, reboot persistence, Recovery and signed development update/data preservation against that exact candidate. Existing mutable package tests cannot replace it.
4. Qualify actual named physical configurations and remaining installer/data/recovery guarantees. Physical installation remains disabled. Production signing, account/service-data migration, power-loss testing, sustained workloads and support lifecycle are not established by this turn.
5. Release beta only with a supported matrix and explicit candidate evidence. Desktop and broader Assets/Store work remain outside this scope.

The owner confirmed no physical test machine is currently available. The security-maintenance path remains unresolved. A target family is not an available device. Neither elapsed time nor no response grants a new maintenance-policy decision. The new platform plan governs future architectural work; the release/security gates above remain open.
