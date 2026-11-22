# Linos 13: current engineering status

Updated: 22 November 2026. Channel: development. Historical interface evidence below is from 14 November; kernel CI, Rust checks and managed image updates have progressed since then. The fresh Server qualification is recorded separately below. This is not a production release certification.

## Verified baseline

| Stage | Evidence | Assessment |
| --- | --- | --- |
| Headless boot | QEMU amd64 and arm64 boots, image smoke logs | Demonstrated |
| First boot | Cliclack setup, administrator account, language/keyboard/timezone/network/hostname review | Demonstrated in amd64 screenshots |
| Authentication | Remembered username, system password via PAM, login handoff to Home | Demonstrated |
| Linos Home | Home overview, slash command palette, native Shell switch | Demonstrated |
| Console sizing | Wide-terminal layout, Ctrl+R resize recalibration | Demonstrated in screenshots, needs small-size tests |
| Assets | Keyboard-driven read-only directory browser | Demonstrated |
| Resources | CPU load, memory/swap/storage readings, progress bars | Demonstrated |
| Settings | Device identity, locale and timezone read-only display | Demonstrated |
| Rust quality | 95 application tests, whole-project rustfmt and strict Clippy passed locally on 22 November | Current working tree verified |
| PTY | Persistent shell, cwd updates, command output | Partly demonstrated, false password mode still reported |

## New features not yet verified in a rebuilt image

- `/hardware`: Linux-reported device classification, architecture, CPU, memory, kernel, firmware and battery presence. This is identification, not a compatibility certification.
- `/store`: Linus store package inventory, installed size estimates, repository search, package metadata and simulated install/remove operations. There is no authenticated transaction UI or independent Linos package repository.
- Consistent product casing across Home and these new views.

These source additions must pass rustfmt, Clippy, unit tests, build and live QEMU verification before checking them as complete.

## Product identity and independence gap

Linos aims to be a consumer operating-system product, **not a Debian spin with a custom shell**. Upstream repositories are a pragmatic early source of software. They are not intended to prescribe the final Linos kernel policy, runtime contracts, application behaviour or user experience.

**What exists:** Linux-based QEMU images, Linos-owned kernel configuration and source overlays, packaged AMD64/ARM64 kernels with successful CI boot and installed-system checks, systemd/PAM/APT foundations, Rust-owned setup and console features, a read-only Assets browser, Hardware and Linus store views, and signed development image update/rollback tooling. Older compatibility images use Debian kernels.

**What does not yet exist:** a production-qualified kernel and physical hardware support matrix; measured production latency/rendering/input budgets; a native Linos Rust UI/application SDK; an integrated multi-edition app manifest, sandbox and lifecycle; a fully featured Assets file manager; a developer-facing Linus store; and a complete production update and release architecture. Desktop is still an X11 window experiment, not a bootable Desktop platform.

The difference is important. No currently demonstrated Linos image proves the intended deep operating-system independence. A source tree expressing those intentions is a plan, not validation.

## The user-experience standard

The target is not just that Linux commands or package installation technically work. The system must be understandable without technical instruction, feel consistently responsive, recover from errors safely, maintain deliberate interaction and visual rules, and support advanced tools without requiring ordinary users to become Linux specialists.

Kernel and resource policies must be measured and reproduced per hardware profile, and cooperate with application frameworks, display/input rendering, services and device drivers. The kernel alone does not create smooth animations or native application consistency.

## Critical blockers

1. **Command composer:** a normal command sometimes leaves the UI in `Password (hidden)` mode. This is a UI/PTY state bug, not evidence that ordinary Linux commands require a login password. Verify prompt handling in a rebuilt guest and stop claiming the bug is fixed until observed.
2. **Terminal semantics:** the embedded PTY shows a text preview rather than a real terminal emulator. Full-screen programs and interactive sessions should run in native Shell mode.
3. **Privilege boundary:** first-boot administrator enrollment does not mean every command should run as root. `sudo` authentication is a separate privilege escalation request. Linos must never silently grant root or store administrator passwords.
4. **Package operations:** safe previews exist in source; actual install, upgrade and removal need confirmation, authorization, progress/error handling and transaction recovery.
5. **Kernel qualification:** source-controlled profiles, provenance and AMD64/ARM64 QEMU checks exist. Physical devices, workload latency, power behavior and production support remain unqualified.
6. **Updates and recovery:** signed development image staging, health approval and fallback exist. Production account/home/service-data migration, release keys, physical installation, Secure Boot and power-loss qualification remain open. Recovery's installation and repair menu entries remain disabled.
7. **Hardware:** no validated physical compatibility matrix covering laptop, x86 server, Raspberry Pi or RK3588/Orange Pi devices.

## Milestones and release status

