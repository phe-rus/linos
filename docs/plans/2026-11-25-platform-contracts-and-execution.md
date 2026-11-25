# Linos platform review and staged execution plan

25 November 2026. Planning checkpoint, not an implementation specification or release approval.
Reviewed source: `c1d0c0607a3f63935928379f18ffca90771c6f82`.

## Resume after clearing the conversation

Read this document, `docs/architecture/current-handoff.md`, and `git status --short --branch`.
The owner requested review, planning and staging before clearing the conversation. No platform
implementation, kernel compilation, image assembly, VM boot or workflow dispatch belongs to
this checkpoint. Only the planning documents are staged. No commit, push or publication.

First implementation slice: establish candidate identity, artifact reuse and bounded CI/local
execution (stage 1 below). Then resolve the storage and rollback contract before changing the
layout. Preserve the existing Server behavior and development rollback evidence throughout.

The owner currently has no physical test machine. Continue design and VM development without
repeatedly asking for one. Physical qualification and the previously requested physical beta
scope remain open; VM success does not close them. Server remains the first implemented edition.

## Direction established in the conversation

Debian remains a software source. Linos should own platform organization, behavior and lifecycle.
Do not change the distribution base as a shortcut. A custom kernel alone does not establish smoothness.

The owner favors an editable boot environment inspired by OpenCore: ordinary users obtain
prebuilt components, copy compatible device extensions into an organized EFI, and reboot.
They should not rebuild the OS to adapt device support. The installer carries the working device
configuration to the internal disk. The mounted internal EFI remains understandable and editable.

```text
USB_ROOT/
├── EFI/
└── recovery.pherus.org/

INSTALLED_DISK/
├── EFI                         boot partition, normally hidden
├── Recovery                    independent recovery partition, normally hidden
└── Linos                       OS partition
    ├── System                  managed operating system
    ├── Applications            separately managed applications
    ├── Users                   personal files and user settings
    ├── State                   device configuration and persistent service data
    └── Snapshots               hidden previous system states
```

The installed partition is named `Recovery`, not `recovery.pherus.org`, and there is no nested
`Recovery/recovery.pherus.org` requirement. One recovery environment handles installation,
repair and rollback. Rollback material can reside in OS storage; Recovery orchestrates it.
Do not introduce a separate visible Installer volume or expose System A/B as extra top level
partitions in the target design. Existing A/B machinery remains the working baseline until a
replacement is specified and qualified.

The OS tree describes logical ownership boundaries, not a promise of literal Linux root paths.
Preserve ordinary Linux interfaces through an explicit mount/path mapping. The filesystem,
encryption, internal deployment mechanism and exact mapping are not selected yet.

`Boot.efi`, `config.toml` and `.ldrv` were illustrative recommendations, not implemented or final
format decisions. Keep preboot `.efi` drivers distinct from Linux OS driver modules and firmware.
Do not infer a need to write a new bootloader merely to obtain an organized EFI.

## Review findings

| Finding | Source evidence | Planning consequence |
| --- | --- | --- |
| Current basic UEFI media has ESP plus ext4 root | `build/media.sh` creates two partitions and uses systemd-boot, `/linos/{kernel,initramfs}`, and `/loader/entries` | The new tree is a target, not current behavior |
| Installed layout proposal has five partitions | `system/src/recovery/layout.rs`, `docs/architecture/installation-layout.md` | Three partition design needs a migration decision, not cosmetic labels |
| Update health is coupled to exact storage | `system/src/recovery/update/health.rs` validates ext4 System A/B roots, separate persistent Data, vfat ESP and partition UUIDs | Preserve authentication, boot counting and fallback while redesigning storage |
| Extension loading is absent | Existing image/kernel packaging places modules in `/usr/lib/modules`; media builder has no bundle discovery path | Specify early and late loading, compatibility, trust and failure recovery before adding folders |
| Resource awareness exists; resource policy is incomplete | `system/src/console/health.rs`, `query.rs`, `monitor.rs` provide pressure reporting and bounded queries | Extend measured behavior; do not call reporting prediction or automatic optimization |
| Assets does not modify files | `system/src/console/assets.rs` is a read only browser | Trash needs a real file lifecycle service before a delete UI |
| Kernel compile and qualification are already separated | `.github/workflows/kernel*.yml` | Reuse exact kernel artifacts for userspace changes |
| Full compatibility CI rebuilds Server in a downstream media job | `.github/workflows/ci.yml`, `build-and-boot` and `installer-media` | Design artifact handoff so candidate identity does not change between jobs |
| OS artifacts are not currently a complete distribution channel | CI primarily uploads diagnostics; kernel build uploads packages with 14 day retention | Add candidate image retention/download and deliberate release promotion |

Live GitHub review on 25 November, repository `phe-rus/linos`:

