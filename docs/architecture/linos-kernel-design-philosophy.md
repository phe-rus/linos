# Linos kernel: design philosophy and engineering architecture

**Status:** Architectural proposal, 16 November 2026. Research and design only: no independent Linos kernel implementation is claimed. Follow [platform/update proposal](kernel-platform-updates-proposal.md), [product](../../PRODUCT.md), and [architecture overview](overview.md).

## Mission: an integrated system, not a cosmetic fork

Linos should deliver trustworthy, understandable, responsive computing across Server, Desktop PC/laptop, TV, Mobile phone and Car infotainment. The ambition is independent Linos operating-system *behaviour and architecture*. The project does not become independent because it changes a Linux release string, scheduler parameter or source directory name.

The initial kernel implementation will be a Linux-derived kernel; it must be represented truthfully and respect the kernel GPL-2.0-only license and relevant third-party terms. A genuinely independent, clean-room kernel could eventually be explored, but it is an entirely different scope involving syscall and driver compatibility, firmware and platform enablement. No shortcut or claim of ownership over upstream code. Design new Linos mechanisms in isolated, documented subsystems and replace upstream mechanisms only with proven benefits.

Apple's XNU demonstrates engineering through an integrated system built on other technologies (Mach, BSD, IOKit), not reinvention of every piece. Apple's quality-of-service work classification, user-space driver isolation and signed-system-volume/verified boot design are useful *principles*. Android GKI provides another useful lesson: separate common kernel and hardware-specific code with versioned compatibility interfaces. Neither is a template to copy wholesale.

## Twelve architectural principles

1. **Physics first:** hardware has finite frequency, bandwidth, memory, thermal dissipation, battery capacity and communication latency. No software can exceed physical bounds.
2. **Evidence over intuition:** measure input-to-display latency, tail latency, scheduler wait, thermal throttling, energy per completed task, bandwidth and reliability on named hardware.
3. **End-to-end responsibility:** smoothness is a chain from input device through interrupt, scheduling, application, rendering, compositor, GPU and display scanout. Kernel tuning cannot alone guarantee it.
4. **Mechanism/policy separation:** the kernel enforces scheduling, memory, isolation and hardware mechanisms; Linos platform services define contextual, authorized device/edition policies. Enter kernel only where bounded latency, security or integrity demands it.
5. **Explicit capability model:** discover what is actually observable, require evidence before a device is usable and expose uncertainty instead of silently pretending support.
6. **Deterministic safety:** probabilistic hardware *ranking* never licenses unsafe MMIO probing, unknown voltage changes, arbitrary firmware loading or safety-critical control.
7. **Graceful degradation:** preserve a usable text recovery, basic display, input and storage path when an optional peripheral or accelerator is unsupported.
8. **Reliability before peak speed:** a fast average with destructive tail latency, failed boots, shortened battery life or data loss is unacceptable.
9. **Composability without accidental complexity:** each kernel subsystem has a narrow interface, documented ownership, bounded allocations and failure semantics.
10. **Compatibility has a price:** Linux syscalls, standard device and graphics interfaces and upstream drivers remain valuable. A break requires migration tests, userspace cost accounting and compelling results.
11. **Verifiable provenance and recovery:** every production boot is a compatible, authenticated set of kernel, modules, DTB/ACPI dependencies, firmware, initramfs and OS image; updates are recoverable.
12. **Users retain control:** automation explains decisions, allows safe overrides and never quietly violates privacy, permission or device-owner choices.

## The real hardware discovery problem

A fully automatic universal driver is physically impossible in general. PCI/USB can be enumerated, ACPI/UEFI describe much PC hardware, and Device Tree describes ARM/embedded boards. Other peripherals (e.g. I2C, SPI, MIPI camera sensors, panels, regulators, modems) often lack standardized self-description, require board wiring data, vendor firmware or specialized protocols. No algorithm can uniquely infer missing board connections from a CPU identifier.

Proposed **Linos Capability Graph** (LCG): an evidence-backed representation of devices, features, resources and dependencies, assembled jointly by the kernel's supported detection mechanisms and a privileged platform service.

