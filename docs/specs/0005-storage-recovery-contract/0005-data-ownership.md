# Data and package ownership

## Summary

Software deployments describe the code that boots. Current user files, credentials
and service databases have separate owners and never change merely because older
code is selected. Configuration is prepared from the latest state for each boot,
so rollback cannot resurrect an old password or remove a recently created account.

This proposed child uses the selected ext4 images and confirmed record model.
It contains no implemented mount changes.

## Ownership and Linux paths

`V` means the private Linos mount at `/run/linos/volume`, fixed by the boot child.
These paths express ownership within the selected ext4 volume.

| Item | Logical owner and backing location | Linux interface | Update and rollback rule |
| --- | --- | --- | --- |
| OS binaries, shared libraries, base units and vendor defaults | System, selected immutable deployment | `/usr`, merged `/bin`, `/sbin`, `/lib*` and vendor root skeleton | Replace through a deployment; fallback uses its complete matching software set |
| Package database and installed package metadata | System, selected deployment | `/var/lib/dpkg`, `/var/lib/apt/extended_states` | Must change with its package files; never share a mutable database across different system roots |
| Package application selection | Applications, `V/Applications/catalog/` | Linus store inventory and managed package CLI | Derived readonly view of the journal selected deployment's signed `applications` file; not a second mutable authority |
| Independently packaged future applications | Applications | Future runtime path, not selected in this stage | No independent app runtime or format is silently introduced; later app spec must define it |
| Personal files and preferences | Users, `V/Users/<name>/` | `/home/<name>` | Current across OS changes; no automatic data revert or deletion |
| Root administrator files | Users, `V/Users/root/` | `/root` | Current and root private; not part of OS snapshots |
| Current machine configuration | State, `V/State/config/<generation>/` | A directory mount at `/etc` | Prepare from latest authoritative generation using the rules below |
| Account/password/group databases, sudo configuration, subuid/subgid mappings and SSH host keys | State, selected configuration generation | Ordinary paths inside `/etc` | Preserve current contents, modes, UID/GID assignments and keys; never restore an older copy as a side effect of OS rollback |
| Device identity, owner, pending enrollment and provisioning state | State, `V/State/var/lib/linos/` | `/var/lib/linos` | Current across all eligible deployments; a missing mount is an error, not permission to create new identity |
| Machine ID and entropy seed | State configuration and State var | `/etc/machine-id`, `/var/lib/systemd/random-seed` | Preserve current machine identity and seed; do not replay older copies from OS history |
| Setup completion, remembered login and machine settings | State configuration | `/etc/linos`, `/etc/hostname`, timezone/locale/network configuration | Current; reconcile through a known compatible transformation, otherwise refuse the switch |
| Persistent service databases, container data and application state | State, `V/State/var/` and explicitly registered paths | `/var/lib/<service>`, `/var/spool`, registered `/srv` paths | Current; services may start only with a compatible state contract |
| User installed local binaries and site configuration | State, `V/State/local/` | `/usr/local` | Current and administrator owned; not overwritten by a base image or silently certified compatible |
| Site data | State, `V/State/srv/` | `/srv` | Current; register service compatibility before automatic rollback can restart its service |
| Service and boot logs | State, `V/State/var/log/` | `/var/log` | Retain under bounded log policy; never treat logs as system snapshots |
| APT lists, downloaded packages and other caches | State cache directories | `/var/lib/apt/lists`, `/var/cache` | Disposable cache; does not determine installed package identity |
| Runtime mounts and sockets | Ephemeral | `/run`, `/dev`, `/proc`, `/sys` | Recreated; `/var/run` and `/var/lock` retain their standard runtime links |
| Large temporary files | State, `V/State/tmp/` | `/tmp`, `/var/tmp` with separate sticky directories | Disk backed; cleanup never enters Users, service data or deployment directories |
| Update journal, locks, receipts and state contracts | State, `V/State/var/lib/linos/updates/` | `/var/lib/linos/updates` | Root private, generation checked, synced and durable across deployments |
| Historical software references | Snapshots | Managed history view, hidden from the ordinary top level | Preserve complete referenced deployments and configuration provenance; not a backup of current Users or databases |

`/var` may be backed by State, but the selected deployment's package database and
APT installed state must be mounted over the corresponding State paths as read
only. Mount order is mandatory and checked before any package command or service
starts. Old leftover package databases in State must never become authoritative.
The backing volume is not exposed as a user writable bypass around these mounts.

## Configuration generations and accounts

Do not bind mount individual `passwd`, `shadow`, `group` or `gshadow` files from
another directory. Native account tools use replacement and rename operations.
Mount the complete selected `/etc` directory, retaining those ordinary operations.

A configuration generation is mutable while it is the live `/etc`. Its stable
numeric identity is not a content hash. The deployment's vendor defaults remain
immutable and versioned separately. The journal records which generation is
authoritative, including during a trial or interrupted switch.

Initial generation 1 is seeded from the verified baseline's vendor defaults before
InstallPending is published. It has no owner, no copied personal credentials and
locked baseline system accounts. Generate fresh device/machine identity and SSH
host keys in persistent State/configuration, not in the immutable image. Later
enrollment modifies this selected tree with native account tools.

