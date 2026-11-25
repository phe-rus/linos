# Deployment lifecycle and failure rules

## Summary

Keep one current healthy deployment and a complete healthy fallback. A candidate
becomes selectable only after its code, configuration compatibility and boot assets
are durable. Recovery handles unsafe or ambiguous cases without changing current
personal or service data. The storage/boot and records children provide the
concrete mount and wire formats for this proposed contract.

## Transaction state machine

| State | Entry condition | Permitted next state | Durable invariant |
| --- | --- | --- | --- |
| InstallPending | Initial target copied and verified; no prior healthy deployment | Healthy(baseline), RecoveryRequired | First boot proves identity and unclaimed setup; independent Recovery is available |
| Healthy | Recorded current deployment and boot assets validated | Preparing | One known healthy boot path and authoritative configuration exist |
| Preparing | One volume lock; candidate identity and space plan accepted | Staged, Aborted | Healthy system, Users and service data remain untouched; incomplete files are private |
| Staged | Candidate and all matching boot artifacts copied, verified and synced | Trial, Aborted | Target selection is not yet exposed as a boot default |
| Trial | Durable intent binds candidate, fallback, config source and transaction before counted entry publication | Healthy(candidate), RollbackRequired, RecoveryRequired | Two attempts maximum; retries cannot recreate or reset the counted entry |
| RollbackRequired | Candidate trial attempts exhausted or matching failed-trial recovery intent | Healthy(fallback), RecoveryRequired | Current Users/State preserved; fallback must pass current compatibility checks |
| RecoveryRequired | Missing/ambiguous identity, unsafe data/configuration, no intact eligible boot set, or unrecoverable state write | Repaired Staged or Healthy through explicit Recovery action | No speculative blessing, automatic data restore or overwrite of remaining healthy artifacts |
| Aborted | Unpublished candidate abandoned or staging failed | Healthy | Unreferenced candidate material may be reclaimed; fallback and current data remain |

Healthy retains the current and previous healthy deployment/entry pairs explicitly.
The records child defines their atomic update on approval and rollback. The source
and currently selected configuration generation remain explicit throughout Trial.
There is no implicit inference from the largest filename or last directory mtime.

Use the volume lock for each durable step and compare phase plus expected journal
generation before writing. Preparing, Staged, Trial, RollbackRequired and
InstallPending exclude every competing package/update/rollback/GC operation until
resolved or explicitly aborted through the state machine. A lock is not held by
a process across reboot; the durable phase provides that exclusion.

Owner requested rollback from Healthy uses a fresh operation `rollback` through
Preparing, Staged and Trial against the retained eligible target. It does not
enter RollbackRequired directly. That state is reserved for reconciling a failed
trial. A request made during an unresolved trial is refused pending its explicit
Recovery reconciliation, never treated as another competing rollback operation.

The new layout needs its own journal format. Existing format 1/2 records remain
readable only by their existing implementation until an explicit conversion is
specified. Reuse generation checking and durable write behavior, not a permissive
parser that treats missing new fields as old defaults.

The layout record binds the common parent disk and selected Linos volume identity.
The deployment binds system content, kernel, initramfs, application selection and
extension set. The initial extension set is explicitly empty; it is not a guessed
bundle format. Stage 4 owns extension discovery, trust and compatibility details.

EFI and Linos are separate filesystems; there is no cross filesystem atomic rename.
Publication must use ordered durable intents and receipts. Persist candidate
artifacts first, then Trial intent, then the counted boot entry. Reconcile observed
firmware filenames on retry. A missing entry after a publication receipt requires
inspection; it does not permit a fresh two attempt budget.

Health approval requires the observed boot identity, root ancestry, configuration
selection and current state compatibility to match the transaction. Retain the
native blessing sequence: persist intent, call systemd blessing for the exact
observed entry, verify the stable entry and sync EFI, then commit the journal.
An uncertain write outcome is reread before another mutation.

The initial InstallPending check verifies mount/image identity, valid unclaimed
device state, completed identity initialization and a running enrollment process
offering setup. It does not require an enrolled owner or the post-enrollment ready
marker. An update trial instead requires the existing enrolled owner/account,
setup completion, valid persistent identity/hostname, login service and the native
kernel ready handshake when provided. CI must prove actual enrollment/PAM login;
a runtime health receipt does not claim to test a user's secret password.

Every managed trial entry masks `systemd-bless-boot.service`; only the Linos health
process may call the native blessing helper after its own checks. Validate
`LoaderBootCountPath` against the exact candidate entry pair and recognized counter
states before any approval. Refuse independent auto blessing rather than assuming
the native generator will wait for the Linos service.