- Nodes: SoC, CPU cluster, GPU, display, sensor, clock, regulator, bus, radio, storage, driver, firmware, capabilities.
- Edges: depends-on, powers, connected-to, provides, consumes, controls and conflicts-with.
- Evidence: bus enumeration, signed OEM manifest, DT/ACPI, driver-reported facts, device IDs, observed tests, provenance and version.
- State: discovered, candidate, verified, degraded, unsupported, blocked. A confidence score is advisory, never a security authorization.
- Device identities are exact where required. Matching CPU architecture alone **never** certifies a phone or board.
- On conflicts, prioritize independently verified sources, quarantine suspect profiles, show understandable diagnostics and require explicitly safe recovery. The fallback can be "unsupported".
- Human-readable presentation in Settings/Recovery and stable machine-readable interfaces for qualified clients.
- Signed per-device board manifests describe hard-coded unknowns; they are **not** a failure of dynamic discovery but necessary input for non-enumerable hardware.

Example schema (illustrative, not implemented):

```json
{
  "schema": 1,
  "board": "example-phone-revision-a",
  "architecture": "arm64",
  "sources": [
    {"type": "device-tree", "identifier": "vendor,example-board", "trust": "verified"},
    {"type": "i2c", "identifier": "bus-2:0x36", "trust": "unverified"}
  ],
  "capabilities": {
    "display": {"status": "verified", "driver": "named-driver", "requiredFirmware": null},
    "camera": {"status": "unsupported", "reason": "no trusted sensor mapping"}
  }
}
```

The example does NOT instruct software to probe the I2C address; unverified identifiers must not be accessed without a known-safe binding.

### Inference, probabilistic reasoning and proof

Use probabilistic inference for **non-destructive candidate selection** and measurements, not guesses about power rails. Given evidence E and profile H, Bayes' rule can rank candidates:

`P(H | E) = P(E | H) P(H) / P(E)`.

In practice, probabilities require calibrated data; arbitrary scores disguised as precise probabilities are worse than honest unknowns. A profile can be activated only when independent **deterministic constraints** are satisfied: matching board ID/revision, driver, firmware provenance, required buses/regulators, permitted operating range and tested security posture.

## Responsive policy: from physics to decisions

The scheduling system is not a mystical "intelligence" inside the kernel. The kernel implements bounded mechanisms; platform services may build predictions from recent metrics and apply policy through well-defined Linux mechanisms (cgroup v2, PSI, scheduler hints, I/O control, power governors).

Use constraints first:

- All hard deadlines for medically/safety critical functions are **not** implicitly supported. Car scope is non-safety-critical infotainment only.
- Interactive tasks have a frame/deadline budget; background tasks cannot silently starve input, audio or essential services.
- Memory, power and thermal safety constraints have priority over cosmetic responsiveness.
- Decisions have bounded computation cost and must be stable under noisy sensor data (hysteresis, minimum dwell time, rate limits and defaults).

For a display refresh rate `f`, the frame period is `T = 1000/f` milliseconds. Thus 60 Hz gives 16.67 ms, 90 Hz gives 11.11 ms and 120 Hz gives 8.33 ms. This is the *whole frame pipeline budget*, not an entitlement to use all of it for CPU work.

Define an observed latency chain `L = L_input + L_queue + L_app + L_render + L_composite + L_scanout`, noting overlap and asynchronous pipelines when measuring reality. Track P50/P95/P99 input-to-display latency and missed-deadline fraction, not only FPS.

Power and thermal constraints: `E = integral P(t) dt`; a simple RC thermal approximation is `C dT/dt = P_heat - (T - T_ambient)/R`. Calibrate to measured hardware; a simplistic model does not replace hardware thermal safeguards.

Policy choice is **multi-objective**, not an invented universal number. For safe candidate decisions `a`, minimize a cost such as

`J(a) = w_L P99_latency(a) + w_E energy(a) + w_M memory_pressure(a) + w_T thermal_risk(a) + w_F failure_risk(a)`,

subject to isolation, data integrity, hardware safety, memory limits and deadlines. Weights are scenario/hardware-specific, dimensionally normalized, measured and documented. A Pareto-frontier comparison is preferable when different objectives cannot defensibly be collapsed into a weighted scalar. Avoid expensive machine learning in kernel fast paths. Prefer validated models in controlled userspace with transparent fallbacks.

Linux EEVDF and energy-aware scheduling already implement sophisticated mathematical ideas. Benchmark before replacing them. Linos' initial differentiator should be intent-aware userspace policy and full-pipeline instrumentation, not an unsupported claim to have invented optimal scheduling.

## Code structure and ownership boundary

The primary repository remains `phe-rus/linos`, `main` only. Keep upstream Linux source out of Git history. Build inputs are pinned, fetched and verified; Linos changes remain reviewable.

