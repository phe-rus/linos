# Linos 13: current engineering status

Updated: 23 November 2026. Channel: development. Historical interface evidence below is from 14 November; current Server, Recovery and VM qualification is recorded separately below. This is not a production release certification.

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
| Settings | Saved user preferences and confirmed hostname/timezone controls; language and keyboard remain in setup | See the current interaction qualification below |
| Rust quality | Current workspace: 133 tests, formatting and strict Clippy on Rust 1.96.1; two separate explicit AMD64 guest tests; earlier kernel-tool and Rust 1.100.0 evidence is retained below | Current source verified locally |
| PTY | Persistent cwd/environment, ordinary commands after sudo, masked input after Ctrl+L and native Shell return verified in the visible AMD64 guest | Text preview, full terminal emulation remains open |

## Management views exercised in AMD64 QEMU

- `/hardware`: Linux-reported device classification, architecture, CPU, memory, kernel, firmware and battery presence. This is identification, not a compatibility certification.
- `/store`: Linus store package inventory, installed size estimates, repository search, package metadata and simulated install/remove operations. There is no authenticated transaction UI or independent Linos package repository.
- Consistent product casing across Home and these new views.

Hardware and store views were exercised in the visible native-kernel Server VM on 23 November. This does not qualify physical hardware or actual package transactions.

## Product identity and independence gap

Linos aims to be a consumer operating-system product, **not a Debian spin with a custom shell**. Upstream repositories are a pragmatic early source of software. They are not intended to prescribe the final Linos kernel policy, runtime contracts, application behaviour or user experience.

**What exists:** Linux-based QEMU images, Linos-owned kernel configuration and source overlays, packaged AMD64/ARM64 kernels with successful CI boot and installed-system checks, systemd/PAM/APT foundations, Rust-owned setup and console features, a read-only Assets browser, Hardware and Linus store views, and signed development image update/rollback tooling. Older compatibility images use Debian kernels.

**What does not yet exist:** a production-qualified kernel and physical hardware support matrix; measured production latency/rendering/input budgets; a native Linos Rust UI/application SDK; an integrated multi-edition app manifest, sandbox and lifecycle; a fully featured Assets file manager; a developer-facing Linus store; and a complete production update and release architecture. Desktop is still an X11 window experiment, not a bootable Desktop platform.

The difference is important. No currently demonstrated Linos image proves the intended deep operating-system independence. A source tree expressing those intentions is a plan, not validation.

## The user-experience standard

The target is not just that Linux commands or package installation technically work. The system must be understandable without technical instruction, feel consistently responsive, recover from errors safely, maintain deliberate interaction and visual rules, and support advanced tools without requiring ordinary users to become Linux specialists.

Kernel and resource policies must be measured and reproduced per hardware profile, and cooperate with application frameworks, display/input rendering, services and device drivers. The kernel alone does not create smooth animations or native application consistency.

## Critical blockers

1. **Command composer:** password rendering and routing defects found in the VM have regression fixes and live verification in build `20261123.4`. Secret masking follows PTY echo state and sends values directly to the PTY. Embedded command output remains a text preview; full-screen tools still require native Shell mode.
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

1. Preserve the verified first-boot, PTY secret-input and interrupted-setup regressions.
2. Run full CI and native AMD64/ARM64 kernel integration against the new source before publication.
3. Retain the fresh immutable build and visible VM evidence, keeping image qualification distinct from mutable guest tests.
4. Complete store inventory accuracy and repository/error qualification across larger installations and supported architectures.
5. Extend Settings controls beyond the qualified preferences/hostname/timezone slice and add an explicitly authenticated transaction workflow for package changes. Ordinary-user sudo authorization for hostname/timezone is now covered in a disposable guest.
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
checkpoint does not publish a release. Fresh native Recovery qualification was
completed in the continuation below; the complete CI release gate remains open.

### 23 November continuation

Fresh AMD64 Recovery build `20261122.2` reused the qualified
`linos-kernel_1.0.0_amd64.deb`. The builder checked package metadata and equality
of the extracted kernel bytes. Its independent UEFI image reached the Rust
Recovery menu under KVM with one vCPU and 768 MiB RAM. See
`target/fresh-recovery-build.log` and
`images/recovery/builds/amd64/20261122.2/recovery-smoke.log`. Installation and
repair remain disabled. The Recovery test now adds system utilities to PATH,
matching the Server UEFI test, so missing `debugfs` does not masquerade as an
invalid hardware profile.

