# Linos Server baseline and gaps

24 November 2026. Application baseline: `a682af8` (later handoff changes did not alter it). CI repair checkpoint: `307defd`. Server only. Read alongside the [primary source platform comparison](2026-11-24-server-platform-comparison.md).

## Assessment

Linos is a small, useful Server administration environment on Linux. Its strongest current qualities are local ownership, guided enrollment, ordinary PAM and sudo, keyboard navigation, native shell compatibility and deliberate confirmation. It is suitable for development VM experimentation by users prepared to use Linux tools. It is not yet a qualified replacement for an established server distribution, storage appliance, virtualization host or managed company fleet.

An Apple informed approach would finish a small set of coherent journeys, communicate actual state and consequences, preserve context on failure and make recovery dependable. It would constrain supported hardware before promising broad compatibility. Those are inferred recommendations, not claims about what Apple would build. Simplicity is the work a user avoids, not the number of controls hidden.

## Measurements

One baseline run, not a cross platform ranking. AMD64 QEMU/KVM, one host CPU exposed, one vCPU, 1024 MiB RAM, native `6.18.63-linos`, 100 by 32 serial terminal, no network. A fresh copy of native build `20261123.4` receives the current development package and resolver dependency. The observer and debug fixture run inside the guest. Package installation into this copy is not final image qualification or normal login validation.

| Quantity | Observed result |
| --- | --- |
| Console binary | 1,292,704 bytes |
| Compressed Server package | 654,552 bytes |
| Base image logical size | 3 GiB |
| Base image host allocation | About 349 MiB |
| Guest root used after fixture setup | 304,263,168 bytes (about 290 MiB) |
| Guest kernel visible memory | 991,428 KiB (about 968 MiB) |
| Home idle resident memory | 3,308 KiB (about 3.2 MiB), excludes other processes and shared accounting |
| Guest used memory during idle sampling | About 65 to 90 MiB, calculated as MemTotal minus MemAvailable |
| Idle Home CPU | No change in process user/system counters across observer samples 2 through 12; no claim of zero machine CPU |
| Home startup after shell command | 119 ms, single observation, excludes enrollment/login |

Latency is host send to visible assembled terminal response. Input tests use 30 samples, views 10, commands 15. Medians and 95th percentiles include harness, serial transport and host scheduling. The polling helper uses a 20 ms read poll, so small differences are not precise rendering measurements. Loaded means one additional bounded busy loop on the single guest vCPU. No memory or I/O pressure latency series, PSS, physical display timing or competitor timings were measured.

| Operation | Idle median / p95 | CPU load median / p95 |
| --- | --- | --- |
| Single typed character | 24 / 33 ms | 17 / 55 ms |
| Resources navigation | 178 / 394 ms | 384 / 2,042 ms |
| Hardware navigation | 3 / 1,667 ms | 2 / 2,827 ms |
| Services navigation | 226 / 2,671 ms | 478 / 5,423 ms |
| Simple command response | 254 / 407 ms | 316 / 675 ms |

The longest typed response was 217 ms under load. Ten view observations are too few for a stable tail estimate. The data identifies a responsiveness problem to investigate, not a proven minimum system requirement. This is lightweight at idle, but smoothness under work is not yet established. Serial redraws and input scheduling deserve investigation before scheduler tuning.

Raw evidence: `target/console-qualification/guest-benchmark/latency.csv`, terminal log and idle snapshot; `target/server-benchmark/baseline-samples.txt`, `baseline-system.txt`, `baseline-test-2.log` and the disposable `baseline-2/disk.ext4`. The first harness attempt failed because its text matcher expected the wrong prompt; it is excluded from these measurements.

## First improvement measurement

The bounded input batching and shorter wait while work is pending were measured in a second fresh guest copy with the rebuilt package. Idle still uses the original wait. The same terminal geometry, kernel, resources and workload apply. This run took 38 seconds including fixtures and extra status verification, versus 75 seconds for the earlier benchmark. That total is not a normal boot or application benchmark.

| Operation | Improved idle median / p95 | Improved CPU load median / p95 |
| --- | --- | --- |
| Single typed character | 21 / 37 ms | 16 / 62 ms |
| Resources navigation | 3 / 109 ms | 2 / 162 ms |
| Hardware navigation | 2 / 130 ms | 2 / 229 ms |
| Services navigation | 57 / 66 ms | 118 / 217 ms |
| Simple command response | 108 / 223 ms | 64 / 392 ms |

This supports a substantial reduction in the observed navigation/command delays, not a universal performance ranking. Typing tail behavior did not improve: the loaded maximum was still 218 ms. Single startup observation increased to 222 ms. Home idle RSS was 3,432 KiB (about 3.35 MiB), with unchanged CPU counters through the idle samples. The guest does not prove smooth behavior on a real serial link, a slow board, a large process population, I/O congestion or an SSH session.

The new status view reports available memory, root storage, CPU/memory/I/O waiting pressure and failed system services, explicitly marking unavailable checks. A real deliberately failed systemd service was detected through the status CLI under the ordinary guest user. This is condition reporting and threshold advice, not prediction, hardware certification or autonomous balancing.

