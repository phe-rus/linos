# Linos architecture

Linos is its own Linux-based operating system, designed as a coherent system rather than a desktop environment or a renamed Debian installation. Debian 13 repositories and packages are an initial bootstrap dependency. Long term, Linos owns its system architecture, integration, identity, policies, services, release model and distribution choices. Linux application compatibility is an intentional boundary, not a commitment to adopt Debian's product design. The first engineering milestone is a stable, headless server image; graphical software is out of scope for the present phase.

## Product philosophy and distribution boundary

- Design end-to-end coherence across installation, system settings, updates, security and application experience. Study Apple's integration philosophy, but do not imitate its UI or rely on closed Apple technologies.
- Keep user control and administrator freedom, with understandable permissions and safe recovery.
- Start from Debian packages and `apt` as implementation inputs, not a permanent identity. Record which base components Linos adopts, changes or eventually replaces.
- Maintain compatibility deliberately: changing packaging or system internals requires migration plans and measurable tests.
- Keep the server core independent of any future desktop shell.

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