The full on-demand compatibility and installer-media CI workflow was started
at commit `40ac0c2`: [run 45076704644](https://github.com/phe-rus/linos/actions/runs/45076704644).
The CI failure was traced to the newer toolchain's
`chunks_exact_to_as_chunks` lint in the EFI filename decoder. A local fix uses
fixed size chunks after the existing length validation. Whole-workspace strict
Clippy, formatting and the recovery/update regression tests passed locally.
The changed source has not yet been pushed or verified in CI. This workflow
does not replace native AMD64/ARM64 kernel integration
and publication gates. The earlier green Kernel Release workflow at this commit
ran only Rust validation; build, boot, integration and publication were skipped.
No beta release was published here.

An interactive user-session libvirt guest named
`linos-server-marauder-sequence9` was started with one vCPU and 768 MiB RAM.
Virtual Machine Manager was launched on `qemu:///session`. The guest uses a
separate writable copy under `images/local-vms/server-marauder-sequence9/`,
preserving the qualified image. Its interface is the serial console. Release
metadata already identifies Marauder, Sequence 9, generation `1.0.0`; the
channel remains development.

Desktop baseline: `cargo check --locked --bin linos-graphics` passed. This
checks the existing X11 experiment only. Native graphical boot, compositor,
setup/login and display-failure recovery remain the next Desktop implementation
work, as described in [Desktop platform](desktop-platform.md).

### Interactive Server audit and fixes

The initial Virtual Machine Manager guest selected Microsoft-key Secure Boot
firmware automatically and remained in firmware settings. Its local definition
now pins standard OVMF UEFI and a separate variable store, matching the successful
boot tests. Secure Boot remains unqualified.

Fresh Server UEFI media no longer masks enrollment. Its smoke test requires the
real administrator setup prompt. Enrollment grants the local administrator
`sudo` and `adm` membership so system logs are readable without running Home as
root. Setup maintains local hostname resolution and flushes configuration files
and parent directories before publishing completion. Build `20261123.1` exposed
an empty `/etc/hosts` after interrupted setup; the durability correction passed
the repeated interruption and subsequent authenticated hostname/journal checks
in build `20261123.2`.

Build `20261123.2` passed direct boot, UEFI setup, interrupted/resumed enrollment,
PAM login, native kernel readiness, SSH restrictions, the bounded memory check,
all six managed A/B update boots, independent Recovery UEFI boot and offline
payload deployment to a disposable GPT System A. Local VMs ran sequentially
with one vCPU and 768 MiB RAM. Evidence is under
`target/qualification/server-fixed2-*.log`; the retained enrolled fixture is
identified by `target/qualification/server-fixed2-enrollment-fixture`.
Account/data migration and physical installation remain unqualified.

The compact console renderer previously showed characters typed into real sudo
prompts. Its fix masks secret input and bypasses slash routes for secret values.
A further regression covers Ctrl+L and translated prompts: masking depends on
PTY echo state and command ownership, not English text. These tests failed before
their respective corrections and passed afterward. Final source passes 100
application tests and strict Clippy on Rust 1.100.0, plus six kernel-tool tests.
Server build `20261123.4` includes the final console corrections. Background
terminal-size probing was removed because it read and discarded keyboard bytes
while waiting for size replies. Startup negotiation and explicit Ctrl+R remain,
alongside native terminal resize events.

The final visible VM passed fresh UEFI setup with the owner's requested local
test credentials, automatic reboot, PAM login, Home, real sudo rejection and
acceptance, secret masking after Ctrl+L, ordinary commands afterward, non-root
identity, hostname resolution, journal access, native kernel readiness and
Shell-to-Home return. A six-second idle observation confirmed background size
probes stopped. The deliberately incorrect secret was absent from rendered
serial output. Separate embedded commands retained an exported value and cwd.
Earlier live checks also exercised management views, missing Assets paths,
empty store search guidance and default-cancel shutdown confirmation.
See `target/qualification/vmm-final4-results.log`, `vmm-final4.log` and
`server-visible-serial.log`. The user-session VM is left running at Home with
the test console detached and Virtual Machine Manager reconnected. Explicit
Ctrl+R refreshed the guest to the visible window's 46-row geometry. The final
immutable image's console bytes match the compiled executable. Earlier writable
guest disks are preserved beside the current copy.

The six update boots and independent Recovery/installation tests above used
build `20261123.2`, before the final console-only corrections. Final source
passes strict Clippy and 100 application tests with Rust 1.100.0. Formatting,
shell syntax and diff checks pass. The changes remain local; no beta was published.

### Server interaction and resource controls, 23 November

The next local console continuation adds repeated-key and Unicode editing,
bounded paste and history, explicit shell completion/exit status, progress-line
handling, native terminal handoff, measured CPU bars, live task controls and
working Settings mutations. Background inspection reads leave the key loop
available. Confirmation panels and default cancellation protect system/task
changes. Secret masking remains active across view changes.

The current workspace suite passes 123 tests. The real-terminal driver covers
all navigation views, resource refresh, persisted preferences, editing/history,
secret input, failed commands, task signals, native interruption/return and
logout. The separate sequential AMD64 QEMU check uses one vCPU and 1024 MiB,
with the new release console injected into a disposable copy of the native
`20261123.4` image. It also applies and reads back hostname/timezone settings.
This is a console qualification, not a newly packaged immutable image or a
repeat of enrollment, ordinary-user PAM/sudo or network workload qualification.

See [the interaction review](../reviews/2026-11-23-server-interaction.md) for
commands, source behavior, artifacts, product philosophy comparison and the
remaining release/workload gates. Prior immutable-image evidence in its
original checkpoint continues to refer to that older packaged source.

### Ordinary-user authorization and bounded queries, 23 November

The current continuation closes the ordinary-user sudo qualification gap for
hostname/timezone Settings. The guest verifies masked real authentication,
cancellation without mutation, successful authorized changes, normal commands
after sudo and denial of cross-user process signals. It also reproduces and
fixes local hostname resolution after renames through the standard
`libnss-myhostname` Server package dependency.

Task, Settings and power dialogs now reject modified Y shortcuts and repeated
keys. External inspection helpers have cancellation, a five-second deadline,
a combined four-MiB output limit and process-group cleanup. Store failures no
longer become zero installed packages, and complete inventory is parsed before
the displayed list is limited.

The workspace passes 133 tests. Both explicit AMD64 guest tests install the
rebuilt development package and resolver dependency in disposable image copies,
sequentially at one vCPU and 1024 MiB. Formatting and strict Clippy pass. The
local `.deb` is rebuilt, but a new immutable image, standard enrollment/login,
SSH and broader server workloads remain separate gates. See [the authorization
and query review](../reviews/2026-11-23-server-authorization.md) for hashes,
reproduction steps, artifacts and exact limits.
