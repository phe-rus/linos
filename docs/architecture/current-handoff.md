# Linos current handoff

Updated 25 November 2026. Start here and check the exact CI runs before making a release claim. This supersedes the 23 November handoff.

## Planning checkpoint for the next conversation

Read [the platform review and staged execution plan](../plans/2026-11-25-platform-contracts-and-execution.md)
before implementation. The owner requested review, planning and staging, then a conversation clear.
The agreed direction is USB `EFI/` plus `recovery.pherus.org/`, and installed `EFI`, `Recovery`,
and `Linos` partitions. Linos has logical System, Applications, Users, State and Snapshots roles.
These are target contracts, not implemented layout changes. Filesystem, extension format and
rollback/data mappings still need design. Recovery owns install, repair and rollback.

Kernel and full OS builds belong in CI; reuse verified artifacts for local VM acceptance.
Use focused bounded local tests and sequential VMs, and do not duplicate expensive CI work.
Publication must promote qualified bytes separately from test runs. No builds, boots, workflow
dispatch, platform implementation or publication were performed for this planning checkpoint.

The owner currently has no physical test machine. Continue VM development while retaining the
physical qualification boundary. Start next with artifact provenance/reuse and execution routing,
then the storage/recovery contract, following the staged plan. Server remains the first edition.

## Stage 1 implementation checkpoint

Stage 1 is in progress. See [candidate artifacts and execution](candidate-artifacts.md)
for the implemented identity, archive, download and retention contract, exact local
evidence, and remaining CI gate. Kernel selection now uses `kernel/candidate.conf`
or an explicit manual run/source pair, with successful boot and artifact digest
verification. Server candidates are captured before tests. Installer media consumes
the captured OS archive instead of rebuilding Server. Compiler and VM caps are explicit.

Local archive/API rejection tests, shell syntax, Actionlint, six kernel tooling tests
and 42 library/Recovery tests pass. A real AMD64 kernel download passed artifact and
package checksums and the existing provenance verifier against this checkout.
The changed workflows still need a committed source and CI execution, followed by
native candidate download/VM evidence. No full OS or kernel build, VM boot, workflow
dispatch, commit, push or release was performed. The earlier planning documents
remain staged; Stage 1 changes are unstaged. Storage and security policy are unchanged.

## Owner direction and sequence

Server only. First assess Linos against its product promise, Apple's published quality principles and established server platforms, then improve the demonstrated gaps. Run security and image qualification in CI, fix and rerun. Only after those gates pass, qualify a fresh native image locally, complete the beta support/recovery gates and publish an explicitly supported Server beta. Desktop and broader Assets/Store work are deferred until afterward.

The owner wants physical PCs, homelab machines, servers, Raspberry Pi and Orange Pi families as well as VMs. No exact physical test configuration has been provided or qualified. Do not silently narrow the requested beta to VM only or call an arbitrary ARM board supported because ARM64 QEMU boots.

## Assessment and implementation

- [Measured Server baseline and gaps](../reviews/2026-11-24-server-baseline.md): footprint, interaction timing, architecture, hardware awareness and audience suitability.
- [Primary source platform comparison](../reviews/2026-11-24-server-platform-comparison.md): Apple principles and Debian/Ubuntu/RHEL/Proxmox/TrueNAS reference points. This is not a measured competitor performance ranking.
- [Support matrix](server-support-matrix.md): exact qualification limits and required physical evidence.
- [Status and response spec](../specs/0003-server-status-response.md): bounded input batching, faster polling only while work is pending, and shared CLI/Home status checks.

The implementation preserves the existing visual direction, shell compatibility and authorization. Status now reports memory/root headroom, CPU/memory/I/O pressure and failed services, marking unavailable checks. A neutral Home greeting replaces the unconditional readiness claim. Advice does not mutate resources or claim failure prediction. Hardware identification remains distinct from qualification.

One 1 vCPU, 1024 MiB native AMD64 guest baseline showed Home idle RSS about 3.2 MiB and no idle Home CPU-counter change over the sample window. Installed root use was about 290 MiB; the sparse base image is logically 3 GiB. The improved package used about 3.35 MiB idle RSS. In the two observed runs, loaded Services navigation tail fell from 5.4 seconds to 217 ms, and idle simple-command median from 254 to 108 ms. Typing tail did not improve and single startup was slower. These are small serial-terminal samples, not physical display, minimum-RAM, production or comparative OS guarantees.

## Local evidence

Application checkpoint `d54889c12421cea7f0c312687b7238b6eac3e400` is committed and pushed. CI repair checkpoint is `307defdc4abcbeefc2d98683fcbf18e4720903ce`.

- Workspace: 135 passing Rust tests, four explicit guest/benchmark cases ignored in the ordinary run.
- Formatting, strict all-target Clippy and shell syntax checks pass.
- The rebuilt application passed the full Home interaction, ordinary administrator authorization and representative workload guest tests, each against a fresh disposable copy of native `20261123.4`.
- A separate repeat benchmark detected a deliberately failed systemd service through the new status CLI running as an ordinary user.
- All local VMs ran sequentially with one vCPU, 1024 MiB and guest loopback/no external network. Copies remain under disk-backed `target/`. Existing user VMs were preserved. No Python was used.

