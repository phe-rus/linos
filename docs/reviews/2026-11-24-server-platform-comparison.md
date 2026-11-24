# Linos Server: platform and product-quality comparison

Date: 24 November 2026. Scope: Server only. This is a source-based comparison and an assessment framework, not a cross-platform performance benchmark. Linos implementation evidence comes from the [23 November handoff](../architecture/current-handoff.md); the separate local assessment must supply current source findings and measurements. The handoff supersedes older implementation-status prose in `PRODUCT.md`.

## Finding

Linos has a credible purpose: a coherent, locally owned Server experience that helps a non-specialist administer a machine while preserving ordinary Linux tools. The demonstrated Home, PAM, sudo, shell and workload paths are useful foundations. They do not yet establish the reliability, recoverability, workload breadth, hardware support or maintainability needed to displace established server platforms.

The opportunity is to make routine administration understandable and safe. The largest risk is presenting a calm interface over operations whose consequences, support limits and recovery procedures remain unclear. No overall percentage, ranking or numerical quality score is defensible from the available evidence.

## What comparison with Apple means

Apple discontinued macOS Server on 21 April 2022. Comparing Linos with a hypothetical modern Apple server cannot establish which product is faster or more capable. Apple's design and security documentation provide a quality reference; the server-specific recommendations below are our inferences, not statements of Apple's plans. [Apple: macOS Server changes](https://support.apple.com/en-us/101601).

Apple's published design principles emphasize usefulness, user control, safety, familiar behavior, accessibility, clear hierarchy and iteration in real use. For Server, apply those principles to a person trying to keep a service working, understand an incident or recover their data. A decorative terminal or fewer visible controls does not prove simplicity. [Apple: design principles](https://developer.apple.com/design/human-interface-guidelines/design-principles).

Apple's design discussion explicitly separates simplicity from minimalism: hiding all functions together can make them harder to find. Context can make an interface simpler. This supports a Home that explains an important state and its next action rather than only displaying low-level counts. [Apple: principles of great design](https://developer.apple.com/videos/play/wwdc2026/250/).

Apple's signed system volume verifies system integrity and uses snapshots to allow restoration after an unsuccessful update. This is a useful comparison for Linos's eventual system/data separation, authenticated artifacts and recovery promises; Linos payload signatures or update-state tests alone are not equivalent. The implementation need not copy APFS or Apple's hardware trust architecture. [Apple: signed system volume security](https://support.apple.com/guide/security/signed-system-volume-security-secd698747c9/web).

### What an Apple-informed approach would improve

These are recommendations derived from the references and Linos's stated purpose:

| Quality | Concrete Server behavior | Evidence required before claiming it |
| --- | --- | --- |
| Purpose | Choose a supported first beta audience and a short set of complete administration journeys | Fresh-machine task completion by intended users, without developer coaching |
| Agency | Make operation state, cancellation, authorization and recovery consistent; retain an ordinary shell | Exercise interruption, permission denial, retry and return to Home |
| Clarity | Distinguish healthy, degraded, unsupported, unavailable and unknown; say what changed and what to do | Disconnected network, missing telemetry, failed service, full disk and denied inspection cases |
| Craft | Keep typing, navigation and status refresh responsive during work | Input-to-visible-response distributions at idle and under specified CPU, memory and I/O load |
| Responsibility | Explain impact before stopping a process, rebooting, installing or changing connectivity | Preflight, deliberate confirmation, preserved evidence and tested recovery |
| Familiarity | Use one navigation, focus, action and error vocabulary across views | Keyboard-only journeys, small terminals, resize, interrupted operations and a new-user evaluation |
| Whole-system care | Couple updates and installation to data preservation and recovery | Candidate-specific enrollment, SSH, reboot, rollback, interrupted-update and restore tests |

An Apple-informed priority would be fewer fully supported journeys with explicit guarantees, followed by expansion. It would not be a claim that every Linux package is integrated or that every detected device is supported.

## Established server reference points

These comparisons identify capabilities to learn from, not a requirement to implement every competing product before a narrowly scoped beta.

| Reference | Documented strength | Implication for Linos |
| --- | --- | --- |
| Debian 13 | Published architecture support, release lifecycle and upgrade documentation; a general-purpose foundation | Debian package availability is useful, but a Linos-owned kernel and release require their own support and regression policy. [Debian release information](https://www.debian.org/releases/trixie/) |
| Ubuntu Server | Published LTS maintenance periods and automatic security-update mechanisms | State who delivers security fixes, what is covered, and how a user learns about a required restart or failed update. [Ubuntu lifecycle](https://documentation.ubuntu.com/release-notes/26.04/), [security updates](https://documentation.ubuntu.com/security/security-updates/) |
| RHEL | Cockpit management for services, storage, networks, logs and accounts; system roles for repeatable multi-machine configuration | A friendly local console is only part of company readiness. Shared configuration, identity, policy, diagnostics and remote operations need explicit support. [RHEL web console](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/10/html/managing_systems_in_the_rhel_web_console/index), [RHEL system roles](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/10/html/automating_system_administration_by_using_rhel_system_roles/introduction-to-rhel-system-roles) |
| Proxmox VE | Integrated virtualization administration, delegated permissions, backups and high-availability workflows | Being a usable guest OS does not make Linos a virtualization-management host. Lab or company claims must distinguish those roles. [Proxmox administration guide](https://pve.proxmox.com/pve-docs/pve-admin-guide.pdf) |
| TrueNAS | Storage-focused hardware guidance, drive-health alerts with next steps, scheduled snapshots and replication | Hardware awareness is actionable condition reporting and data protection, beyond device enumeration. Linos should not claim NAS reliability from a file-integrity workload. [hardware guide](https://www.truenas.com/docs/scale/gettingstarted/tnhardwareguide/), [drive health](https://www.truenas.com/docs/scale/storage/disks/drivehealthmanagement/), [snapshots](https://www.truenas.com/docs/scale/dataprotection/periodicsnapshottasks/) |

The comparison set is deliberately representative. No review can establish superiority over “all other servers,” and specialist NAS, virtualization and general-purpose operating systems solve different jobs.

## Suitability by user and job

The assessment below is based on the latest Linos handoff, not a certification of workloads not yet run.

| Audience | What the current evidence supports | What must be established for a credible recommendation |
| --- | --- | --- |
| Homelab learner | Disposable VM experimentation, local ownership, Home exploration and ordinary Linux commands | Clear first service/network/SSH journey, safe update and restore, practical documentation, useful errors and a supported host/guest configuration |
| Experienced single-server administrator | A development environment with sudo separation, process controls and basic service workload evidence | Fresh-candidate SSH, storage and network diagnosis, package/security policy, persistent configuration and recovery under failure |
| Small company | Evaluation in an isolated noncritical environment | Backups with restoration proof, access policy, security maintenance, remote administration, audit trail, documented responsibility and service recovery |
| Shared teaching or research lab | Disposable isolated guests for exploration | Repeatable provisioning/reset, separate users and quotas, reproducible environments, network isolation, delegated administration and dataset preservation |
| Production or regulated company | No readiness claim established | Threat model, supported hardware, update/incident procedures, verified recovery, workload certification, lifecycle commitments and any applicable compliance evidence |

A meaningful first beta can target an explicitly supported VM configuration and noncritical single-machine experimentation. It must say so plainly. Physical install, storage appliance, clustering or company-critical claims require their own gates. A narrower beta does not prove the broader roadmap complete.

## Hardware awareness and predictive behavior

Evaluate awareness in layers:

1. **Detection:** accurately identify what is present and how trustworthy the information is.
2. **Condition:** identify pressure, device errors, thermal or power constraints and degraded services.
3. **Context:** connect the condition to a workload and describe the effect on a person.
4. **Advice:** offer an explained, evidence-backed next step with uncertainty and scope.
5. **Adaptation:** alter policy only within an explicit, tested contract, preserving user control.

Linos's handoff establishes Linux-reported inventory and resource readings; it does not establish all five layers. Current source and real-device tests must determine the rest. A name read from sysfs is detection, not certification; a full CPU bar is utilization, not proof of a responsiveness problem.

TrueNAS illustrates useful condition and advice: drive-health alerts identify a disk, report the relevant condition and present next steps. It also warns that intensive drive tests affect performance. Its scheduled snapshots provide a distinct protection mechanism; health alerts do not replace backups. [Drive health management](https://www.truenas.com/docs/scale/storage/disks/drivehealthmanagement/), [snapshot tasks](https://www.truenas.com/docs/scale/dataprotection/periodicsnapshottasks/).

For Linos, early anticipatory behavior should be deterministic and transparent: warn about insufficient free space before staging an update; identify unsupported or stale telemetry; explain that a task pause can interrupt a service; report an action's actual outcome. Trend-based “time until full” needs retained samples, a stable observation window and explicit uncertainty. Do not call threshold warnings AI, or claim to predict hardware failure without validated evidence.

## RAM, disk and smoothness: avoid false comparisons

Official requirements are useful planning constraints, not benchmark results:

| Product | Published planning guidance | Interpretation |
| --- | --- | --- |
| Debian 13, no desktop | 512 MB minimum RAM, 1 GB recommended, 4 GB disk; minimum assumes swap and non-live/text installer | Workloads and user data need additional resources. [Debian installation guide](https://www.debian.org/releases/trixie/amd64/ch03s04.en.html) |
| Ubuntu Server 26.04 LTS | Starts at 1.5 GB RAM and 4 GB storage, depending on use | A release requirement, not measured idle consumption. [Ubuntu release notes](https://documentation.ubuntu.com/release-notes/26.04/) |
| Proxmox VE | At least 2 GB for host OS/services plus guest memory; storage choices need more | Host virtualization features are not equivalent to Linos guest features. [Proxmox administration guide](https://pve.proxmox.com/pve-docs/pve-admin-guide.pdf) |
| TrueNAS | At least 8 GB RAM for basic use; recommended boot volume at least 20 GB | Storage integrity, caching and workload configuration change resource demand. [TrueNAS hardware guide](https://www.truenas.com/docs/scale/gettingstarted/tnhardwareguide/) |
| Linos development workload | Existing tests used a 1 vCPU, 1024 MiB VM | This is a passed fixture size, not an established minimum, idle footprint or latency result. [Linos handoff](../architecture/current-handoff.md) |

Measure Linos's package size, logical image size, actually allocated host image size, installed filesystem use, idle guest memory, console RSS/PSS, idle CPU, boot-to-ready time and input latency separately. Use repeated runs with image identity, kernel, guest resources, host load, instrumentation and cache state recorded. Record median and tail behavior, failures and raw samples. Measure under CPU, memory and storage pressure, including the longest synchronous view queries.

Comparative speed or efficiency requires running equivalent workloads on equivalent hardware, with equivalent services and protection enabled. Without competitor runs, report Linos measurements and externally published requirements separately. Fewer features and missing safety services can reduce footprint without improving the product.

## Benchmark tasks and release priorities

Use concrete tasks rather than appearance ratings:

- A new user enrolls, finds the machine's address, connects using SSH, checks service health and returns to Home.
- A user understands and recovers from denied authorization, a missing dependency, failed inspection and a disconnected network.
- An administrator finds the cause of resource pressure without losing input responsiveness or exposing secrets.
- A user previews a consequential action, cancels safely, retries and receives an accurate outcome.
- A candidate updates or fails to update without losing identity, accounts or service data, and a recovery procedure restores a known working state.
- A second operator can follow the documentation and reproduce the result without developer knowledge.

Prioritize measurable correctness, accurate status, responsiveness and safe recovery before cosmetic refinement. Record each gap as one of: release-blocking defect, scope limitation that can be honestly documented for the chosen beta, or later enhancement. CI green, local functional tests, user usability evidence and physical-hardware evidence are separate gates.

Desktop, a broader Assets file manager and a transactional Store are outside this assessment's implementation scope. Their absence must be reflected honestly in the supported beta journeys rather than silently expanded into new feature work.
