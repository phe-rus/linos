# Linos architecture

Linos is an independent Linux-based operating-system product, **not a Debian spin by design**. Debian 13 repositories currently supply bootstrap packages, but Linos does not intend to inherit Debian's product logic, application integration contracts, kernel policy, user experience or release structure. In the broad technical sense, Linos distributes Linux software; its project identity is a consumer-oriented rethink of how the operating system functions as a coherent experience. The first engineering milestone is a dependable Server edition; desktop and other editions are future platforms, not the current booted image.

## Product philosophy and distribution boundary

- Design end-to-end coherence across installation, system settings, updates, security and application experience. Study Apple's integration philosophy, but do not imitate its UI or rely on closed Apple technologies.
- Keep user control and administrator freedom, with understandable permissions and safe recovery.
- Start from Debian packages and `apt` as implementation inputs, not a permanent identity. Record which base components Linos adopts, changes or eventually replaces.
- Maintain compatibility deliberately: changing packaging or system internals requires migration plans and measurable tests.
- Keep the server core independent of any future desktop shell.

## Product ownership versus upstream dependencies

| Layer | Initial implementation | Linos ownership target |
| --- | --- | --- |
| Software supply | Debian 13 packages and APT | Curated upstream inputs, Linos packaging policies, native apps and Linus store |
| Kernel | Debian-packaged kernel in QEMU | Reproducible Linos kernel builds/configuration profiles, tested for each supported target |
| Boot/service management | Linux initramfs and systemd | Linos lifecycle, defaults, capability management, recovery/upgrade contracts and UX |
| Login/permissions | PAM, standard Linux identities | Linos authentication experience and fine-grained authorization, retaining secure Linux boundaries |
| App experience | Ordinary Linux binaries; early Rust console tools | Linos SDK, native UI framework, integration contracts, permission model and edition-aware app lifecycle |
| Filesystem interaction | Linux filesystem and read-only Assets browser | Assets as a full native file manager with safe operations, previews and platform integration |
| Release engineering | Development packages and QEMU smoke tests | Signed, reproducible releases that enforce compatible kernel and platform contracts |

**Dependence does not imply product inheritance.** Conversely, the currently booted system cannot be called independent of Debian's kernel and service defaults until the relevant Linos-owned implementation and validation exist.

## Kernel and performance contract

The target isn't a collection of fashionable optimization flags. Linos needs a source-controlled, reproducible performance and kernel policy that is consistent across supported hardware and releases.

1. Pin the upstream kernel source/version and document patches, config fragments and firmware/module provenance.
2. Build configuration from a common Linos baseline plus explicit hardware-capability profiles, failing on unsupported config drift during upgrades.
3. Define user-facing budgets for interactive latency, boot, power, memory pressure, storage I/O and responsiveness under background activity.
4. Test compositor/input/display pipelines together with CPU scheduling, graphics drivers and userspace. Display fluidity and animation physics are predominantly whole-platform properties, not kernel-only switches.
5. Preserve kernel security, ABI/driver compatibility, recovery paths and support windows; require benchmarks and regression tests before switching policy.
6. Record per-edition and per-device validation. A server, laptop, television and handheld can share platform principles without sharing an identical configuration.

The **current** build installs a Debian kernel. These kernel profiles and upgrade enforcement are planned, not implemented. The goal of consistent responsiveness is an acceptance target, not a measured current result.

## Native application model and development experience

Rust is the preferred language for Linos-owned systems software and the intended native app platform. The architecture should provide Linos UI primitives, app manifests, permissions, accessibility APIs, application lifecycle, launch/install contracts, and tooling so third-party developers can build applications that actually feel native.

A convenient single-file Rust runner is a target developer workflow: a small source file should be compilable and runnable with one obvious Linos command, hiding temporary build artifacts and offering sensible caching and diagnostics. This requires explicit security and dependency rules. It does not make Rust an interpreted language, and must not execute untrusted source silently.

Applications can have different integration levels: fully native Linos apps, Linux-compatible apps with managed integration, and legacy/isolated apps. The OS cannot guarantee arbitrary third-party apps look or behave identically without cooperation or compatibility technology.

The long-term Linus store acts as a discovery, permissions, distribution, update and review platform across Linos editions. Its current implementation is only a read-only dpkg/APT browser and transaction simulator. Assets is currently a read-only TUI browser, not yet the graphical native file manager.

## Layers and boundaries

1. **Hardware and firmware**: discover CPU architecture, platform model, memory, boot method, block devices, and supported capabilities. Detection does not imply a compatible bootloader or available driver.
2. **Linux kernel**: use a supported kernel and platform-specific configuration, firmware and device trees when needed. The current QEMU image uses Debian's kernel package. The Orange Pi host currently runs an Armbian vendor kernel; this is separate from the guest.
3. **Early boot and services**: initramfs mounts the root filesystem, then systemd manages the userspace and services. The Rust tool called `init` is **not PID 1** and does not replace systemd.
4. **System base**: Debian packages via `apt` plus Linos-owned packages, release metadata, defaults and eventual system policy.
5. **Device services**: future first-boot provisioning, ownership, system updates, recovery and resource policies. These should be independently testable, with short internal names.
6. **User and applications**: ordinary Linux processes, tools and packages. A future desktop shell must not be required by the server image.

