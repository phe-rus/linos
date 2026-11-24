# Linos Server OS vulnerability triage

Reviewed: 24 November 2026. Scope: the 26 unique CVEs reported in the AMD64 CI OS scan. This is triage, not a finding suppression, package patch or release approval. Primary vendor sources were checked live. Package availability and tracker statuses can change; refresh them before implementation and before publication.

## Decision

**An ordinary trixie package refresh cannot resolve this scan today.** All 26 entries still identify the current trixie source package as vulnerable, with no trixie fixed version in the Debian Security Tracker. Most have Debian `no-dsa`/minor treatment; this does not mean that the package is fixed or that Linos's strict gate passes. Two OpenSSL findings remain unfixed in Debian sid too, although upstream fixes or scheduled releases are referenced.

Keep the HIGH/CRITICAL gate unchanged. The reviewed next choices are:

1. Prepare a reproducible, reviewed Linos backport plan from the vendor patches, preserving the stable package ABI and package integration where possible. Build and test both architectures, retain provenance and regression evidence, and ensure scanner recognition reflects actual patch status rather than a fabricated version.
2. Wait for suitable stable fixes and rebuild/rescan when they arrive.

Do not switch to sid, change distribution, remove essential packages, suppress unfixed findings or create automatic exemptions merely to make CI green. A broad backport effort creates a maintenance responsibility and is not a routine package-refresh task. Source-wide applicability corrections, if eventually proven, require separate evidence and an explicit review; this document does not authorize them.

## Scan evidence and counting

