# Linos kernel product architecture and release standard

This document is the kernel implementation contract for Linos 13 and future
Linos editions. It reflects the repo's product philosophy, not a claim that
all of it has already shipped.

## The right abstraction

Linos is a Linux-compatible OS family. The kernel provides reliable, narrow
mechanisms; Rust-native Linos system services implement human-friendly
policies. Do not create new syscalls, kernel threads or custom drivers solely
to make the project sound independent. Upstream-compatible behavior is
valuable. The test is whether a change solves a real user or device problem.

### Kernel responsibilities

- Scheduling/preemption choices and interrupt latency, proven by measurements
- Memory pressure, OOM behavior and cgroup v2 limits exposed to Rust services
- Filesystems, I/O schedulers, device-mapper, data and boot integrity primitives
- Kernel-side access control, namespaces, seccomp, Landlock, hardened memory
- GPU DRM/KMS, input, audio, networking, USB, firmware and device power support
- Architecture-specific boot, recovery console and persistent diagnostics
- Small, stable and documented Linos-specific contracts where Linux lacks one

### Linos 13 userspace responsibilities

- Native graphical compositor, shell, UI responsiveness and animation timing
- App permissions, sandbox orchestration and user explanations
- Installation, staged updates, rollback and recovery workflows
- Hardware discovery, profile selection and service prioritization
- Local identity, optional Passid account integration and offline ownership
- Rust developer APIs and Linus store; Linux-compatibility tooling

Kernel flags cannot make these user-facing experiences correct on their own.

## Supported profile versus desired profile

| Profile | Now | Gate before claiming support |
| --- | --- | --- |
| QEMU AMD64 | Compilation/boot qualification in progress | Tested kernel and Linos OS boot, shell, storage, recovery |
| QEMU ARM64 virt | Compilation/boot qualification in progress | Same gates plus serial and device-tree validation |
| Generic x86-64 PC | Not qualified | UEFI/Secure Boot policy, GPU, storage, Wi-Fi, audio, suspend/resume, firmware and real hardware |
| RK3588 / Orange Pi 5 Plus | Not qualified | Device-tree and vendor driver compatibility, GPU/VPU/network, boot firmware, thermal and power |
| Smartphone/tablet | Not supported | SoC BSP, modem, display/touch, sensors, camera, radio, battery, security and regulatory requirements |

It would be misleading to label a profile 'universal' because its CPU architecture
matches. Hardware qualification must always identify actual device families.

## What we take from Apple, without copying Apple

- Integrate kernel and device services under a single tested product contract.
- Ship fewer supported configurations that actually work, not unlimited options
  whose recovery, input and power behavior are left to the user to assemble.
- Measure perceived performance, first-input responsiveness, frame pacing,
  background I/O contention and wake latency on supported hardware.
- Provide understandable failure states and safe recovery without obscuring
  administrator access or Linux compatibility.
- Keep software signing, source licensing, hardware ownership and offline use
  transparent. Apple-style integration does not require proprietary lock-in.

## Where conventional Linux can frustrate consumers

Diverse distributions, GPU stacks, packaging systems, firmware and init choices
can make behavior inconsistent across devices. Linux's flexibility is a strength;
Linos must resolve product choices for a specific supported release instead of
asking the consumer to integrate them. Upstream kernel components are not
automatically deficient. Changes require comparative measurements and maintenance.

## Hard release gates

1. Confirm a trusted upstream signed tag and the exact maintained source overlay.
2. Resolve Kconfig on each target, reject unsatisfied mandatory and forbidden
   settings, and verify the distributed kernel configuration.
3. Compile image and required modules reproducibly from fixed build inputs.
   Record hashes, provenance, dependency versions, module inventory and sizes.
4. Boot the **packaged** kernel in disposable QEMU guests and assert actual Linos
   boot-state and pressure-reporting behavior, not a version string.
5. Boot a normal Linos 13 guest from the exact kernel package with console,
   first-boot, login, SSH lockout and recovery services working together.