```text
linos/
  kernel/
    README.md
    PHILOSOPHY.md
    sources.lock.json       # stable+research exact source revisions/digests
    upstream/               # fetch recipes, license/provenance metadata
    config/
      common/               # required security, fs, cgroup, scheduler, isolation
      arch/{amd64,arm64}/
      work/{server,interactive,media,infotainment}/
      board/{qemu-amd64,qemu-arm64,pc-r1,rk3588,...}/
    linos/                  # Linos-authored kernel-side mechanisms only
      core/                 # narrow interfaces, versioning, invariants
      capability/           # exposure of verified capabilities, no magic probing
      scheduling/           # optional carefully measured scheduler hooks
      memory/               # optional pressure/priority integration
      io/                   # latency and block/network policy integration
      security/             # integrity, authorization hooks
      power/                # bounded interactions with governors/thermal limits
      telemetry/            # low-overhead diagnostics
    patches/                # ordered, provenance-tracked integration patches
    abi/                    # Linux UAPI expectations and Linos interface contracts
    profiles/               # supported targets + firmware/DTB/ACPI dependencies
    scripts/                # source verify, Kconfig compose, build, package
    tests/
      unit/                 # KUnit, property tests for small algorithms
      integration/          # kselftest, kernel/driver tests
      qemu/                 # amd64/arm64 boot; fault injection
      devices/              # device profile qualification
      benchmarks/           # reproducible latency/energy/throughput comparisons
    research/
      questions/            # questions, existing prior art, hypotheses
      models/               # documented mathematical models and assumptions
      experiments/          # experimental method, baselines, reproducibility
      decisions/            # adopted/rejected policy with evidence
  crates/
    hardware/               # proposed Rust userspace capability graph service
    policy/                 # proposed Rust workload and resource policy
    updates/                # proposed verified manifest/update service
    platform/               # versioned public Linos platform interfaces
  system/
    profiles/               # edition/device policies, no duplicated kernel source
  docs/
    architecture/
    kernel/
      support-matrix.md
      perf-contract.md
      interface-contracts.md
```

These are **proposed** directories, not new executable components. No abstract layer should be added merely to match a diagram. For kernel patches, isolate actual Linux-specific C/Rust changes, document compatibility with upstream versions and prefer an upstreamable interface. Kernel-side Rust is limited by what the upstream Rust-for-Linux infrastructure and target drivers support; userspace Rust remains the default for Linos services.

**Build variants**: `research` follows a recent stable Linux kernel and experiments behind explicit flags, while `qualified` stays on a maintained longterm branch until a full device/edition test matrix passes. Each build records source revision, complete resolved .config, toolchain, patch inventory, modules, firmware, test evidence and trust metadata. Neither branch name nor version alone grants supported status.

**Kernel-private versus public interfaces:** prefer Linux's stable userspace contracts (syscalls, netlink, sysfs, DRM/KMS, input) and small versioned Linos netlink/sysfs interfaces where justified. Do not promise a stable internal kernel ABI across versions. A Linos SDK calls a stable platform API, not arbitrary internal kernel implementation.

## Five device families as workload contracts

| Device | Non-negotiable behaviour | Primary tests |
| --- | --- | --- |
| Server | Data durability, networking, recovery, no interactive UI dependency | Boot under faults, storage/network throughput and tail latency, load/PSI |
| Desktop PC | Smooth input/rendering, multi-display, suspend, safe graphics reset | Frame pacing, input-to-photon, GPU failures, USB hotplug, laptop power |
| TV | Stable playback, lip-sync, remote input, predictable wake, codec fallback | Audio/video drift, dropped frames, HDMI/CEC where supported, decoders |
| Mobile | Battery/thermals, touch latency, suspend, radios, sensors, secure firmware | Radio availability, thermal stability, suspend-resume, camera (real devices) |
| Car infotainment | Fast safe startup, isolated vehicle-data read interfaces, audio/navigation | Fault injection, offline navigation, startup, interface isolation; **no safety-critical actuation** |

There is no automatic path from a phone's architecture to its modem, display or camera functionality. Some vendor drivers and firmware are not publicly available or redistributable. Qualification depends on supply, permissions and actual tests.

## CI as an experimental laboratory, not a substitute for devices

For public GitHub repositories, standard GitHub-hosted compute is currently free and available without a monthly minutes cap; concurrency, per-job disk/CPU/RAM, fair use, bandwidth and **artifact storage** are limited. Larger runners and storage over plan allowances can cost money. Kernel builds can be too large for a runner's available disk unless outputs are minimized.

