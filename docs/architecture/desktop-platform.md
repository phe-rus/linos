# Linos 13 platform: Server first, Desktop next

## Shared system foundations

Server and Desktop share the **same Rust platform logic** for device ownership,
kernel boot readiness, recovery/update state machines, installation verification,
local accounts and package trust. They have different user interfaces, hardware
policies and delivery images. Editions are explicit in installed media and never
inferred from CPU type or graphical display availability.

The shared library at `system/src/lib.rs` exports the edition contract from
`system/src/platform/edition.rs` and the recovery update model and journal. Server
requires an accessible CLI from boot to recovery. Desktop requires a graphical
experience, but **must retain the headless recovery console** when DRM, GPU or
input drivers fail. The existing `linos-graphics` X11 adapter is only a
development experiment, not the Desktop compositor.

## Server release-critical work

1. Preserve the successfully qualified Linos AMD64/ARM64 kernel, boot and
   identity-persistence checks.
2. Keep the real Cliclack enrollment and PAM-backed login functional after
   successful owner provisioning, including reboot and SSH restrictions.
3. Complete fail-closed verified A/B updates: separate inactive system slot,
   persisted transaction journal, verified boot choice, health acknowledgement,
   fallback to last-good system and explicit data-volume preservation.
4. Run recovery media and installer against disposable UEFI QEMU disks, then
   qualify physical hardware. No blind disk mutation in a production host.
5. Measure boot, memory, latency and package size. Reject regressions against
   documented hardware-specific baseline rather than subjective impressions.

## Desktop execution order

1. Define graphical boot/recovery failure handling over the same edition and
   ownership contracts; retain serial/TTY fallback.
2. Establish native Linux DRM/KMS + libinput + rendering architecture and a
   Wayland-compatible compositor without copying the X11 development adapter
   into the production runtime.
3. Build graphical setup, PAM-backed lock/login and session management using
   shared Rust state transitions; do not create a second identity database.
4. Add Desktop shell, accessibility, display scaling, settings and Assets.
5. Test image construction, virtual GPU boot, lost-display recovery, suspend,
   resume, power and input latency on each claimed hardware profile.

## Engineering and performance policy

- Rust 2024, Cargo.lock and targeted binaries. Avoid whole-workspace release
  builds for a package that ships only four executables.
- `.cargo/config.toml` uses two concurrent rustc jobs by default for memory
  constrained workstations; `CARGO_BUILD_JOBS` can override the setting.
- Development and test profiles omit large DWARF debug data; release enables
  optimisation level 2 without expensive LTO and strips debug symbols.
- Measure before increasing `codegen-units`, enabling LTO or adding GUI
  frameworks. No blanket CPU architecture flags on mixed hardware.
- Preserve meaningful Rust error handling and native panic diagnostics where
  needed. `panic=abort` applies only to installed release executables.
- CI separates fast Rust checks from expensive VM image and kernel builds.
- Desktop functionality is not release qualified just because a Rust binary
  compiles or opens a development X11 window.
- The graphics experiment imports the shared edition library directly, rather
  than compiling a second private copy of the edition module.
- Local image and VM tests support disk-backed `TMPDIR` and explicit VM memory,
  CPU and AMD64 acceleration overrides. See [local qualification](image-updates.md#local-qualification-with-bounded-memory).

## Current limitations

The A/B transition model in `system/src/recovery/update.rs` is pure. Its separate
Rust storage module, `system/src/recovery/update/journal.rs`, persists model
transitions with generation checks, directory locking and atomic replacement.
The recovery console only inspects this state. The development-only
`linos-update-image` caller now stages signed inactive image files and prepares
counted systemd-boot entries for ordinary image files. The managed development guest now mounts persistent
state, checks its actual boot identity and reconciles native boot blessing and
rollback with its Data journal. See [image updates](image-updates.md) for its format,
commands and qualification boundary. Installed update machinery remains a Server
release blocker and a shared requirement for Desktop.

## Measured Rust engineering baseline

The independent `rust:measure` CI path compiles the four shipped Server
executables using the production Cargo profile. It records elapsed time,
peak compiler resident memory, and per-binary disk sizes. Desktop's existing
X11 graphics adapter is only type-checked, not installed in Server.

### Initial measured Rust baseline

GitHub Actions AMD64, November 18 2026, two Cargo jobs, release profile:
24.13 seconds for the four shipped Server executables; maximum resident
set size reported by GNU time was 438,720 KiB for the largest measured
process. This is **not** aggregate RAM across concurrently running compiler
processes. Binary sizes: init 496,408 bytes, enroll 923,128 bytes,
linos-login 782,712 bytes, linos 1,073,576 bytes, totaling 3,275,824 bytes.
These are measurements for that runner and commit, not guaranteed limits on
ChromeOS, ARM or other hardware. See the `rust:measure` CI artifacts.

Recovery supports strictly validated, read-only inspection of an optional
`/var/lib/linos/update-state` journal, rejecting symlinks, duplicate fields,
invalid slots and invalid boot-attempt counts. Authentication, installation and
rollback remain part of the required update engine.

## Persistent update journal, 22 November 2026

The shared Rust `Journal` API opens an existing directory owned by the caller,
refuses writable untrusted directories and symlink traversal, and holds an
exclusive advisory directory lock for each writer session. It does not create
system directories or infer which slot is active. An installer must supply that
verified initial slot. A production caller must run within the privileged update
service; this library is not an authorization or payload verification boundary.

Each accepted event compares the expected phase and generation, checks the pure
transition rules and advances the generation. Version 1 records remain readable
at generation zero; the next accepted event writes version 2. The generation
rejects stale callers, including a state that returned to its previous phase.
It does not provide signed release ordering or protection against privileged
journal tampering.

Writes use a private pending file in the same directory, flush the file, rename
it over the journal, then flush the directory. An interrupted pending file is
never authoritative and is discarded on the next valid write. Corrupt committed
records block both updates and initialization. An error after rename has an
uncertain commit outcome: callers must inspect the journal before retrying.
All journal readers and writers must follow the locking protocol.

Validation: `cargo test --locked --lib --bin recovery` covers persisted success
and rollback sequences, stale generations, competing writers, invalid records,
unsafe paths and a child process exiting during a pending write. These are
filesystem and process tests on the development host, not power-loss or QEMU
boot qualification. The separate image backend connects signed staging and boot
selection to this journal. Managed development guests now reconcile health and
rollback in persistent Data. Host workspace reconciliation and production account
migration remain open.