For ordinary configuration, compare base vendor (B), latest current (C) and target
vendor (N) objects. Equality includes object type, bytes or link target, numeric
owner/group, mode and supported ACL/xattrs. If C=B take N; if N=B keep C; if C=N
keep C; otherwise stop with a conflict. Absence is a value, so an unchanged vendor
file deleted locally stays deleted, a vendor deletion of an unmodified file takes
effect, and a deletion conflicting with an edit is not silently resolved. Refuse
ancestor deletion/type changes that would discard retained descendants. Do not
follow symlinks while merging; identity/account files must be regular files.

The four native account databases are exceptions to file level merging. Parse
them by account/group name and verify unique names, UID/GID assignments and cross
references. Preserve every current account's credential, password aging, UID/GID,
home, shell and current memberships unless an explicit reviewed account action
authorizes a change. New vendor data cannot restore an old human password or
grant a human user new membership by default. Preserve current SSH host keys.

For a newly required service account absent from both old vendor and current
configuration, use its signed declared UID/GID, a locked native shadow entry,
no login shell and only declared service memberships. Create it in the prepared
configuration with native tools after reserving IDs. A current name/ID collision,
an administrator deletion of a previously existing required account, or a required
privilege change for an existing account is a conflict for review, not automatic
recreation, renumbering or privilege escalation. Retain removed service accounts,
groups and numeric ownership so their retained data cannot be reassigned to a new
principal. Never recycle those reserved IDs automatically.

At the switch boundary, before normal logins or services start:

1. Take the volume transaction lock in the boot environment. Quiesce the previous
   runtime before deriving a new configuration. Do not merge from an old staging
   copy while live accounts can still change.
2. Read the latest authoritative configuration and the old/new vendor defaults.
   Perform an explicit three input merge: unchanged administrator files take new
   defaults, administrator only edits remain, and conflicting edits block activation.
   A change in both inputs is not silently resolved by timestamp or file ordering.
3. Merge account records using the keyed rules above without reverting credentials,
   user names, UID/GID ownership or authorization policy. Validate references among
   passwd/group/shadow/gshadow and existing Users directories before selection.
   New package service accounts must not collide with any current or reserved ID.
   A collision fails the transaction; existing accounts are never renumbered.
4. Reconcile hostname, device owner and setup records. Preserve current account
   authorization and local keys. A fallback code version must understand these
   records; otherwise enter Recovery without enabling its services.
5. Sync the new tree and migration receipt before publishing its selection with
   the journal protocol. Keep the source generation until the target is healthy.
   If publication outcome is uncertain, reread the journal before retrying.

If a trial retry finds the target configuration already durably selected for that
same transaction, reuse it and its latest native account changes. Do not derive
it again from the earlier source generation. Rollback derives a new compatible
generation from this latest selection. Interrupted prepared but unselected trees
are either verified and completed under the matching intent or discarded; they
never override the journal's authoritative current selection.

The next rollback derives configuration from the latest selected generation,
including changes made during a failed trial. It does not mount a historical
configuration unchanged. Unknown configuration migrations and unsupported formats
block the switch. The first implementation supports identity preserving merges
only; arbitrary application configuration conversion requires an explicit adapter.

An interrupted first enrollment keeps pending owner state, its locked account and
its home. Successful enrollment keeps the password, owner, setup marker, sudo
membership and device ID after successful update and fallback. Native PAM and
ordinary administrator authorization remain the authentication mechanism.

## Service state compatibility

Each managed service contract records a unique stable service ID, all persistent
paths, numeric schema, reader schema range, writer schema and compatible executable
version. The installed state registry records the current schema and the completed
migration receipt. A deployment supplies the contracts its service versions accept.

Before starting attached services, compare each registered current schema to the
selected deployment. Unknown state or a missing required contract cannot be treated
as compatible. A service declared unmanaged may remain installed, but its data cannot
be included in automatic rollback guarantees. A switch needing that service must
stop with an actionable compatibility error.

The first implementation admits only schema preserving writers: writer schema
must equal the durable current registry schema, and every allowed code version
must read that schema. Before first service start, persist its exact registration,
path owners and schema receipt, then permit writes. No service may create or
upgrade a format and update the registry afterward. A schema change needs a
separate crash consistent service migration protocol and is disabled initially.
Maintainer scripts never receive live databases. There is no generic downgrade.

Removing a service marks its current record dormant before allowing the switch;
it retains data, schema and numeric ownership. Dormant records need no executable
contract in a deployment that does not attach/start that service. Reattaching it,
including on rollback, requires the retained schema and declared writer/reader
checks before enabling access. Refuse a conflicting registration or unknown
existing data; never call it a fresh database to bypass compatibility checks.

