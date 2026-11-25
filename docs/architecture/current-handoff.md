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

## Stage 1 execution gate passed, security gate remains open

Stage 1 artifact provenance and execution routing passed its development review.
See [candidate artifacts and execution](candidate-artifacts.md) for the implemented
contract, exact candidate identities and evidence. Kernel selection uses `kernel/candidate.conf`
or an explicit manual run/source pair, with successful boot and artifact digest
verification. Server candidates are captured before tests. Installer media consumes
the captured OS archive instead of rebuilding Server. Compiler and VM caps are explicit.

Local archive/API rejection tests, shell syntax, Actionlint, six kernel tooling tests
and 42 library/Recovery tests pass. A real AMD64 kernel download passed artifact and
package checksums and the existing provenance verifier against this checkout.
Implementation and planning documents are committed and pushed at
`717b45066d47c7509e1054f2b2d232e5aff5ceac`. Native run
[45553431687](https://github.com/phe-rus/linos/actions/runs/45553431687) completed:
both architectures passed functional checks and failed only final OS vulnerability
enforcement. AMD64 passed enrollment, firmware boot, six update/fallback boots and
independent Recovery. The separate downstream development job restored the original
AMD64 archive, built its offline payload and UEFI media without rebuilding Server,
and booted it to setup. The overall run remains failed. Kernel compilation and
publication were skipped. Repository security run
[45553413542](https://github.com/phe-rus/linos/actions/runs/45553413542) passed.

The AMD64 archive was downloaded using repository/run/source/API digest verification,
restored with all internal hashes checked, and smoke booted locally with one vCPU,
1024 MiB and no external network. This passed sign-in/first-boot identity checks.
Artifacts and logs are in `target/stage1-ci/`; restored bytes are under
`images/server/builds/amd64/20261124.1/`. This is development execution evidence,
not final security, interactive local enrollment or physical qualification.

Fresh scans retain the same 26 advisory IDs: AMD64 has 85 package matches (82 HIGH,
three CRITICAL), ARM64 83 (80 HIGH, three CRITICAL). No fixed package versions are
reported. The existing security maintenance decision remains unresolved; no gate
was relaxed. Stage 2 storage/recovery design is next, before any layout changes.

## Stage 2 accepted storage and Recovery contract

The [assembled Stage 2 specification](../specs/0005-storage-recovery-contract/index.md)
has owner design approval, with four implementation contracts and a source/alternatives
record. The owner explicitly selected ext4 deployment images, current data preservation
with incompatible rollback blocked, managed APT deployment transactions, optional
Linos volume encryption with the plain path first, and the five record ownership model.

The proposal specifies 1 GiB EFI, 4 GiB Recovery and the rest Linos on a 32 GiB
minimum development virtual disk, read only loop mounted system images, current
configuration generations, service schema checks, versioned signed records and
owner local package provenance. It defines USB Recovery without a third root folder,
offline image chunking, EFI transfer, retention/space reserves and an interruption
matrix. These details are design, not behavior implemented in the existing images.

The independent cross check completed and found six gaps: durable fallback/trial
authority, service registry consistency, keyed account/configuration merging,
Recovery/transport trust and installation ordering, owner packages across official
upgrades, and package files hidden by writable mounts. The owner approved all six
recommended fixes. The four child contracts and index include them. An independent
closure check identified five alignment issues, which were resolved and checked
again with no direct contradiction found among the revised rules. The rationale
retains the findings/resolutions. The owner accepted the assembled spec on
25 November 2026 and authorized proceeding. Stage 2 design is complete; the spec
status tracks the still unbuilt Stage 3 implementation.
No storage code, kernel input, disk image or VM was changed or
built for this design. The five partition development baseline stays active.
Security maintenance and physical qualification remain open. Begin Stage 3 only
from the reviewed specification and retain the current baseline until parity passes.

## Stage 3 first slice: layout 2 geometry

The accepted Stage 2 design is committed and pushed at `3b1ba72`.
Stage 3 implementation has begun with `system/src/recovery/layout/v2.rs` and the
read only `recovery plan-v2 DISK_BYTES SECTOR_BYTES` command. The planner computes
three aligned inclusive sector ranges for 1 GiB EFI, 4 GiB Recovery and the Linos
remainder, retaining 16 MiB for alignment/GPT at a 32 GiB development minimum.
Geometry supports 512/4096 byte logical sectors; neither proves installation support.
The old `plan /dev/DEVICE` and five partition calculation remain unchanged.

Nineteen Recovery Rust tests pass, including four new boundary/overflow/alignment
tests. Strict Recovery Clippy, formatting and shell syntax pass. The new
`tests/layout-v2.sh` creates a fresh sparse regular file under disk backed `target/`,
uses the Rust plan to write GPT with `sgdisk`, validates both GPT copies and all
three names/ranges/type codes, and rejects a device path as numeric plan input.
Evidence is retained at `target/layout-v2-tests/`; the sparse disk is removed.
Actionlint passes, and the lightweight `kernel:validate` workflow now runs this fixture.

Source checkpoint `eed46c2474fedd874c18e6b5452a364400dfde4d` is committed and pushed.
Lightweight validation run
[45563935326](https://github.com/phe-rus/linos/actions/runs/45563935326) passed the
artifact checks, disposable GPT fixture and 77 focused Rust tests. Kernel build,
boot, OS integration and publication jobs were skipped. Logs are retained locally
at `target/layout-v2-tests/ci-validation.log`. Repository security run
[45563935348](https://github.com/phe-rus/linos/actions/runs/45563935348) passed both
Rust advisories and repository scanning. This is separate from the existing failed
OS package security gate.

This is geometry evidence only. No filesystems, boot helper, layout 2 record
parser, installation or VM boot is implemented by this slice. No host block device
was written; no full kernel or OS image build ran locally. Build step 1 remains
partial: strict records/signatures and observed loop/bind ancestry are next.
Security, physical installation and release gates remain open.

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