A manual selection of a different deployment during an unresolved trial stops
before mounting writable configuration/Users/State into the real root. It is not
fallback evidence. Recovery may explicitly cancel/reconcile that trial, checking
current data before creating an authorized rollback transaction. Exhausted trial
entries alone permit automatic fallback to the recorded healthy pair.

For detected boot failure or failure to meet the initial 120 second health deadline,
do not bless; request a bounded reboot to the boot manager. Kernel panic uses a
10 second reboot delay. A completely hung kernel needs a VM reset in qualification;
no unqualified hardware watchdog is promised. Reboot/reset consumes the existing
firmware counter, never a newly published entry or fresh attempt budget.

## Snapshot meaning, retention and full storage

System history is a set of references to complete software deployments and their
configuration provenance. It excludes current Users, account state and service
databases. A historical configuration is source material for a merge, not the
configuration to mount unchanged during rollback. System history is not a backup.

Proposed initial default: current plus one healthy fallback, with at most one
unpublished or trial candidate. The active deployment, fallback, any pending trial,
its boot assets and independent Recovery are protected from automatic deletion.
Additional administrator pinned history is allowed only after explicit space
admission. Any optional pruning removes only unreferenced history, oldest first.
One serialized writer updates references and garbage collection.

Garbage collection follows journal and history references to the base manifests,
signatures, reviewed plans, adapter and repository evidence needed for retained
local deployments. Keep that metadata even when an unreferenced old system image
is removed. No retained deployment may become unverifiable through collection.
Every active and dormant service record also pins its contract descriptor,
path/owner map and authentication evidence independently of retained deployments.

Before a transaction, derive required capacity from signed system/boot lengths,
download/cache needs, staging conversion requirements, the configuration copy and
filesystem metadata margin. The storage child fixes that formula and
its reserve constants. Do not count sparse logical holes or future compression as
guaranteed free capacity. Check EFI separately from Linos.

If admission or a later write fails, leave selection unchanged and abandon only
unpublished material. No automatic deletion of personal files, service databases,
current software, fallback or Recovery is a space recovery mechanism. If the
journal cannot be durably updated, preserve evidence and use Recovery inspection.
Report available/required space and the exact affected volume.

## Recovery and offline ownership

One Recovery environment owns installation, repair and rollback. Its installed
partition is `Recovery`; it does not contain a required nested directory named
`recovery.pherus.org`. Its code and essential boot assets do not depend on a healthy
Linos root. If EFI or the physical disk fails, an external USB boot remains the
recovery route; independence does not imply protection from all disk failures.

The USB root follows the owner selected `EFI/` and `recovery.pherus.org/` tree.
The latter owns the Recovery environment, offline payload and transport metadata.
The storage child must define its actual filesystem and any file chunking required
by that filesystem. Neither current payload format is silently reinterpreted.
Payload identity, exact sizes, complete content and external trust must be checked
before publishing any installed boot selection. Hashes alone are not authenticity.

Initial installation and USB to internal EFI transfer use this publication order:

1. Identify the source USB ESP from the observed firmware boot partition and the
   Recovery loop backing file, requiring the same source ancestry. For installed
   Recovery, require its direct root PARTUUID and recorded ESP ancestry instead.
   Do not confuse the two modes. Ambiguous/duplicate ancestry is a refusal.
2. Inspect a distinct blank disposable guest target. Confirmation binds its stable
   device identity, capacity, sector size, source ESP identity and authenticated
   payload hashes into a SHA256 inspection token. Reinspect immediately before
   writing; any change invalidates confirmation. Never accept a token for a path
   alone, the boot disk, an in use target or an unknown physical target.
3. Generate target UUIDs once and persist an installation resume receipt on the
   source's disk backed workspace. Before formatting, record the confirmed blank
   identity and intended layout. On retry, match that receipt to observed target
   UUIDs/progress and source/payload identity. Resume verified copies; never treat
   a partly installed target as blank or automatically reformat on mismatch.

   The first USB implementation uses `recovery.pherus.org/workspace/` for the
   bounded resume receipt, keeping all authenticated boot/payload files unchanged.
   Require that workspace writable and synced before target mutation; if it is
   unavailable, refuse rather than hold the only receipt in RAM. Qualification
   uses a disposable copy of the USB artifact. Guest target selection additionally
   requires the fixture serial `linos-install-target` on a QEMU guest; this does
   not enable installation on physical machines.
4. Inventory the working EFI configuration and its architecture. Copy supported
   regular files through bounded paths, not links or device nodes. Keep the
   original USB boot artifacts untouched. Resume receipts may use the explicitly
   writable disk backed workspace without modifying those boot artifacts.
5. Regenerate target selectors from the new internal UUIDs. Do not copy a source
   USB PARTUUID or an arbitrary stale root path into the installed boot entry.
