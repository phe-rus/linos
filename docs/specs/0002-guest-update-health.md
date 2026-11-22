# Persistent guest update state and health approval

Status: Implemented and verified for development QEMU images only. See the
[guest health checkpoint](../architecture/image-updates.md#guest-health-checkpoint-22-november-2026).

- Data mounts at `/var/lib/linos`; the journal, deployment binding and health
  intent live under `updates/`. Identity initialization requires the configured
  mount, so a missing Data filesystem cannot create replacement local state.
- The signed image contains a protected layout with distinct System A/B, Data
  and EFI partition UUIDs. The guest checks actual mount devices and their common
  parent disk against that layout, not just command-line claims.
- Boot identity is slot, release, transaction and root PARTUUID. Duplicate or
  stale identity fields, wrong disks, bind subdirectories, read-only mounts and
  missing mount points cannot approve an update.
- The only policy implemented here is `development-boot`: successful native
  identity initialization and a running serial login service are required.
  If the Linos kernel exposes readiness, it must already be acknowledged. This
  does not certify enrollment, PAM account migration, production health budgets
  or a newly qualified Linos kernel.
- Before calling native `systemd-bless-boot good`, persist a health intent bound
  to the deployment. Require the EFI boot-count variable to identify the observed
  candidate entry, then verify the resulting stable entry and flush EFI state.
  Only then commit the guest journal. A reboot between those steps can retry.
- A fallback boot can consume failed-trial transitions only after observing the
  exhausted candidate entry. Persist the rollback decision, finish the journal
  state machine and retire the failed entry. Earlier manual selection of the
  old slot leaves the trial pending.
- Host image export accepts an empty destination and a completed trial
  publication. It cannot overwrite guest state. No block-device installer,
  account database sharing, physical update or release publication is enabled.

Verification: Rust tests exercise stale identity, wrong EFI identity, invalid
mount shapes, native blessing failure, interrupted journal commit, repeated
approval and interrupted rollback. Disposable OVMF scenarios withhold approval
for two boots and reconcile fallback, then approve B on a fresh trial and verify
B remains selected across reboots. Data's sentinel and device identity must
survive; journal changes are intentional.
