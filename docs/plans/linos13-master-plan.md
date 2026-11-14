# Linos 13 master development plan

> Product authority: [PRODUCT.md](../../PRODUCT.md). Vision: [Why Linos exists](../vision.md).
>
> Updated 14 November 2026. This is a staged engineering plan, not a claim that future capabilities already ship. Existing code and history remain on the single `main` branch. All milestones require tests and evidence; avoid large speculative rewrites.

## The target

**Linos is a Linux-based operating system redesigned for ordinary consumption, not a Debian spin.** Software packages may initially come from stable upstream repositories. Linos must gradually own the platform contracts that determine what applications, the kernel, system services, resource policies, UI and releases feel like together.

The product question at every stage is: *Can someone who isn't a Linux specialist perform the task comfortably, understand the result and recover when something goes wrong?*

The reference isn't a macOS skin. It is the amount of care put into the complete user experience: responsive input, consistent language, considered visual physics, safe permissions, system intelligence and coherent defaults.

## Operating rules

- **One source of truth:** root `PRODUCT.md` for confirmed product facts; eventual `DESIGN.md` for the audited existing visual and interaction system; this document for execution; `docs/architecture/roadmap.md` for evidence-linked milestone status.
- **Preserve compatibility:** do not change the running boot path, package database, authentication model or partitioning without controlled migrations and disposable-device tests.
- **Safe by default:** no password caching in the UI, silent elevation, automatic disk writing or unexplained service stopping. Risky work has explicit authorization, preview, progress and recovery.
- **Performance is measured:** an appealing animation or kernel option is not proof of responsiveness. Record input latency, dropped frames, CPU/memory, power, disk I/O, PSI and thermal behaviour where supported.
- **No ceremonial stages:** every milestone has observable acceptance criteria, test coverage and a user-visible demonstration.
- **Canonical names:** Linos, Linos 13, Assets and Linus store. Acronyms retain uppercase; Rust crate and binary names use lowercase.
- **No fabricated maturity:** mark functionality as *planned*, *source implemented*, *tests passed*, *boot demonstrated*, *real hardware validated*, or *release qualified*.

## Where we are today

| Area | Completed or demonstrated | Improvement / blocker | Current classification |
| --- | --- | --- | --- |
| OS base | QEMU amd64 and arm64 boot; development packages and rootfs generation | Debian kernel is still installed by image builder; kernel policy unowned | Demonstrated prototype |
| First boot | Local enrollment and Cliclack setup | Stronger recovery, interrupted setup and accessibility coverage | Demonstrated |
| Sign-in | Local PAM authentication; remembered username | Validate all login/prompt security states; no automatic elevation | Demonstrated, hardening pending |
| Home | Ratatui workspace and context navigation | Command composer can still become incorrectly stuck at `Password (hidden)` | Development blocker |
| Shell | Native Bash switch and persistent PTY execution | Full interactive terminal semantics and secret handling require a robust design | Partly demonstrated |
| Assets | Read-only keyboard file navigation | Previews, search, file operations and eventual graphical manager | Demonstrated initial view |
| Resources | CPU load, memory, swap and storage readings | Metrics vs actual utilization, PSI, service workload and resource policies | Demonstrated initial view |
| Settings | Read-only inspection | Real preferences, hardware-specific controls, undo, permission boundaries | Early prototype |
| Hardware | Linux-based detection code | Real hardware verification, capabilities, drivers, thermal/power information | Source only |
| Linus store | dpkg/APT inventory, search and simulated operations code | Transaction UX, confirmation, repository trust, rollback and developer distribution | Source only |
| Recovery | Separate recovery and disposable installation proof-of-concept | Safe physical install, verified update recovery and data preservation | Experimental |
| Rust tests | 54 Rust tests plus rustfmt and Clippy passed at `4db71a0` | Re-run on every later revision; add hardware/PTY integration tests | Historical verified baseline |
| Screenshots | Supplied actual VM setup, sign-in, Home and sizing evidence | Commit carefully selected labeled images to GitHub showcase | Evidence available, not published |

**Release status:** development preview, not validated alpha, beta or production.

## Stage 0: establish a product quality contract

**Deliverables**

1. Root `PRODUCT.md` initialized from confirmed product facts.
2. Run Impeccable `document` against the *existing* Home/first-boot UI and create `DESIGN.md`, rather than inventing a new aesthetic.
3. Audit each surface: first boot, login, Home, composer, navigation, Assets, Resources, Settings and terminal resizing.
4. Create measurable interaction design criteria: task completion without instructions, readable errors, keyboard traversal, focus behavior, small-screen/serial handling, reduced-motion and user consent.
5. Establish GitHub-native screenshots of actual versioned images with build ID, date and verified feature caption.
6. Add UI regression tests and screenshot evidence to CI for supported QEMU terminal dimensions.
7. Fix documentation drift so user-facing pages link to product, design, evidence and release status.