6. Test failure conditions: crash evidence, interrupted updates, inaccessible
   graphics, low-memory and I/O pressure, invalid authorization and safe rollback.
7. Measure boot, input latency, frame timing, memory, CPU and storage under
   repeatable workloads, with compared baselines and acceptance thresholds.
8. Review actual AMD64/ARM64 physical machines, firmware and driver support
   before promising PC/board compatibility.
9. Publish complete corresponding source instructions, configured source
   patches, packages, SHA-256 provenance and a support statement.
10. Only then publish a versioned release. Never claim the release is fully
    consumer-ready while critical physical-hardware tests remain unperformed.

## What is currently implemented

Signed upstream source acquisition, Kconfig fragments, a Linos boot-state
kernel source overlay, Linux security/resource feature requirements,
architecture-specific QEMU build jobs, optional installed-OS integration
gates and package provenance. This is **not yet** a fully validated shipping
kernel. At present we must not claim finished recovery, signed boot, full
hardware portability, mobile support, Rust kernel drivers, or Apple-level
responsiveness. Those need implementations and tests.

### CI cost and latency standard

The kernel release entry point remains the only tag-producing pipeline.
Static policy and source overlay tests run **before** upstream source downloads.
The two supported architecture builds use different native GitHub runners
and may run concurrently within that single release workflow. Each architecture
compiles only once, using a bounded 1.5 GB ccache restored across runs;
QEMU and image verification consume packaged artifacts and do not recompile.
The legacy unreferenced kernel research workflow has been superseded.

### Provenance and build budgets

Every kernel candidate now records byte size of the kernel image and module
installation, module-file counts, signing tag, upstream Git commit, binary
and configuration SHA-256 hashes, and SHA-256 hashes of Linos source overlays,
source-application logic and the build script. The first successful artifacts
set the *measured* size and build-time baseline. Do not fabricate a universal
size or latency target before observing architecture-specific results.
The arm64 source-fetch order is tuned for GitHub-hosted ARM runners, while
the upstream signed-tag and exact-commit verification remains mandatory.

A Linux source overlay changes the detached source Git worktree. Linux's
`scripts/setlocalversion` can append an extra `+` when `LOCALVERSION`
is unset even with automatic Git suffixes disabled. The build now explicitly
sets `LOCALVERSION` empty while `CONFIG_LOCALVERSION=-linos` supplies
our deterministic suffix. A mismatched release string fails with its
actual value printed.

### Language contract

Linos-owned build-support code is Rust. The external Linux kernel contains
upstream C and assembly and requires standard Linux build scripts. Linos does
not rewrite those upstream subsystems merely to declare a single-language
product. Kernel-level Linos Rust code requires a supported `CONFIG_RUST`
toolchain and a deliberate design/review and QEMU test for the specific API;
none is falsely claimed by this tooling migration.

### Installed-system persistence release gate

Kernel integration now tests both the initial Linos 13 Server sign-in and
**two successive writable QEMU boots** using a disposable copy of the same
kernel-backed OS image. The existing Rust/Linos test harness exercises
filesystem recovery after the guest is stopped and requires the persistent
Linos device ID to remain unchanged. An image that boots once but loses its
identity or cannot recover its ext4 filesystem fails integration.
This is a targeted recovery test, not yet full update rollback or physical
power-loss certification.

### Kernel artifact source consistency

The Rust kernel tool validates reused build artifacts against the exact
Linos overlay C source, overlay Kconfig, kernel build script, upstream LTS
release, product version, architecture and Linos release suffix **before**
assembling an OS image. It also checks the downloaded provenance record is
byte-identical to the record embedded in the kernel Debian package. A stale,
altered or wrong-architecture kernel now fails integration without booting.
The source-application tool's own hash is recorded in provenance, but the
integration-only job deliberately permits newer *verification tooling* when
reusing previously compiled kernel packages. Full releases compile new
packages on the same commit. This is supply-chain regression protection,
not full binary reproducibility or a signature on the OS image.