6. Copy and independently verify Server and Recovery artifacts. Initialize the
   target device identity as unclaimed, device keys, configuration generation 1,
   empty/dormant service registry and reserve file. Do not create an owner account.
   Flush all files/directories and verify their recorded identities from the target.
7. Persist InstallPending and its receipts before publishing the stable initial
   OS entry. Publish independent Recovery entry/boot files first, then the OS entry,
   flush EFI, and make the internal loader discoverable last by its final rename.
   Never publish `EFI/BOOT/BOOTX64.EFI` or a firmware entry to an incomplete target.
8. Verify installation on a disposable virtual disk, remove USB, boot internally
   and then boot independent Recovery. Only these observations establish the
   completed transfer for that candidate.

Editable EFI remains an owner interface. Before Stage 4 defines a bundle loader,
preserving a custom file does not claim that an OS driver was recognized or loaded.
Unknown additions are inventoried and kept inactive rather than guessed compatible.
Secure Boot, production key provisioning and extension signing remain separate gates.

## Failure matrix

| Interruption or failure | Safe outcome and required check |
| --- | --- |
| Payload signature, architecture, hash or length mismatch | Refuse before selection; preserve healthy deployment and current data |
| Duplicate fields, unsupported format or unknown deployment identity | Refuse parsing; do not infer an old layout or accept partial metadata |
| Full Linos or EFI during staging | No boot default change; reclaim only unreferenced staging after confirming journal state |
| Crash before candidate rename/sync | Partial material stays private; healthy deployment remains selectable |
| Rename succeeded but sync returned an error | Read actual durable state before retry; no assumed commit or second transaction |
| Trial intent durable, EFI publication interrupted | Reconcile the matching transaction and observed entry; retain fallback; never reset trials |
| One or two failed candidate boots | Count through existing firmware mechanism; exhausted candidate leads to eligible fallback |
| Candidate system image or kernel missing/corrupt | Refuse that complete deployment; select intact eligible fallback or independent Recovery |
| Kernel/modules/application set mismatch | Do not combine parts from another deployment to make it boot |
| State mount absent or wrong volume/disk ancestry | Stop before identity creation, login, service writes or health approval |
| New account/password/hostname during trial | Retain latest configuration; fallback merge must preserve changes |
| Configuration merge conflict or UID/GID collision | Refuse activation or enter Recovery; do not replace current credentials or renumber accounts |
| Service data outside fallback reader range | Block rollback/startup; retain data until an explicit safe migration or separate restore |
| Boot succeeds but mandatory health fails | No blessing; use remaining trial budget and eligible fallback |
| Blessing succeeds before journal commit is interrupted | Matching durable receipt permits reconciliation without blessing another deployment |
| Owner manually selects old entry before trial exhaustion | Do not declare automatic fallback or reset candidate attempts |
| Competing stage/GC while Trial survives reboot | Reject through durable phase and expected generation, regardless of process lock lifetime |
| Local package catalog conflicts with a new official base | Refuse/review the exact resolution; no silent package removal |
| Candidate writer would change the registered service schema | Reject before startup; initial implementation permits schema preserving writes only |
| Package supplies an unclassified file beneath State `/var` | Refuse the adapter/plan; never copy staged `/var` over current data |
| Initial install stops between artifact copy, journal and loader publication | Resume only matching recorded target; no discoverable OS entry before durable InstallPending, no automatic reformat |
| Recovery partition damaged | Retain ordinary eligible boot path; use unchanged external USB for repair |
| Linos encrypted and unavailable/wrong secret | Recovery still boots; no placeholder filesystem or identity is created |
| Lost encryption recovery secret | Explain that reinstall cannot recover encrypted user data; no bypass key is manufactured |
| EFI copy incomplete during installation | USB remains working; target is uncommitted and must not be presented as installed |
| USB removed after successful install | Internal OS and Recovery must each boot without loading USB paths |
| Unknown old layout or migration interrupted | No automatic repartition; original source stays intact and import is resumed or discarded explicitly |

Each row becomes a Stage 3 negative or interruption test tied to the corresponding
acceptance criterion in `index.md`. File sync and simulated crashes are development
evidence; they do not establish physical power loss behavior.

## Migration constraints

Retain the existing five partition tests while the three partition implementation
is developed on new disposable virtual disks. New parsers and health validation
must have explicit layout version dispatch. Only retire the old test path after
the replacement preserves all current boot, trial, fallback, identity and Data
sentinel guarantees, plus real enrolled account preservation.

Do not shrink or repartition an existing disk as an implicit upgrade. A future
import copies from a quiesced source to a new target, verifies numeric owners,
credentials, current service schema and content, then boots the target before
offering retirement of the original. Physical installation remains disabled in
this stage. Host orchestration never writes a real host block device.