**Acceptance:** another contributor can explain the Linos promise, see its real current UI, identify blockers, and review a UI change against a documented product and design contract.

## Stage 1: finish the Linos 13 Server alpha interaction loop

**Do this before another public alpha claim.**

- Fix the PTY password lock by using a reliable command state/PTY protocol, not only output-text heuristics. Test idle shell, `cd`, `ls`, multiple commands, shell prompt changes, `sudo` and cancellation without ever storing or logging secrets.
- Ensure Home and native Shell can switch back and forth without losing relevant context or leaving the terminal damaged.
- Split full-screen terminal execution from the read-only plain text command transcript; provide safe fallback to native Shell for tools requiring cursor control.
- Verify Ctrl+R, real size events, terminal minimum sizes, serial/SSH behavior and zero uncontrolled redrawing.
- Complete system views: accurate resource statistics, refresh controls, real status/progress, empty/error/loading states and assistive labels.
- Improve Assets' file browser ergonomics, selection, directories and error recovery.
- Add tests that boot an immutable new image and exercise sign-in, Home commands, logout and source-to-package freshness.

**Acceptance:** a novice can log in, understand device health, navigate Assets, run basic commands, return from Shell and log out without seeing a false password prompt or a broken layout. All tests and QEMU flows pass.

## Stage 2: define the Linos-owned kernel and runtime policy

**Do not confuse an upstream kernel update with an implemented Linos kernel.**

- Pin kernel source/release and identify licensing, upstream maintenance support, modules, firmware and supported architectures.
- Keep a **common baseline plus hardware profiles**, not a single universally optimized config.
- Source-control kernel configuration fragments, generation scripts, hashes and policy checks. Produce a reproducible kernel package and evidence manifest.
- Test boot, syscalls, networking, filesystems, drivers, memory pressure, cgroups, security modules, suspend/wake and failure/recovery for supported platforms.
- Establish measured profiles for latency-sensitive foreground work, stable server services, energy efficiency and background maintenance.
- Add performance budgets with automated regression measurement and rollback when kernel upgrades fail compatibility checks.
- Keep all initial Linos performance decisions reversible and traceable. Avoid disabling security features merely for benchmark scores.

**Acceptance:** the selected kernel configuration can be rebuilt and verified for the supported target matrix; a version bump fails CI if required behavior/configuration drifts; measured results are published and regressions block release.

## Stage 3: a common Linos platform contract

- Define local device model and capabilities: CPU topology, memory, storage, graphics/input, thermals, firmware, battery and networking, while clearly marking unavailable measurements.
- Introduce system policy services around authenticated settings, capabilities, updates, application lifecycle, file operations and events. Rust libraries expose typed contracts to all editions.
- Specify permissions, consent, user identity, audit history and the separation between ordinary app actions and privileged system changes.
- Use Linux-native interfaces such as systemd, cgroup v2 and PSI where appropriate, with Linos policies layered above them. Do not pretend the underlying Linux mechanisms have been replaced.
- Document versioning and compatibility: what is stable between OS generations, what can be rolled back, and what must be migrated.

**Acceptance:** multiple Linos tools interact with a single documented authorization and capability model, rather than implementing unrelated privileged shell commands.

## Stage 4: native Rust application experience

- Define what a **native Linos app** is: manifest, lifecycle, window or terminal surface, state, accessibility, permissions, identity and package metadata.
- Prototype the Linos-native Rust UI stack and component library. Select graphics/compositor architecture through experiments; Rust is the implementation language, not automatically the renderer.
- Specify motion and interaction physics as measurable easing, input response, timing, frame pacing, gestures, accessibility and reduced-motion contracts.
- Prototype a single-file Rust workflow: obvious entry command, compile-on-demand, diagnostics, caching and a safe permission policy. Treat arbitrary source code as potentially untrusted.
- Build a minimal but real first-party sample using the same SDK external developers will use.
- Define compatibility tiers for native, integrated Linux and legacy applications; avoid a promise that unmodified apps can always be forced to behave identically.

**Acceptance:** a developer can author, build, run, inspect and uninstall a native Rust sample while users experience consistent system prompts and interactions.

## Stage 5: first-party applications and software distribution

### Assets
- Read-only browse → search and preview → confirmed file mutation → metadata/indexing → native desktop edition.
- Stable keyboard, pointer and touch equivalents as supported by each edition.
- Undo/trash, permissions, external storage and error recovery before bulk destructive operations.