Each active or dormant record independently pins its exact contract descriptor,
registered path/owner map and original authentication evidence under
`State/var/lib/linos/service-contracts/<contract-sha256>/`. Store the canonical
descriptor and signed source metadata before first service start. Retain these
even after every deployment that used the service is collected. Garbage collection
roots include all service records, not only software history. Reattachment checks
the new contract's schema, paths and stable numeric ownership against this retained
descriptor. Removing these pins requires a separate explicit data removal protocol,
which is not enabled in the initial implementation.

If state becomes incompatible despite preflight, ordinary rollback and service
startup stop. Recovery retains both code deployments and the current data. A later
service specific migration or explicit backup restore must name its data set,
backup identity, quiescence check and result schema. A restore is a distinct owner
decision with an explicit description of lost newer writes. Never make personal
file or database rollback a hidden part of selecting an older OS.

## Managed APT transactions

The package resolver remains native APT/dpkg. There is one desired application
catalog, one coherent package database per deployment and no second package
database pretending that libraries under `/usr` are independent applications.

The journal's `selected_deployment_id` and that deployment's signed `applications`
file are the sole active desired catalog. `Applications/catalog/` is a derived
readonly view; unactivated plans live in State and cannot change it. Failed trials
and rollback select the catalog of the software actually selected by the journal,
while retaining failed plan evidence for explicit review/retry. Re-resolution on
an official upgrade always starts from this authority, not a stale mutable cache.

The proposed administrator surface is `linos system packages plan|stage|activate`.
`plan` accepts install/remove/upgrade and validated package identifiers; it resolves
using the active inventory and configured trusted repositories, returning exact
package versions, download/image/EFI space needs and reboot/data compatibility
requirements. `stage` requires the reviewed plan identity and privileged
authorization. `activate` consumes a completed staged transaction and performs
the normal boot trial, not a live package replacement.

APT applications may be managed separately in the catalog while sharing their
system deployment's dependency closure. Installing or removing one composes a new
deployment. It does not relocate arbitrary Debian packages into `/Applications`.
Removing an application retains its personal and service data by default.

An official OS upgrade carries the active desired application catalog forward.
Resolve its complete package closure against the new official base, show any
version/conflict/adapter changes in a new reviewed plan, and refuse unresolved
choices. It must not silently replace a locally composed root with an image that
drops requested packages. Plan identity binds the active deployment, catalog,
official base and expected journal generation; changed inputs require replanning.

The worker creates a private writable staging root from the selected software
baseline. It uses the staging package database and a consistent configuration
copy. Service starts are suppressed, live State is not mounted writable, and
scripts cannot write through to the running deployment. Dependency downloads and
verification precede the isolated offline install phase. Unknown maintainer
script requirements fail rather than borrowing live databases or host mounts.
The records/transactions child fixes the worker isolation interface.

Before publication, refresh the configuration merge from the then current source,
recheck UID/GID reservations and service schema compatibility, verify installed
files/database coherence, and seal the complete deployment and boot selection.
Activation is serialized with all other deployment and configuration switches.

On the running managed system, native APT query, simulation and cache refresh may
remain available. Native mutating APT/dpkg commands must fail before maintainer
scripts can run, with a managed transaction instruction. The read only package
database and system mounts are enforcement, not just a changed Store message.
An administrator deliberately bypassing managed mounts leaves the qualified
contract; the platform must not promise rollback of arbitrary root modifications.

## Encryption boundary

Optional volume encryption covers Linos, including Users, State, configuration
generations, deployment images/subvolumes and local system history. EFI and Recovery
remain independently bootable and do not hold account secrets or plaintext copies
of Linos data. Disk encryption does not establish Secure Boot or authenticate an
editable EFI against a physical attacker.

The encrypted variant must use a standard LUKS2 container below the selected Linos
filesystem, with unlocking in both ordinary early boot and Recovery. A recovery
secret stays with the owner, outside the device. No key is embedded in editable
EFI configuration or an offline installation payload. Lost secrets cannot be
replaced by an OS reinstall that preserves unreadable encrypted data.

The first VM qualification uses `encryption=none`. Until an encrypted candidate
passes unlock, failed unlock, rollback, independent Recovery and key backup tests,
installation must report encryption as unavailable. This defines its boundary
without claiming the encrypted path is implemented or qualified.

## Verification obligations

* Update, then change a password, create a user and change hostname; rollback must
  preserve current credentials, UIDs/GIDs, home ownership, device ID and setup.
* Interrupt enrollment and configuration publication at each durable boundary;
  retry must preserve the authoritative generation and never enable an unclaimed account.
* Introduce a configuration conflict, UID collision and unsupported schema; each
  prevents selection without modifying current data.
* Run package install/remove in staging; the active root, active package database
  and live service data must be unchanged before activation.
* Try direct mutating APT and dpkg on the managed running system; verify refusal
  before a fixture maintainer script can modify a sentinel.
* Use a service fixture with compatible and incompatible reader/writer schemas;
  verify safe trial eligibility and refusal of unsafe rollback.
* Remove an application; retain its data. Deliberate data restore must require a
  separate operation and an identified backup, not a software rollback flag.

These are Stage 3 acceptance obligations for **AC-2**, **AC-3**, **AC-4**, **AC-9**
and **AC-10** in `index.md`, not claims about the current development image.
