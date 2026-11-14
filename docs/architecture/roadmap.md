# Development roadmap

This document tracks intended milestones, not promises or shipped features. The repository's `main` branch is the working branch. Current development channel: **development**, product **Linos 13**, generation **1.0.0**. Build IDs are assigned per build and must not be treated as the source revision or an alpha tag.

## Completed proof of concept

- [x] Initialize Rust workspace and source layout.
- [x] Discover model, architecture, firmware, CPU count, memory and kernel on Orange Pi 5 Plus.
- [x] Build minimal Debian 13 ARM64 root filesystem using `mmdebstrap`.
- [x] Package and include Linos release metadata.
- [x] Boot Debian kernel 6.12 guest on QEMU ARM64 `virt` to `multi-user.target`.
- [x] Display initial Linos serial console banner.
- [x] Test an offline Rust hostname initializer (four unit tests).
- [x] Keep generated disk images, tarballs and Rust targets out of Git.
- [x] Publish sources on `main`.

These are **prototype achievements**, not proof of general hardware compatibility or complete system integration.

## Milestone 1: credible first boot

- [x] Define persistent device identity separate from hostname.
- [ ] Complete the uninitialized, unclaimed, provisioned and recovery state model; interrupted offline initialization is tested.
- [x] Harden offline `init` against common symlink/path traversal mistakes and recover from interrupted writes.
- [x] Verify local console administrator enrollment, interruption recovery and no-default-password policy in persistent QEMU tests.
- [x] Verify the packaged root-privileged initializer and enabled systemd first-boot service in QEMU.
- [ ] Integrate canonical OS identification without conflicting with Debian `base-files` ownership.
- [x] Confirm root filesystem checks, enrollment console behavior, ownership persistence and service readiness in QEMU.

**Acceptance:** a fresh VM can initialize, enroll its first local administrator, reboot and preserve its identity without requiring network connectivity.

## Milestone 2: reproducible builds and supported targets

- [ ] Encode dependency versions and build inputs in a reproducible recipe.
- [x] Add an AMD64/ARM64 GitHub Actions matrix for Rust checks, package builds, disk images and QEMU smoke testing.
- [x] Confirm green CI boots on both architectures and investigate any ARM64-specific failures.
- [x] Build and boot an AMD64 QEMU image on the ChromeOS Debian 13 development host; retain architecture autodetection.
- [x] Distinguish system package architecture from SoC-specific firmware and device trees in profile metadata.
- [x] Add initial hardware support manifests, including an unverified RK3588 profile (not hardware certification).
- [ ] Create an installer that discovers storage and asks for confirmation before formatting.

**Acceptance:** independent ARM64 and AMD64 test VMs boot and pass the same core test suite.

**Current assessment:** the VM acceptance target has been met. Reproducible dependency
pinning, physical-device validation, and an enabled safe installer are outstanding.

## Installer and Recovery integration (active)

