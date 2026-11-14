# Linos 13: current engineering status

Updated: 14 November 2026. Channel: development. This status reflects directly observed screenshots and user-supplied test logs, plus source updates that have not yet been rebuilt. It is not a production release certification.

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
| Rust quality | 54 tests, rustfmt and clippy passed on commit `4db71a0` | Verified by local user log |
| PTY | Persistent shell, cwd updates, command output | Partly demonstrated, false password mode still reported |

## New features not yet verified in a rebuilt image

- `/hardware`: Linux-reported device classification, architecture, CPU, memory, kernel, firmware and battery presence. This is identification, not a compatibility certification.
- `/store`: Linus store package inventory, installed size estimates, repository search, package metadata and simulated install/remove operations. There is no authenticated transaction UI or independent Linos package repository.
- Consistent product casing across Home and these new views.

These source additions must pass rustfmt, Clippy, unit tests, build and live QEMU verification before checking them as complete.

## Critical blockers

1. **Command composer:** a normal command sometimes leaves the UI in `Password (hidden)` mode. This is a UI/PTY state bug, not evidence that ordinary Linux commands require a login password. Verify prompt handling in a rebuilt guest and stop claiming the bug is fixed until observed.
2. **Terminal semantics:** the embedded PTY shows a text preview rather than a real terminal emulator. Full-screen programs and interactive sessions should run in native Shell mode.
3. **Privilege boundary:** first-boot administrator enrollment does not mean every command should run as root. `sudo` authentication is a separate privilege escalation request. Linos must never silently grant root or store administrator passwords.
4. **Package operations:** safe previews exist in source; actual install, upgrade and removal need confirmation, authorization, progress/error handling and transaction recovery.
5. **Kernel consistency:** current QEMU images use Debian kernel packages; there are no version-controlled Linos kernel configuration fragments automatically applied and verified on kernel upgrades.
6. **Updates and recovery:** no complete signed, tested, rollback-capable upgrade process or production-safe physical installation.
7. **Hardware:** no validated physical compatibility matrix covering laptop, x86 server, Raspberry Pi or RK3588/Orange Pi devices.

## Milestones and release status

- **Completed:** Rust workspace, bootable guest roots, boot identity, offline enrollment, independent Recovery proof-of-concept, responsive Home management views, initial resource and Assets introspection.
- **Active:** native command usability, hardware reporting, Linus store, Settings controls, installer/recovery integration, automated hardware compatibility checks.
- **Planned:** Linos kernel policy, performance tuning based on measured workload, cgroup v2/PSI-based resource control, signed distribution updates, rollback, physical device support and independent store backend.
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
6. Introduce kernel configuration ownership, cross-hardware capability profiles and tests that validate preservation across upgrades.
7. Implement update and recovery testing before widening real-hardware installation.

## Evidence

Screenshots taken during 14 November 2026 development show first-run setup, sign-in, Home, Settings, Assets and Resources. Do not represent these images as proof of newer source features until the corresponding image has been rebuilt and tested.

See [roadmap](roadmap.md), [architecture overview](overview.md), and [server console](server-console.md).