Original benchmark artifacts are archived under `target/server-benchmark/baseline-evidence/`. Improved artifacts are under `target/console-qualification/guest-benchmark/`, including individual timing samples; observer data and the failed-service report are `target/server-benchmark/improved-samples.txt` and `improved-status.txt`. Both builds are package tests on the older native image; fresh candidate qualification remains separate.

## Usability and audiences

| Audience | Useful today | Missing proof or workflow |
| --- | --- | --- |
| New homelab operator | Guided enrollment, inventory, resources, local shell and SSH foundations | End to end service setup, network repair, backup/restore, trustworthy status, beginner task testing |
| Experienced server administrator | Standard Linux tools, services/log inspection, authorized task signals | Automation interface consistency, structured output, remote recovery, documented updates and lifecycle |
| Company | PAM/sudo authorization and local accounts | Fleet enrollment, role management, audit retention, patch/support policy, tested restores and production incident process |
| Research/teaching lab | Offline local ownership, disposable VM use | Reproducible provisioning, safe reset, quota/isolation, multiuser workload fairness, shared dataset recovery |
| Storage or virtualization homelab | Linux compatibility foundation only | No integrated NAS storage/snapshot management or hypervisor/cluster management qualification |

No human usability study was performed. Terminal automation proves tested behavior, not that a novice understands it. Accessibility and actual SSH/small-terminal variations need dedicated evaluation. Existing full-screen applications appropriately use the native terminal.

## Hardware and awareness

`system/src/console/hardware.rs` reads DMI, device tree, procfs and two battery paths. Its conservative classification avoids calling unknown hardware supported. The separate `detect` executable duplicates some discovery and reports device-tree models but not ordinary DMI PC models. Neither probe certifies drivers, board firmware or power behavior.

Resources measure CPU counter deltas, available memory, swap, root capacity and task identity. These are observations. There is no retained trend model, validated failure prediction, thermal policy, SMART/NVMe health integration, UPS management or workload aware balancing. Showing total host resources can also mislead in a constrained container unless cgroup limits are accounted for.

Hardware profiles establish AMD64 UEFI QEMU and ARM64 QEMU virt paths. Orange Pi 5 Plus is explicitly unverified. Raspberry Pi is a target family, not a tested board matrix. Pi/Orange Pi require exact board boot firmware, device tree, storage, NIC and driver qualification. A generic ARM64 boot is not that evidence. The owner requests PCs, servers and those board families, so physical support remains an explicit release gate, not a silent VM-only scope change.

Predictive awareness should begin with explained preflight and observable advice. It should not silently kill tasks, change resource policy or infer safety from a percentage. Missing or stale readings must remain unknown. A static warning is not a forecast; disk exhaustion forecasting requires retained samples and validated uncertainty.

## Code structure

The one Cargo package with explicit binaries is a reasonable small-system structure. Console privilege is ordinary user privilege; kernel authorization, sudo, setup/login and recovery retain distinct responsibilities. `query.rs` is a useful deep module: callers receive bounded, cancellable subprocess results without reimplementing timeout and process cleanup. Stable pidfds and protected ancestors are strong task-control decisions.

The largest concentration is `console/tui.rs` (2,049 lines at baseline). It combines input, routing, shell lifecycle, authorization presentation, queries and rendering. Extract around a real shared responsibility when a change needs it, rather than creating layers for every file. `monitor.sample()` scans and sorts all visible tasks synchronously, even for Resources. Rendering rereads hostname, uptime and memory. Assets enumerates/stat calls synchronously and without a directory entry bound. Networked/large directories and large process counts need qualification. Assets expansion remains outside this scope.

The CLI and Home differ: hardware/resources/tasks require the interactive view, while status/network/services are scriptable. Documentation still contains older kernel, test and prompt claims. Current handoff and qualified artifacts outrank those claims. Package version `1.0.0` is reused for development rebuilds, so hashes and build identity are essential. Release provenance and package dependency metadata are stronger evidence than the version alone.

## Priorities and gates

1. Fix demonstrated CI infrastructure failures, then inspect actual scanner results without suppressing advisories. Old OS scans never reached Trivy because tar could not create device nodes. Native AMD64 enrollment, UEFI and managed update boots passed; Recovery assembly failed because namespace users could not traverse GitHub's private temporary directory.
2. Improve measured interaction delays and honest health reporting using existing Linux facts. Preserve explicit permissions, shell compatibility and the established visual design. Record before/after results and limits.
3. Rebuild a fresh native image with a unique identity. Qualify enrollment, real SSH, reboot persistence, recovery, updates and sustained representative workloads against it. Application package tests cannot replace this.
4. Physical beta support requires an exact device matrix and actual device evidence. Public physical installation stays disabled until transactional install, recovery and data safety are established. Production signing keys, update migration and power-loss behavior remain gates where claimed.
5. Publish Server beta only against the documented supported matrix and explicit known limitations. Do not equate all-green CI with a production-ready OS. Desktop and broader Assets/Store work remain deferred.