* [Native integration run 45479141286](https://github.com/phe-rus/linos/actions/runs/45479141286)
  completed at `3996db4`; AMD64 and ARM64 failed the final OS vulnerability enforcement step.
* [Compatibility run 45479117710](https://github.com/phe-rus/linos/actions/runs/45479117710)
  completed at `3996db4`; both architectures failed the vulnerability enforcement step.
  Downstream Recovery and installer media jobs were skipped.
* Repository security run `45479110416` succeeded. This is not an OS security pass.
* `gh release list` and the releases API returned no releases. CI build artifacts do exist:
  run `44213747067` has unexpired AMD64/ARM64 kernel artifacts at review time. This does not
  establish permanent availability or a published OS release.

Kernel and OS builds belong in CI, with distributable releases hosted on the GitHub repository.
That is the owner's execution direction, not proof that release assets are currently published.
No CI runs were dispatched for this review. Prior test counts and timings in the existing handoff
remain historical evidence; no application tests or benchmarks were rerun for these doc changes.

## Build, testing and publication allocation

| Change or activity | Local responsibility | CI responsibility |
| --- | --- | --- |
| Documentation and planning | Review source, links, diff and consistency | No expensive image or kernel run |
| Rust userspace change | Format, relevant tests, bounded compilation as needed | Integration check using the existing qualified kernel; wider checks at the slice gate |
| Kernel config, overlay or source change | Review policy and lightweight kernel tooling tests | Compile once per architecture, then boot and integrate those exact packages |
| Boot, installer, storage or updater change | Relevant unit tests and disposable file based checks | Build one candidate per architecture and boot, recover and update that candidate |
| Resource behavior change | Targeted profiling or one disposable VM reproduction when needed | Reproducible pressure/workload scenarios, record latency and memory evidence |
| Candidate acceptance | One focused interactive VM session if it adds evidence beyond CI | Complete candidate specific security, boot, persistence and recovery gates |
| Release | Review candidate identity and qualification record | Promote the qualified bytes, publish manifests/checksums/provenance without rebuilding |

Rules for both sides:

1. Record source SHA, architecture, build ID, kernel release/package digest and artifact digests.
   A mutable filename or version alone is insufficient. Package inventory and source provenance
   travel with each OS candidate. Scan identity includes scanner and advisory database versions.
2. Local full Linux/kernel builds and full OS image assembly are not the normal loop. Download
   verified CI artifacts for local boots. Small userspace builds remain available for focused debugging;
   a modified guest is explicitly development evidence, never final candidate qualification.
3. Inspect available RAM, disk and active processes before local work. Start compilation with
   `CARGO_BUILD_JOBS=1`; permit more only with measured headroom. One local VM at a time,
   initially one vCPU and 1024 MiB when the scenario supports it. Adjust deliberately for the test.
4. Large images and scratch files live on disk backed workspace storage or verified `/var/tmp`,
   never assumed disk backed `/tmp`. Python is prohibited, including temporary automation.
5. Bound CI jobs and VM resources explicitly. Parallel architectures on separate runners are
   acceptable within budget; do not stack memory hungry VM scenarios on one runner. Avoid
   concurrent full native and compatibility suites unless they answer different outstanding questions.
6. Run focused checks while editing, then the full relevant suite once per coherent slice. Repeat
   only after changed inputs, a failure, or unresolved uncertainty. Keep logs for negative cases.
7. Treat artifact reuse as a provenance decision. Check source, ABI/config, architecture, hash and
   successful qualification. An expired artifact is not permission to use an arbitrary replacement.
8. Existing markers: `kernel:validate` is lightweight; `kernel:build` compiles without publication;
   `kernel:integration` or manual kernel workflow dispatch reuses the pinned package run;
   `kernel:release` requests publication. Never use the release marker merely to obtain tests.
9. Candidate qualification and public release promotion remain separate. A failed security gate
   can still yield useful development evidence, but cannot yield a qualified release.

## Stages and review gates

### Stage 0: planning checkpoint, completed in this turn

Review code/workflows and live run outcomes. Preserve owner intent, unresolved decisions and
this execution policy. Stage only documentation for review. Do not start implementation.

### Stage 1: candidate provenance and execution routing

25 November execution review: the development artifact/execution gate passed at
`717b450`, native run `45553431687`, with downstream media boot and verified local
AMD64 download/smoke evidence. The overall run failed unchanged OS vulnerability
gates on both architectures. See [the candidate execution record](../architecture/candidate-artifacts.md)
for exact identities, tests and boundaries. This is not release qualification.

Define and then implement artifact handoff across kernel, OS, Recovery, tests and local download.
Replace the implicit hardcoded reuse decision with an explicit verified candidate/kernel identity.
Determine which artifacts are retained for development and which are promoted as releases.
Preserve manual expensive CI gates and set appropriate compiler/VM concurrency caps.

Exit review: a userspace change demonstrably reuses a compatible kernel; downstream media
tests consume the same OS candidate without rebuilding it; a local VM can consume verified CI
bytes. Failed jobs retain usable diagnostics. No release happens as a side effect of qualification.

### Stage 2: storage, application ownership and recovery contract

25 November design checkpoint: [spec 0005](../specs/0005-storage-recovery-contract/index.md)
records the owner selected ext4 image deployment, current data preservation,
managed APT, optional encryption boundary and ownership model. The independent
review findings and revision alignment issues are closed in the written contracts.
The owner accepted the assembled specification and authorized proceeding on
25 November. Stage 2 design is complete. No new image behavior is established by
this design review; Stage 3 implementation and qualification remain separate.

Write a buildable spec before changing disk code. Decide filesystem and encryption boundaries,
Linux path mappings, System/Applications package ownership, direct APT behavior, snapshot
retention and disk full behavior. Define account/configuration/database migrations, application
compatibility on rollback, and boot selection of a complete kernel/system/extension combination.
Define independent Recovery access, offline installation and USB to internal EFI transfer.

Evaluate the current A/B implementation against snapshot/deployment alternatives. Do not pick
Btrfs just because it supports snapshots or remove validated A/B behavior before its replacement
has equivalent failure and recovery evidence. Existing spec `0004` security policy remains a
separate unresolved release decision, not automatic authorization to maintain package backports.

Exit review: every persistent item has an owner, path, update rule and rollback rule; a deployment
state machine and failure matrix cover interrupted writes, incompatible data and missing boot assets.

### Stage 3: storage and unified Recovery implementation

25 November first slice: the pure layout 2 geometry planner and explicit numeric
Recovery preview are implemented, with boundary tests and disposable GPT file
verification. Existing layout 1 remains active. No layout 2 installation, filesystems
or boot has been implemented/qualified; see the current handoff for source/checks.

Implement the agreed three partition design on disposable virtual disks. Adapt layout validation,
updater, health approval, fallback and offline payload handling together. Recovery provides install,
repair and rollback from one environment. Preserve Linux paths, credentials and user data per spec.

Exit review: CI proves USB boot, virtual installation, USB removal/internal boot, independent
Recovery, successful update, failed update rollback, interruption recovery and data preservation.
Physical installation stays disabled until separately qualified; no host block device writes.

### Stage 4: editable EFI and device extensions

Specify bundle name/format, matching kernel ABI, dependencies, firmware provenance/licensing,
signatures and owner trust, discovery/enable/disable rules, early storage driver loading, late
network driver loading, update compatibility and known working extension selection. Reuse the
existing bootloader where sufficient. Then implement discovery and activation, not empty folders.

Exit review: add/replace/disable a prebuilt bundle on USB and internal EFI without rebuilding
the OS. Wrong architecture/kernel, tampering, duplicates and failed boot have useful outcomes.
An OS rollback selects compatible extensions. A recovery boot can bypass owner additions.
VM simulated devices prove the mechanism, not physical WiFi coverage.

### Stage 5: measured resource management and behavioral contract

Specify Server workload priorities, sampling cost, pressure thresholds/hysteresis, operator control
and safe behavior when measurements are unavailable. Start with observation and explicit bounded
policy using existing Linux/systemd/cgroup interfaces. Do not silently stop services or predict failures
from a single metric. Measure input latency, query completion, idle CPU/RSS and loaded behavior.

Document shared interaction principles: immediate acknowledgement, honest progress, cancellation,
reversibility, consistent focus/input and clear degraded states. Graphical motion, gestures, haptics
and sound are future device specific work, not deliverables of the Server slice or kernel flags.

Exit review: representative workloads retain correctness under pressure, Home remains responsive,
the policy can be inspected/reversed, and idle overhead and regressions are reported honestly.

### Stage 6: file lifecycle and Trash

Specify per user/per filesystem trash, restore metadata and collisions, removable media, symlinks,
permissions, cross filesystem moves, interruption/disk full cases, retention and explicit permanent
deletion. Preserve compatibility with existing Linux trash conventions where applicable. Separate
Trash semantics from system snapshots and backups; do not claim secure erasure from emptying it.
Define application removal versus data removal. Implement a safe service before destructive UI.

Exit review: deletion/restore are exercised on disposable filesystems, including failure cases.
Normal file trash never moves OS components or active service data without a deliberate policy.
Assets expansion and a graphical UI are separately scoped after the Server foundation gate.

### Stage 7: final candidate and release review

Build in CI, qualify one immutable candidate, and reuse it for the focused local acceptance boot.
Security gate resolution, signing/trust lifecycle, recovery instructions and supported target matrix
must be explicit. No universal hardware, smoothness or production reliability claims from VM tests.
No physical beta claim while machines are unavailable. Reconsider release scope explicitly if needed.

## Stage review checklist

At each stage record changed contracts, exact source/artifacts, checks run, positive and negative
results, remaining risks and the next action. Recheck the plan against evidence before moving on.
Do not combine a filesystem migration, driver loader and resource scheduler into one change.
Do not turn this coarse plan into permission to guess unresolved implementation decisions.

Next session starts at stage 1, with stage 2 design preparation where independent. Security
maintenance can be investigated alongside design, but its proposed policy is not settled by this plan.