- **Completed:** Rust workspace, bootable guest roots, boot identity, offline enrollment, independent Recovery proof-of-concept, responsive Home management views, initial resource and Assets introspection.
- **Active:** native command usability, hardware reporting, Linus store, Settings controls, installer/recovery integration, automated hardware compatibility checks.
- **Planned:** production kernel policy qualification, performance tuning based on measured workload, cgroup v2/PSI-based resource control, production signed distribution updates and rollback, physical device support and independent store backend.
- **Developer alpha:** not yet validated. The current guest experience is a promising developer preview. There is no trustworthy numerical completion percentage for a production OS because the remaining security and release gates are unequal in effort.
- **Production:** not ready.

## Product philosophy check

Linos aims for a coherent, offline-first, user-controlled experience, ordinary Linux compatibility and a capable server-first architecture. The Cliclack setup plus native Home and Shell demonstrate the integrated experience. Using Debian packages as an openly documented bootstrap and retaining PAM/systemd is consistent with pragmatic Linux compatibility. The design is weakest where its polish exceeds its guarantees: terminal authentication, update/rollback semantics, physical device support and reproducible kernel policy.

For comparison, Debian Server has mature package security/update and hardware support but lacks Linos' own guided Home experience; Ubuntu Server offers established installer, cloud/server packaging and a large support ecosystem; Fedora Server offers a contemporary distribution stack with established release testing. Linos should not claim superior reliability, hardening or hardware coverage over those projects yet. Its distinguishing idea is a unified human-oriented terminal-first administration experience, not an independently mature kernel or package ecosystem.

## Required next sequence

1. Reproduce and correct the false PTY password state without compromising genuine password-entry protection.
2. Run rustfmt, Clippy and unit tests for all new source changes.
3. Build an immutable image with a fresh build ID and verify initial setup, login and command behaviours in QEMU.
4. Exercise Hardware and Linus store on the built image, including offline repository failure messages and installed-size accuracy.
5. Add safe Settings mutations and an explicitly authenticated transaction workflow for package changes.
6. Preserve owned kernel configuration and provenance checks while qualifying each additional hardware profile.
7. Complete installed update and recovery qualification before widening real-hardware installation. Keep release publication behind CI qualification.

## Evidence

Screenshots taken during 14 November 2026 development show first-run setup, sign-in, Home, Settings, Assets and Resources. Do not represent these images as proof of newer source features until the corresponding image has been rebuilt and tested.

See [roadmap](roadmap.md), [architecture overview](overview.md), and [server console](server-console.md).

## Current qualification checkpoint

The [development image update record](image-updates.md) describes the retained
six-boot compatibility-kernel checkpoint and the new enrollment-based qualification
path for native Linos kernels. An independent [code review](../reviews/2026-11-22-main.md)
found two health-intent interruption bugs; both were fixed and covered by regression
tests. Graphics now imports the shared edition library, eliminating duplicated
module compilation and the associated strict Clippy failures.

Fresh AMD64 Server build `20261122.2` uses the checksum-verified kernel package from
[qualification run 44213747067](https://github.com/phe-rus/linos/actions/runs/44213747067).
Its recorded source/configuration provenance matches this checkout. Local build and
runtime qualification passed with 768 MiB guest RAM, one vCPU and KVM: direct boot,
identity persistence across two boots, interrupted/resumed enrollment, PAM login,
kernel readiness, SSH enrollment restrictions, UEFI boot and all six managed update
boots. Native blessing committed B, two B failures fell back to A, and persistent
identity, hostname and the Data sentinel survived. Both update slots use copies of
the same enrolled fixture; account migration is not implemented.

Smoke completed in 6.7 seconds and UEFI boot in 10.3 seconds. Guest memory after
login was 733,380 KiB total and 622,256 KiB available (108.5 MiB used by that
calculation), with no guest swap. A bounded 128 MiB tmpfs allocation plus a normal
command passed. Sampled QEMU host RSS peaked at 407,020 KiB (397.5 MiB) during
enrollment and pressure testing. These are development observations, not minimum
hardware requirements or production latency budgets. Logs and measurements are
under `target/qualification/` and `target/update-test-logs/`.

The live test found and fixed readiness reading setup completion from the wrong
directory: setup writes `/etc/linos/setup-complete`, and readiness now reads that
public marker through the existing no-follow helpers. Corrected unit fixtures
failed before the fix and passed after it. An obsolete native shell prompt in
the enrollment test was corrected as well.

Recovery can now consume the same qualified kernel package. That builder path
and its CI wiring passed source review and shell syntax checks; a fresh local
Recovery build and firmware boot remain unverified. Desktop boot, production
account/data migration, physical installation and hardware qualification remain
open. GitHub had no published Releases when checked on 22 November; this local
checkpoint does not publish a release. The next step is fresh native Recovery
qualification, followed by the complete CI release gate.