Artifacts: `target/server-benchmark/`, `target/console-qualification/guest-benchmark/`, and the three guest snapshot directories. Original benchmark snapshots are archived at `target/server-benchmark/baseline-evidence/`. The original timing data has summary distributions; the improved run also retains individual timing samples. `host-manifest.txt` records host/tool details after the runs. Unexposed host cgroup limits were not established.

The development package was rebuilt after selecting new build identity `20261124.1`, but a fresh native candidate image has not been assembled or qualified. Keep the owner's CI-first ordering.

| Prepared artifact | SHA-256 |
| --- | --- |
| `target/release/linos` | `b10fc363fc116bee513361106f512ddaf9156a2342362f840c8788729ec1ff6a` |
| `system/dist/linos13_dev_v1.0.0.deb` | `47097fa4486036c077dfe6373b90cac7d8b9ff11e67842d625e0101ff0d8ba8d` |

These are local development artifacts, not immutable release certification. Package version remains development 1.0.0, so use the build identity and hashes.

## CI repair and security blocker

The old full scan failed while tar attempted unprivileged device-node extraction; Trivy had not run. Extraction now excludes virtual runtime trees while preserving OS/package metadata. Native AMD64 qualification had passed enrollment, firmware boot and six update/fallback boots, then failed Recovery assembly because an unshared user could not traverse GitHub's private temporary directory. Its temporary storage now uses disk-backed `/var/tmp`.

| Run | Verified result |
| --- | --- |
| [45476639614](https://github.com/phe-rus/linos/actions/runs/45476639614), native installed kernel at `307defd` | Both AMD64 and ARM64 passed, including AMD64 Recovery |
| [45477917782](https://github.com/phe-rus/linos/actions/runs/45477917782), repository security at `d54889c` | Passed |
| [45476628685](https://github.com/phe-rus/linos/actions/runs/45476628685), full compatibility image CI at `307defd` | Both architectures completed functional checks and failed the OS vulnerability gate; downstream Recovery/installer media jobs were skipped |
| [45479117710](https://github.com/phe-rus/linos/actions/runs/45479117710), final full compatibility image CI at `3996db4` | Refreshed 25 November: both architectures failed final OS vulnerability enforcement; downstream Recovery/installer media jobs skipped |
| [45479141286](https://github.com/phe-rus/linos/actions/runs/45479141286), final native image and package-security qualification at `3996db4` | Refreshed 25 November: both architectures failed final native OS vulnerability enforcement |

The repaired AMD64 scan produced 85 package/CVE matches, 82 HIGH and three CRITICAL rows, covering 26 unique identifiers. All lack a fixed stable package version in the report. The three CRITICAL rows are the same OpenSSH source advisory, not three independent remotely exploitable server flaws. Exact binary reachability needs review. The image already includes stable updates and security repositories; this is not solved by adding the missing security repository.

Raw report: `target/server-benchmark/ci-security-amd64/os-vulnerabilities.json`, SHA-256 `35ef43f1902364710782ddfee358e9ea4059d7d4c5afea3d3073a521d1243668`. `security-findings.psv` holds the per-row summary. Use [the vendor triage](../reviews/2026-11-24-server-security-triage.md) and [the proposed maintenance decision](../specs/0004-server-security-maintenance.md). Normal stable refresh cannot currently clear this strict gate. No advisory was suppressed, no ignore-unfixed option was added and no base distribution was changed.

Native-kernel image qualification now includes the same OS package scan and retained reports, since full CI scans explicit compatibility images. The final run reached and failed the enforcement step on both architectures. It does not establish advisory coverage of the custom kernel itself. Functional tests still run after vulnerability findings, then the gate fails.

## Remaining work

1. Settle security maintenance: reviewed reproducible backports and candidate-specific applicability evidence, or wait for stable vendor fixes. The maintenance spec is Proposed. Do not silently start owning a new package supply chain or relax the gate.
2. Rerun CI at the final source and triage every finding/error. The qualification path must never use the `kernel:release` publication marker just to run tests.
3. After the security gates are satisfied, build fresh native `20261124.1` with its new package/dependency set. Qualify actual enrollment/PAM login, SSH and noninteractive SSH behavior, reboot persistence, Recovery and signed development update/data preservation against that exact candidate. Existing mutable package tests cannot replace it.
4. Qualify actual named physical configurations and remaining installer/data/recovery guarantees. Physical installation remains disabled. Production signing, account/service-data migration, power-loss testing, sustained workloads and support lifecycle are not established by this turn.
5. Release beta only with a supported matrix and explicit candidate evidence. Desktop and broader Assets/Store work remain outside this scope.

The owner confirmed no physical test machine is currently available. The security-maintenance path remains unresolved. A target family is not an available device. Neither elapsed time nor no response grants a new maintenance-policy decision. The new platform plan governs future architectural work; the release/security gates above remain open.