- Workflow: [full image qualification run 45476628685](https://github.com/phe-rus/linos/actions/runs/45476628685). The parent investigation reports the AMD64 OS gate failing; source and runtime qualification are separate from that result.
- Report: `target/server-benchmark/ci-security-amd64/os-vulnerabilities.json`.
- Report SHA-256: `35ef43f1902364710782ddfee358e9ea4059d7d4c5afea3d3073a521d1243668`.
- Extract: `target/server-benchmark/security-findings.psv`: 85 binary-package/CVE rows, 26 distinct CVEs. Every row lacks `FixedVersion`. There are 3 CRITICAL rows for one CVE, CVE-2026-60002; the other 82 rows are HIGH.
- Ten source families account for these findings: util-linux, dracut, acl, expat, ncurses, systemd, sqlite3, openssl, openssh and perl.
- Related local artifact inspected read-only: `target/server-benchmark/scan-root/`. Its package inventory and files help assess runtime context. This extracted local root is not proof that every future CI/candidate image has the identical file set or effective configuration.

Debian's tracker tables explicitly describe **source packages**. Matching the same CVE to several binary packages built from one source does not prove that every binary contains or executes the vulnerable function. Conversely, an unexercised feature is not a patched package. Preserve both facts.

## Per-CVE review

“None” in the trixie-fix column means that no fixed trixie version is recorded in the linked tracker on the review date. The sid column gives Debian's recorded first fixed source version, not permission to install sid binaries. Descriptions and applicability are summaries; use the linked source for details.

### OpenSSH — installed source version `1:10.0p1-7+deb13u4`

The scan lists each of these six CVEs against `openssh-client`, `openssh-server` and `openssh-sftp-server` (18 rows). Trixie labels all six `no-dsa`, minor. Actual client and server code paths differ.

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-60002](https://security-tracker.debian.org/tracker/CVE-2026-60002) — CRITICAL | `ssh` client use-after-free when a server changes its host key during reexchange. The tracker explicitly says client-side only. The installed client is relevant; source matching does not establish remote exploitation of `sshd` or the SFTP daemon. | None | `1:10.4p1-1` |
| [CVE-2026-106553](https://security-tracker.debian.org/tracker/CVE-2026-106553) — HIGH | `sshd` can retain credentials after failed GSSAPI authentication. Requires that server authentication path; inspect effective `sshd -T`, includes and supported realm configuration. | None | `1:10.6p1-1` |
| [CVE-2026-106555](https://security-tracker.debian.org/tracker/CVE-2026-106555) — HIGH | `sshd` GSSAPI authentication state persists between attempts. Related to but distinct from the preceding CVE. Do not claim addressed from password/PAM tests alone. | None | `1:10.6p1-1` |
| [CVE-2026-106583](https://security-tracker.debian.org/tracker/CVE-2026-106583) — HIGH | Client-side command-line username injection involving special characters. Relevant to outgoing SSH calls and any caller constructing their parameters from untrusted data. | None | `1:10.6p1-1` |
| [CVE-2026-59999](https://security-tracker.debian.org/tracker/CVE-2026-59999) — HIGH | `sshd`: `DisableForwarding=yes` fails to override `PermitTunnel=yes`. Configuration-specific. Establish the effective configuration, including user/host `Match` blocks, rather than inferring from commented defaults. | None | `1:10.4p1-1` |
| [CVE-2026-60000](https://security-tracker.debian.org/tracker/CVE-2026-60000) — HIGH | Server resource-exhaustion issue involving GSSAPI authentication and `MaxAuthTries`. Ordinary loopback workload tests do not test this authentication behavior. | None | `1:10.4p1-1` |

Remediation: review OpenSSH's upstream fixes and Debian packaging, preserving PAM, host keys, enrollment restrictions, systemd activation and compatibility. A supported stable backport or later stable update must be qualified with both inbound and outbound SSH. Upstream's release notes are the original technical context: [OpenSSH releases](https://www.openssh.org/releasenotes.html).

The extracted root has `/usr/bin/ssh`. Its server file comments out `GSSAPIAuthentication no` and `PermitTunnel no`, and has no listed `sshd_config.d` files. Comments are not effective-runtime proof. Its client configuration enables GSSAPI authentication; that does not enable server GSSAPI authentication. No live effective configuration or attack reproduction was performed in this research task.

### util-linux — installed source version `2.41.5-0+deb13u1`

Four CVEs produce 36 rows across `bsdutils`, `libblkid1`, `liblastlog2-2`, `libmount1`, `libsmartcols1`, `libuuid1`, `login`, `mount` and `util-linux` binary packages, with distinct version syntax for `login`. Debian marks all four trixie issues `no-dsa`, minor. Detection against a UUID library does not prove it contains a mount-helper attack path.

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-76642](https://security-tracker.debian.org/tracker/CVE-2026-76642) — HIGH | Failed mount helper followed by privileged post-mount hooks can affect existing filesystems. Inspect allowed user mounts and `X-mount.idmap`/owner hooks; potential local privilege escalation requires relevant setup. | None | `2.42.3-1` |
| [CVE-2026-78408](https://security-tracker.debian.org/tracker/CVE-2026-78408) — HIGH | `nsenter --join-cgroup` leaks an open privileged cgroup descriptor across exec. A privileged operator must enter an attacker-controlled target. Upstream explicitly distinguishes this from an autonomous SUID escalation. | None | `2.42.4-1` |
| [CVE-2026-78409](https://security-tracker.debian.org/tracker/CVE-2026-78409) — HIGH | `X-mount.subdir` detached-tree traversal on Linux ≥6.15, with an authorized user mount. Tracker marks trixie vulnerable, but upstream advisory describes affected versions ≥2.42 while also listing a 2.41.6 patch. Exact downstream source/backport applicability remains unresolved; no exemption is justified from a version string alone. | None | `2.42.3-1` |
| [CVE-2026-78410](https://security-tracker.debian.org/tracker/CVE-2026-78410) — HIGH | Restricted bind mounts can be redirected through a writable source or ancestor; owner/group/mode hooks can then mutate a redirected inode. Requires relevant fstab authorization and attacker-controlled path. | None | `2.42.3-1` |

Upstream context: [nsenter advisory](https://github.com/util-linux/util-linux/security/advisories/GHSA-55fx-f4gg-cfhj), [subdirectory mount advisory](https://github.com/util-linux/util-linux/security/advisories/GHSA-8f2p-47x3-43mv). The local extracted root contains `mount` and `nsenter`; its fstab is an unconfigured base-system placeholder. Candidate installation and administrator configuration can change those preconditions. Kernel 6.18.63 meets the ≥6.15 kernel criterion, which is only one prerequisite.

Remediation: review stable-series upstream patches and exact Debian source, then qualify mounts, installer/recovery use, login and namespace/cgroup behavior. Core util-linux packages cannot be casually removed without breaking the OS.

### systemd — installed source version `257.13-1~deb13u1`

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-16742](https://security-tracker.debian.org/tracker/CVE-2026-16742) — HIGH | `systemd-homed` permits local group escalation for a logged-in homed-managed user. Debian: `no-dsa`, minor. Upstream advisory is specifically about homed/homework and homectl-managed accounts, not every systemd library or service. | None | `261.2-1` |

The scan repeats this source CVE for 9 binary packages, including `libnss-myhostname`, `libsystemd0`, `systemd-boot` and `udev`. The local extracted root has no `systemd-homed` package record and no discovered `systemd-homed`, `systemd-homework` or `homectl` file. That is important evidence of an absent runtime module in this artifact, but does not patch the shared source version or establish future-image applicability. Preserve the raw finding and verify exact candidate contents before any formal applicability conclusion. [Upstream homed advisory](https://github.com/systemd/systemd/security/advisories/GHSA-jm29-p7hh-vjhv).

Remediation: stable update or reviewed backport if homed support is shipped. Do not remove PAM/sudo groups to follow an upstream generic workaround: it would disrupt Linos's administrator model. Do not remove `libnss-myhostname`, which provides a tested hostname-resolution requirement, simply to remove a scanner row.

### dracut — installed `dracut-install` version `106-6`

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-15816](https://security-tracker.debian.org/tracker/CVE-2026-15816) — HIGH | DHCP ROOT_PATH content reaches an unquoted emergency-hook shell message; adjacent-network attacker plus relevant boot failure path. Debian: `no-dsa`, minor. | None | `112-2` |
| [CVE-2026-16445](https://security-tracker.debian.org/tracker/CVE-2026-16445) — HIGH | DHCP injection in dracut's NetworkManager initrd module. Debian: `no-dsa`, minor. | None | `112-1` |
| [CVE-2026-6893](https://security-tracker.debian.org/tracker/CVE-2026-6893) — HIGH | DHCP injection in dracut's legacy networking shell path. Debian: `no-dsa`, minor. | None | `112-1` |

The local root installs `dracut-install`, whose package description says it supplies the install helper for other initramfs tools, plus `initramfs-tools`. It does not list dracut-core/network packages, and the discovered dracut files contain the install executable, not the referenced network shell modules. Exact contents of generated initramfs images still need inspection; rootfs inventory alone cannot establish boot-time code absence. Do not conflate a dracut source match with proven network-initramfs exposure.

Remediation: review which initramfs generator and modules every Server/Recovery image actually ships; obtain a fixed stable package or reviewed patches where necessary. Rebuild initramfs and repeat boot/recovery tests after any change.

### Expat — installed `libexpat1` version `2.9.0-1~deb13u1`

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-113351](https://security-tracker.debian.org/tracker/CVE-2026-113351) — HIGH | Parser reset accounting causes subsequent parses to fail. Debian: `no-dsa`, minor. Requires a consuming application that reuses the affected parser with untrusted XML. | None | `2.10.0-1` |
| [CVE-2026-115506](https://security-tracker.debian.org/tracker/CVE-2026-115506) — HIGH | Entity-amplification protection runs too late, causing CPU denial of service. Debian: `no-dsa`, minor. | None | `2.10.0-1` |
| [CVE-2026-117610](https://security-tracker.debian.org/tracker/CVE-2026-117610) — HIGH | External-entity subparser mishandles empty streamed input and can crash. Tracker does not show a trixie `no-dsa` note. | None | `2.10.0-1` |
| [CVE-2026-118370](https://security-tracker.debian.org/tracker/CVE-2026-118370) — HIGH | Oversized attribute counts in very large XML input can overflow indexes and corrupt memory. Tracker does not show a trixie `no-dsa` note. | None | `2.10.0-1` |

Remediation: reviewed library backport or stable package update, dependency-aware regression tests, then restart/reboot consumers. Find which installed and supported services accept untrusted XML before assigning exposure; the library being installed alone is insufficient to identify a remote service.

### SQLite — installed `libsqlite3-0` version `3.46.1-7+deb13u2`

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-119373](https://security-tracker.debian.org/tracker/CVE-2026-119373) — HIGH | FTS5 prefix/doclist merging can overflow with crafted content/negative row IDs. Debian: `no-dsa`, minor. | None | `3.54.1-1` |
| [CVE-2026-119374](https://security-tracker.debian.org/tracker/CVE-2026-119374) — HIGH | FTS5 secure-delete processing of a corrupted database can overflow. Debian: `no-dsa`, minor. | None | `3.54.1-1` |
| [CVE-2026-119379](https://security-tracker.debian.org/tracker/CVE-2026-119379) — HIGH | Session-extension changeset application can overflow with a crafted changeset. Debian: `no-dsa`, minor. | None | `3.54.1-1` |

Remediation: review feature-specific vendor fixes and ABI compatibility. Verify compiled options and supported applications that open untrusted databases or apply changesets. Do not declare these exploitable over the network without identifying such a caller; do not declare them fixed because Linos's console does not call those features.

### OpenSSL — installed source version `3.5.9-1~deb13u1`

The scan repeats both CVEs against `libssl3t64`, `openssl` and `openssl-provider-legacy` (6 rows).

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid fixed status |
| --- | --- | --- | --- |
| [CVE-2026-107494](https://security-tracker.debian.org/tracker/CVE-2026-107494) — HIGH | OpenSSL QUIC memory exhaustion from retransmitted connection-ID frames; incomplete earlier fix. Not a generic TLS or SSH issue. Upstream 24 November advisory and 3.5 branch commit are linked by Debian; exact advisory content was unavailable through the research browser. | None | Unfixed; current `3.6.5-1` marked vulnerable |
| [CVE-2026-124709](https://security-tracker.debian.org/tracker/CVE-2026-124709) — HIGH | QUIC-server rejected-stream state retention can cause slow memory exhaustion. Upstream calls this Low; scanner says HIGH. Requires an OpenSSL QUIC server, not ordinary TLS use. | None | Unfixed; current `3.6.5-1` marked vulnerable |

The [17 November OpenSSL advisory](https://openssl-library.org/news/secadv/20261117.txt) says fixes are in the public repository and releases are scheduled for 1 December, including 3.5.10 for the rejected-stream issue. This is a future release, not an installed or available trixie fix today. Review the [3.5 branch patch linked by Debian for CVE-2026-107494](https://github.com/openssl/openssl/commit/734c6cd7444bdbf589bfec917e47598c1b21ddd3) and the actual stable integration before deciding backport scope. Inventory OpenSSL-QUIC consumers; do not infer QUIC exposure from libssl linkage alone.

### ACL — installed `libacl1` version `2.3.2-2+b1`

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-54369](https://security-tracker.debian.org/tracker/CVE-2026-54369) — HIGH | Path-based ACL functions follow attacker-controlled symlinks in privileged callers. Debian plans an unstable-first, then point-release update and explicitly says not to backport individual patches. New `acl_*_at()` ABI has compatibility concerns, including tar symbol collisions. | None | `2.4.0-1` |

Remediation: do not treat this as a one-line patch. Review the complete library and affected caller/ABI transition, Debian bug #1141110 and tar compatibility issue #1141146 before a stable backport plan. This is a particularly clear package-maintenance decision rather than a routine refresh.

### ncurses — installed source version `6.5+20250216-2`

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2025-69720](https://security-tracker.debian.org/tracker/CVE-2025-69720) — HIGH | Stack overflow in the `infocmp` CLI, not proof of an overflow in every ncurses library consumer. Debian: `no-dsa`, minor; tracker retains an upstream-status note. | None | `6.6+20251231-1` |

The local root includes `/usr/bin/infocmp`; it is not an absent-tool finding. The scan repeats the issue across `libncursesw6`, `libtinfo6`, `ncurses-base` and `ncurses-bin` (4 rows). Remediation requires the actual upstream CLI fix or stable package update and relevant malformed-input regression checks. A Rust/Ratatui application does not make this installed tool disappear.

### Perl — installed `perl-base` version `5.40.1-6+deb13u1`

| CVE and scanner severity | Vendor scope and applicability | Trixie fix | Sid first fixed |
| --- | --- | --- | --- |
| [CVE-2026-9538](https://security-tracker.debian.org/tracker/CVE-2026-9538) — HIGH, `fix_deferred` | `Archive::Tar` before 3.10 trusts a crafted entry-size field and can exhaust memory. Debian postpones trixie while upstream regressions are sorted out. | None | `5.42.3-1` |

The local root has the minimal `perl-base` package, no full Perl/modules package record, and no discovered `Archive/Tar` file. Verify the exact candidate module inventory before formal applicability classification. Essential Perl runtime removal is not a justified remediation. If the module is shipped or later installed, review its fixed version and regression history instead of taking a sid interpreter wholesale.

## What remains unproven

1. No exploitation was reproduced; this is source/vendor triage with read-only artifact inspection.
2. No claim is made that each source-wide binary match is runtime exploitable. The strongest absence leads are homed, dracut network shell modules and Perl Archive::Tar; generated initramfs, image identity and exact file manifests still need proof.
3. SSH effective configuration, compiled SQLite features, installed XML/QUIC consumers, downstream util-linux patch history and candidate fstab rules still need explicit review.
4. Debian `no-dsa` and upstream Low/Moderate assessments do not override Linos's HIGH/CRITICAL policy. Those differences explain triage context; raw severity remains preserved.
5. Source/kernel advisory coverage is wider than these 26 findings. A clear report would not alone certify a custom kernel, boot chain, recovery media, physical installation or beta.

The correct current release decision is **security gate unresolved**. Continue preparation that does not depend on a clear gate, but do not mark local final-image qualification, physical support or Server beta complete. Any backport effort must finish with exact build identities, scanner/advisory evidence and functional tests on both architectures.
