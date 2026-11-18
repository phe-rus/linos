# Linos 13 platform: Server first, Desktop next

## Shared system foundations

Server and Desktop share the **same Rust platform logic** for device ownership,
kernel boot readiness, recovery/update state machines, installation verification,
local accounts and package trust. They have different user interfaces, hardware
policies and delivery images. Editions are explicit in installed media and never
inferred from CPU type or graphical display availability.

The common edition contract begins at `system/src/platform/edition.rs`. Server
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

## Current limitations

The A/B planner in `system/src/recovery/update.rs` is **pure and read-only**.
It proves permitted state transitions and rejects stale/unverified operations
in unit tests. It does not write partitions, alter the bootloader, persist
transactions or perform rollback. Actual update machinery remains a Server
release blocker and a shared requirement for Desktop.

## Measured Rust engineering baseline

The independent `rust:measure` CI path compiles the four shipped Server
executables using the production Cargo profile. It records elapsed time,
peak compiler resident memory, and per-binary disk sizes. Desktop's existing
X11 graphics adapter is only type-checked, not installed in Server.