## Boot demonstrated

The tested development path is:

```text
Build on ARM64 development host
  -> mmdebstrap Debian root filesystem
  -> install release package and Debian kernel
  -> extract kernel + initramfs
  -> ext4 image
  -> QEMU virt machine
  -> initramfs mounts /dev/vda
  -> systemd reaches multi-user.target
  -> serial login prompt
```

This does **not** establish an installation method for real Orange Pi hardware or an AMD64 machine. QEMU's `virt` board differs from the Rockchip RK3588 and its boot firmware and device tree. Images must never be written to the development host's NVMe boot disk during experimentation.

## Current initialization contract

`crates/init` is a standalone Rust command accepting `--root <directory>`. It canonicalizes the destination, refuses the live root (`/`), then:

- Reads and preserves a nonempty existing `etc/hostname`, or generates a `linos-` hostname.
- Reads and preserves a valid UUID in `var/lib/linos/device-id`, or creates one if absent.
- Writes `var/lib/linos/provisioning-state` as `unclaimed` only after the hostname and device ID exist.
- Checks existing data for invalid values before writing new state; unrecognized provisioning states are rejected.
- Opens the offline root and traverses its directories using `rustix::fs::openat` with `O_NOFOLLOW` and `O_DIRECTORY`; subsequent operations use these opened directory handles.
- Reads existing entries using no-follow file descriptors, rejecting non-regular files and oversized/invalid values.
- Creates missing directories relative to open handles, then publishes fully synchronized temporary files using `linkat` so existing names are never silently replaced.
- Applies restrictive modes to identity/state files and allows an interrupted run to resume.

It has thirteen unit tests, including restart recovery, invalid state, symlink rejection and private file permissions.

The UUID is **not** a cryptographic ownership credential, device key, hardware attestation or Passid identity. Device identity does not prove ownership. The `unclaimed` state is informational, not an access-control boundary. Initialization remains offline-only. Directory-relative operations protect child paths after opening the root, but the initial root selection and any concurrent privileged modifications still need an explicit threat model. The multi-file operation is recoverable rather than a fully atomic transaction. Ownership enrollment, concurrency policy and boot-time integration are still required.

## Planned identity and ownership

Keep these concepts independent:

- **Device identity**: stable unique identifier and eventually local device key material; should survive ordinary OS updates.
- **Hostname**: human-friendly network name; user-editable and not suitable as a security credential.
- **Ownership**: local record of which account is authorized to administer the device.
- **Passid linking**: optional online association for authorized synchronization and device-management workflows.

First-boot provisioning must work offline. Device ownership and administrator setup must require an explicit authenticated enrollment flow, with no universal default password or unauthenticated network setup endpoint. Recovery, factory reset and ownership transfer need separate verified policies. Avoid assuming that a hardware serial number is secret.

Suggested future runtime paths follow Linux compatibility conventions:

| Path | Proposed use |
| --- | --- |
| `/usr/lib/linos/` | Linos-owned installed components |
| `/etc/linos/` | Administrator-editable configuration |
| `/var/lib/linos/` | Persistent per-device state |
| `/run/linos/` | Ephemeral runtime state |

These are design targets, not existing supported APIs.

## Updates and packages

Preserve apt compatibility. An eventual managed base image must define how apt-installed packages interact with rollbacks, writable state and core updates. Do not equate OverlayFS alone with atomic upgrades.

A reliable updater will need authenticated manifests/artifacts, staged activation, integrity verification, recovery on failed boot, safe maintenance scheduling, and an escape hatch for administrator-managed systems. A/B slots are one possible implementation, not a committed requirement. Never reboot a running server automatically without an explicit policy and workload-aware coordination.

## Resource management

Use Linux scheduling and cgroup v2 mechanisms where available, observing PSI, memory pressure and I/O latency before applying policy. Prefer reversible service controls and systemd integration. Blindly applying `SIGSTOP` to unfocused programs can disrupt databases, connections, locks, jobs and updates, so foreground focus alone must not decide service suspension. Server and desktop policies will differ.

## Security baseline

Future security includes secure provisioning, least-privilege services, signed update metadata, disk encryption options, system integrity validation, app isolation and measured recovery behavior. No claim of secure boot, immutable root, verified rollback or production-grade ownership protection is warranted by the present prototype.

## Engineering principles

- One source tree, target-specific builds and capability detection.
- A coherent Linos experience with explicit compatibility contracts.
- No mandatory Passid connection for local administration.
- Clean recovery before risky updates.
- Small focused Rust components rather than one crate for every concept.
- Tests in temporary roots or QEMU before deployment on any real device.

For implementation order and acceptance checks, see [roadmap](roadmap.md).