Use a staged matrix:

1. Per change: source integrity, schema/static checks, license scan, clang/GCC analysis of touched code, model/property tests and Rust unit tests.
2. Per kernel policy change: compile both architectures with pinned toolchains, verify final Kconfig, KUnit, kselftest, QEMU boot/rollback/persistence and negative security tests.
3. Per graphical/platform change: QEMU virtio GPU / software rendering for correctness, frame event traces, input replay and screenshots; **do not** pretend virtual FPS equals physical GPU performance.
4. Nightly: cross-kernel regression against latest qualified and research branches; fuzzing, fault injection, multiple CPU/RAM/storage/thermal constraint simulations.
5. Before any device promotion: named physical hardware validation. A Linux QEMU VM with a phone-size display tests *Linos phone layout*, not a real phone's bootloader/modem/power regulators.
6. Optional external labs/self-hosted runners for board power, hardware-in-the-loop, real thermal and battery measurements.

AOSP Cuttlefish is useful for **Android/AOSP** virtual-device behaviour, but cannot validate a standalone Linos phone kernel and custom platform without integration work; Cuttlefish's normal usage requires host virtualization (KVM). QEMU can run Linos directly at earlier stages with simulated virtual peripherals.

Release assertions are machine-readable: `source_checked`, `built`, `qemu_booted`, `functional_passed`, `hardware_qualified`, `release_signed`. Always preserve the exact test input/expected result, not just green build badges.

## First research experiments

**Experiment A:** Evaluate recent stable vs latest longterm on identical QEMU hardware and userspace images: boot reliability, cold start time, system memory, filesystem/network throughput and scheduler tail latency. Use multiple runs, report confidence intervals, and keep an unmodified upstream baseline.

**Experiment B:** Implement capability graph in a Rust userspace crate with synthetic PC/phone/TV/automotive profiles. Test conflicting evidence, unknown board, dependency cycles, signed manifest mismatch, absent driver and hotplug. **No real probing** before safe support is established.

**Experiment C:** Measure interactive-load policy on a Desktop guest: synthetic input under CPU+I/O saturation. Compare stock scheduling, tuned cgroup/priority policy, and Linos policy. Calculate percentiles, dropped deadlines and CPU/energy costs. Propose kernel changes only if platform APIs are insufficient.

**Experiment D:** Signed release and A/B trial with intentionally corrupted firmware/boot payload, incompatible hardware profile, interrupted download and failed health check. Ensure automatic rollback and user-data preservation.

## Philosophical test of any architectural decision

Before a Linos kernel feature merges, answer:

1. What user-observable or reliability problem exists, on which device, under what workload?
2. What is the physical limit and which uncertainty is fundamental?
3. What does current Linux/Android/Apple or published research already solve? Why is it insufficient here?
4. Which kernel mechanism versus userspace policy should own the change and why?
5. What are the measurable acceptance criteria, counterexamples and failure conditions?
6. What are the security, energy, privacy, recovery and licensing costs?
7. What evidence would cause us to reject or reverse the idea?

**Engineering rule:** perfection is a direction of disciplined falsification and improvement, not a test claim. A model that cannot explain its limitations, and a decision we cannot safely roll back, do not belong in the Linos qualified kernel.

## References

- Linux upstream releases: https://www.kernel.org/
- Apple XNU architecture: https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/KernelProgramming/Architecture/Architecture.html
- Apple QoS: https://developer.apple.com/documentation/dispatch/dispatchqos
- Apple signed system volume: https://support.apple.com/guide/security/signed-system-volume-security-secd698747c9/web
- Apple DriverKit/user-space extensions: https://developer.apple.com/documentation/systemextensions
- Linux device tree usage: https://docs.kernel.org/devicetree/usage-model.html
- Linux EEVDF scheduler: https://docs.kernel.org/scheduler/sched-eevdf.html
- Linux energy-aware scheduling: https://docs.kernel.org/scheduler/sched-energy.html
- Android Generic Kernel Image: https://source.android.com/docs/core/architecture/kernel/generic-kernel-image
- Android Cuttlefish: https://source.android.com/docs/devices/cuttlefish
- GitHub Actions standard runner limits: https://docs.github.com/en/actions/reference/limits
- GitHub Actions billing: https://docs.github.com/en/billing/concepts/product-billing/github-actions
- Linux KUnit/kselftest: https://docs.kernel.org/dev-tools/testing-overview.html
- Linux licensing: https://docs.kernel.org/process/license-rules.html