- [x] Independent Server Recovery CLI boots under UEFI in QEMU.
- [x] Read-only storage inspection and fail-closed installation planning.
- [x] Verify the proposed GPT geometry on fresh disposable images.
- [x] Package an offline Server payload and simulate its installation on disposable storage.
- [x] Add SHA-256 integrity verification and Ed25519 signature verification code.
- [x] Confirm the signed Recovery media path in a completed CI run ([CI run](https://github.com/phe-rus/linos/actions/runs/42594094015)).
- [ ] Implement a transactional, explicitly confirmed physical-disk installer.
- [ ] Add Recovery repair, rollback, and data-preserving reinstall workflows.

All installation experiments remain confined to disposable virtual disks. Signed
payload checks alone do not authenticate the full boot chain; physical-disk install
is deliberately disabled.

## Server Home and management experience (active)

- [x] Cliclack first-boot wizard, local administrator enrollment, remembered username and PAM-backed sign-in demonstrated in QEMU.
- [x] Native Ratatui Home with slash navigation, status, network/services/logs, Assets, Settings and Resources views.
- [x] Persistent PTY-backed ordinary Linux commands; the working directory is displayed above the composer.
- [x] A separate full interactive Shell mode with return-to-Home behaviour.
- [x] Terminal-size recalibration and adaptive layout observed in supplied amd64 QEMU screenshots.
- [x] Read-only Assets keyboard navigation and memory/swap/disk usage bars demonstrated visually.
- [x] Introduced read-only Hardware reporting and Linus store package inventory, search and simulated transactions **in source**, awaiting build and QEMU validation.
- [ ] Verify embedded shell prompt/secret input handling with actual sudo interactions; **a false Password (hidden) prompt was observed**.
- [ ] Implement proper ANSI terminal rendering or a robust command-output model for full interactive applications.
- [ ] Make Assets file previews and management operations safe and usable.
- [ ] Add accessible navigation, additional small-terminal tests and reliable real-console resize tests.
- [ ] Implement controlled Settings changes with explicit authorization and rollback/error handling.
- [ ] Build a transaction-based Linus store UI for install, upgrade, remove, progress and errors.

**Acceptance:** a real user can administer a fresh VM through Home without command lockups or false prompts, inspect hardware and packages, and return to an ordinary shell at any time.

## Milestone 3: security and ownership

- [ ] Implement local ownership, credentials and device keys with a threat model.
- [ ] Define authorized ownership transfer, recovery and factory reset.
- [ ] Integrate local policy enforcement, service isolation and account permissions.
- [ ] Evaluate disk encryption and trusted/measured boot capabilities per hardware.
- [ ] Design optional Passid linking without making an online service necessary.

**Acceptance:** device claims and transfers cannot be performed by an unauthenticated process; loss of network access does not lock out the legitimate local owner.

## Milestone 4: dependable updates

- [ ] Document apt interoperability, overrides and core upgrade boundaries.
- [ ] Produce verified, signed update artifacts.
- [ ] Stage system changes away from the currently running root where feasible.
- [ ] Establish rollback and boot health checks.
- [ ] Define server-specific maintenance windows, deferrals and service coordination.
- [ ] Measure update and recovery behavior under interrupted power or downloads.

**Acceptance:** failed updates recover to a bootable configuration and do not silently discard user data.

## Kernel configuration and portability (not complete)

- [x] Boot using the supported Debian kernel in amd64 and arm64 QEMU images.
- [ ] Define reproducible Linos kernel config fragments with per-hardware capability profiles.
- [ ] Record kernel version, configuration, patches, module/firmware inventory and provenance in build artifacts.
- [ ] Enforce config checks during kernel upgrades, including a kernel version compatibility gate.
- [ ] Benchmark boot, idle memory, storage, scheduling, networking and security posture.
- [ ] Validate Pi/RK3588 and physical x86 targets using actual firmware, drivers and power-management behaviour.

**Current kernel status:** the image script installs Debian's kernel package, not an independently compiled or consistently retuned Linos kernel. Do not claim automatic preservation of Linos-specific tuning across upstream updates.

## Milestone 5: balanced system resources

- [ ] Establish baseline boot time, memory use, disk I/O and command latency.
- [ ] Observe cgroup v2 and Linux PSI on supported kernels.
- [x] Display procfs CPU load and memory usage plus root filesystem utilization in the Server UI.
- [x] Add initial read-only hardware awareness from Linux sysfs/DMI and device tree (source, not yet boot validated).
- [ ] Define server policies for latency-sensitive and background services.
- [ ] Add safeguards against disrupting active jobs, network sessions or databases.
- [ ] Evaluate policies against measured workloads, not aesthetic assumptions.

**Acceptance:** demonstrate reproducible improvements without data loss or starvation.

## Consumer platform independence (planned, not validated)

The differentiation target is not a reskinned Debian environment. Stable upstream repositories are an initial **software source** while Linos defines its own kernel policies, lifecycle, application integration, accessibility, design language, update/recovery guarantees and developer experience.

- [ ] Design an app manifest, installation policy, permission prompts and application lifecycle contract independent of Debian UI conventions.
- [ ] Specify native Rust UI primitives, input/animation timing, accessibility and platform SDK boundaries with an implementable sample app.
- [ ] Prototype a single-file Rust run command with compilation cache, diagnostics, dependency policy and explicit trust boundary.
- [ ] Expand Assets from read-only navigation into a safe, searchable file manager with previews and confirmed mutation operations.
- [ ] Expand Linus store from dpkg/APT previews into managed transactions, recovery, versioning and developer publishing.
- [ ] Define transparent integration levels for native Linos apps, compatible Linux apps and isolated/legacy apps.
- [ ] Establish measurable user-experience criteria: latency, consistency, comprehension, task completion, accessibility and error recovery.
- [ ] Establish shared product contracts across Server, Desktop, mobile, television and other supported targets while preserving edition-specific behaviour.

**Milestone acceptance:** demonstrate a nontrivial native Linos application and a compatible upstream application both following the same documented lifecycle and permissions framework, with user testing and reproducible performance results. Packaging familiarity alone does not count as completion.

## Release gates

| Release stage | Current evidence | Gate |
| --- | --- | --- |
| Proof of concept | Completed QEMU boot, enrollment, recovered guest identity and native TUI | Met |
| Developer preview | Functional UI and 54 Rust tests passed at `4db71a0`; subsequent changes need rerun | Active |
| Alpha | Stable authenticated Home without false password prompts, package/hardware UI validated on rebuilt VM, smoke/CI green, known limitations documented | Not yet met |
| Beta | Physical hardware matrix, guarded installer, tested updates/rollback, threat-model review and performance regression targets | Not met |
| Production | Signed reproducible releases, lifecycle support, full security/upgrade/restore/recovery verification, workload soak tests | Not met |

A green unit test suite is not proof of a successful QEMU boot. Screenshots represent observed builds only; source commits made afterward are unverified until rebuilt and tested.

## Product independence

Linos is **not intended as a Debian spin**. Debian's stable repositories are an initial software supply, not a commitment to Debian's product experience, package presentation, kernel configuration, runtime contracts or OS design. The immediate technical reality is still that development images boot Debian's kernel and use Debian-managed packages. Owning and validating replacements or integration layers is an explicit roadmap requirement. Preserve deliberate Linux compatibility and avoid a breaking rewrite of everything at once.

## Later: user experience

Desktop compositor, window management, Sea Glass visuals and touch/gesture integration follow after the common OS core is stable. Server functionality must remain usable without a GUI.

## Development checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash -n build/image.sh
bash -n build/rootfs.sh
git status --short
```

Run QEMU boot tests before calling a build bootable. Keep generated images ignored by Git and never test partitioning against the development host's boot disk.