### Linus store
- Read-only installed package metrics and search → accurate previews → authenticated install/removal → progress, cancellation, history and recovery.
- Package provenance, repository signatures and compatibility with existing Linux packages.
- Future developer accounts, manifests, publishing review, releases and distribution across supported edition targets.
- No secret password storage, no silent root elevation and no pretending upstream apps are Linos-native.

### Settings, Resources and Hardware
- A clear hierarchy of read-only system facts vs editable preferences vs privileged administrative operations.
- Device-specific configuration surfaced only when supported.
- Accurate progress and metrics with labelled units, measurement periods and error states.

**Acceptance:** installing or removing an app is understandable, authorized, reversible where possible and comparable in polish to familiar consumer OS workflows.

## Stage 6: whole-device reliability, recovery and releases

- Transactional installer restricted to verified targets; separate boot, Recovery, OS and user-data policies.
- Signed update manifests, activation, boot-health validation, rollback, maintenance windows and interrupted-power testing.
- Standardize kernel, platform libraries, first-party apps and release manifests in one compatibility matrix.
- CI runs Rust static checks, unit/integration tests, QEMU amd64/arm64 smoke, UI interaction snapshots and upgrade/recovery simulations.
- Add real supported-hardware validation with signed manifests and a non-destructive test program.
- Define release channels: development → alpha → beta → stable, with supported update paths and documented support windows.

**Acceptance:** failed updates recover to a known bootable version with user data protected, and verified source maps to a signed release artifact.

## Stage 7: editions beyond Server

Server establishes the product's base contracts, but it is not the template from which every UI is mechanically resized.

- **Desktop/laptop:** display stack, graphics, native applications, input latency, accessibility, power, battery, Assets and software installation.
- **Single-board:** supported firmware/DTB, GPIO and hardware-specific limits, offline recovery, memory/power budgets.
- **TV:** focus and remote navigation, lean media rendering, clear permissions and updates.
- **Mobile:** touch, gestures, radio/cellular, suspended app lifecycle, energy/privacy constraints, hardware support and store suitability.

Each edition has its own hardware and experience qualification gate. An edition is not shipped because a common Rust crate compiles.

## Gate table

| Gate | Minimum evidence |
| --- | --- |
| Developer preview | QEMU boot, basic native Home, setup and known defects disclosed |
| Alpha | Reliable authentication/Home/command loop, stable packaging, no blocking UX/security defect, reproducible smoke and regression tests |
| Beta | Supported physical hardware, kernel/platform policy validated, secure app operations, updates/recovery tested and performance budgets measured |
| Stable Server | Signed reproducible release, support cadence, threat model, soak testing, reliable upgrade/rollback and incident/recovery procedure |
| Later edition | Meets common stable contracts plus form-factor-specific accessibility, rendering, hardware and power acceptance |

Do not promise a release date or percentage completion without validating the corresponding gate.

## How Impeccable fits

Impeccable is a **design review and iteration skill**, not a replacement for kernel engineering, Rust testing or security review.

- `init`: establish durable product facts in `PRODUCT.md` (done).
- `document`: inspect the incumbent UI and record the actual visual system in `DESIGN.md` (not run yet).
- `critique`: evaluate one real surface against product and usability principles.
- `audit`: inspect accessibility, responsiveness, focus, copy and visual consistency; Rust console limitations need manual/adapted checks.
- `shape`: define UX before new work, such as editable Settings or an Assets operation flow.
- `polish`, `adapt`, `clarify`, `harden`: implement focused improvements; validate on the real terminal and device.
- `extract`: consolidate stable UI patterns into Rust modules/components after their behavior is proven.

Since this repository currently has a native Ratatui UI rather than a browser-based product, do not expect the web live-preview workflow or CSS-focused checks to validate native terminal behavior automatically.

## Priorities for the next engineering cycle

1. **P0** False password-state lock; safe PTY prompt/input handling; command continuity.
2. **P0** Compile and boot-test newer Hardware and Linus store source; Rustfmt/Clippy/test clean on current head.
3. **P1** Impeccable `document` and audit of Home/onboarding, with a measured accessibility and terminal-resize matrix.
4. **P1** Source-controlled kernel config/profile prototype and baseline performance measurements.
5. **P1** Native Assets and Linus store UX specifications; keep mutations preview-only until authorization is designed.
6. **P2** Linos SDK/native app proof-of-concept and device/platform contracts.
7. **P2** Recovery/update qualification and physical-device matrix.

This ordering brings the immediate server experience back to reliability while beginning the long-term differentiators in parallel through well-contained prototypes.
