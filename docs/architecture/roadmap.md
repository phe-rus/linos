# Development roadmap

This document tracks intended milestones, not promises or shipped features. The repository's `main` branch is the working branch. Current development release: **Marauder, Sequence 9**, generation **1.0.0**, build **20261105.2**.

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
- [ ] Verify local console administrator enrollment, interruption recovery and no-default-password policy in persistent QEMU tests.
- [ ] Verify the packaged root-privileged initializer and enabled systemd first-boot service in QEMU.
- [ ] Integrate canonical OS identification without conflicting with Debian `base-files` ownership.
- [ ] Confirm root filesystem checks, enrollment console behavior, ownership persistence and service readiness in QEMU.

**Acceptance:** a fresh VM can initialize, enroll its first local administrator, reboot and preserve its identity without requiring network connectivity.

## Milestone 2: reproducible builds and supported targets

- [ ] Encode dependency versions and build inputs in a reproducible recipe.
- [ ] Run and verify the new local QEMU boot smoke script on ARM64 and AMD64, then add CI automation.
- [x] Build and boot an AMD64 QEMU image on the ChromeOS Debian 13 development host; retain architecture autodetection.
- [ ] Distinguish system package architecture from SoC-specific firmware and device trees.
- [ ] Add explicit hardware support manifests for real boards, including RK3588.
- [ ] Create an installer that discovers storage and asks for confirmation before formatting.

**Acceptance:** independent ARM64 and AMD64 test VMs boot and pass the same core test suite.

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

## Milestone 5: balanced system resources

- [ ] Establish baseline boot time, memory use, disk I/O and command latency.
- [ ] Observe cgroup v2 and Linux PSI on supported kernels.
- [ ] Define server policies for latency-sensitive and background services.
- [ ] Add safeguards against disrupting active jobs, network sessions or databases.
- [ ] Evaluate policies against measured workloads, not aesthetic assumptions.

**Acceptance:** demonstrate reproducible improvements without data loss or starvation.

## Product independence

Debian repositories, `apt` and the Debian kernel are first-stage build inputs, not the identity of Linos. Incrementally define Linos-owned package policies, core services, system behavior and release infrastructure. Preserve deliberate Linux compatibility and avoid a breaking "rewrite everything" transition.

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
