# Linos current handoff

Updated 23 November 2026. Start here, then check git status and current GitHub Actions results. This supersedes the earlier interaction-only checkpoint for resuming work.

## Owner direction

Finish Server qualification toward a beta while beginning Desktop work in the next session. Keep Linos's “Made to feel at home” philosophy. Server's “The Familiar Workbench” is an edition-specific expression, not a Desktop UI specification. Do not call Server beta ready until the release gates below have evidence.

The owner requested committing/pushing first, representative real VM workloads next, then CI vulnerability checks. The existing combined console/setup/recovery work was committed and pushed as `b86ee19eee63865a41d3d193afe96a6329614ec1` before new VM testing. No production deployment or beta publication was performed. Existing user VMs were preserved.

## Current implementation

- Reliable Home editing, bounded paste, history, Unicode cursor movement and native terminal handoff with interruption recovery.
- Explicit shell readiness and exit status, password masking and safe routing across view changes.
- Measured resource bars and task controls with kernel authorization, process identity protection and deliberate confirmation.
- Saved preferences, real sudo-authorized hostname/timezone settings and local hostname resolution through the declared `libnss-myhostname` package dependency.
- Cancellable inspection subprocesses with five-second deadlines, combined four-MiB output budgets, process-group cleanup and accurate Store inventory errors.

Details: [interaction review](../reviews/2026-11-23-server-interaction.md), [authorization review](../reviews/2026-11-23-server-authorization.md), [current status](current-status.md), and [console contract](server-console.md).

## Verified locally

The Rust workspace passed 133 tests. Formatting, strict all-target Clippy and diff whitespace checks pass. The new workload test is intentionally ignored in ordinary workspace runs, like the two earlier explicit guest checks.

Three explicit AMD64 guest cases have passed against the rebuilt development package and native `6.18.63-linos` base:

1. Full Home navigation, editing/history, masked input, task signals, modified-key confirmation rejection, native command interruption/return and logout.
2. Ordinary administrator sudo authentication, cancellation without mutation, successful settings changes, denial of cross-user task signals and local hostname resolution after renaming.
3. Representative server workloads, added in `tests/console.rs::representative_server_workloads_in_guest`:
   - An unprivileged user creates an 8 MiB random file, archives/extracts it and checks byte-for-byte equality and SHA-256 integrity.
   - A systemd-managed loopback TCP echo service runs as that user and handles 20 request/reply connections, then another batch after task pause/resume.
   - The administrator restarts the service, verifies journal evidence and confirms request handling afterward, then stops it.
   - A private root-owned file remains unreadable to the workload user.
   - A bounded five-second CPU load and 32 MiB tmpfs allocation run while Home's resource view refreshes. The captured CPU bar reports 100% on the single-vCPU guest. Pressure data is cleaned up afterward.
   - The guest shuts down normally.

All local VM runs are sequential, one vCPU and 1024 MiB RAM. Large copies are disk-backed under `target/`. Networking is limited to guest loopback, with no external network configured. Root debug shells are fixture setup only; these tests do not requalify normal enrollment/login PAM or SSH. These are functional foundation tests, not a database/container certification, a sustained latency benchmark or storage power-loss qualification.

Artifacts:

- `target/console-qualification/guest-workloads/`: assembled terminal snapshots, pressure readings, results and terminal output.
- `target/console-qualification/guest-authorization/`: ordinary-user authorization evidence with credential redaction.
- `target/console-qualification/guest/`: full interaction evidence.
- `target/console-qualification/workspace-tests.log`: 133-test baseline.
- `target/server-workload-qualification/disk.ext4`: disposable workload test image.
- `system/dist/linos13_dev_v1.0.0.deb`: local development package, not an immutable release image.

Console SHA-256: `70ecf9f7be76dce87bbdc11a2e6ec6fc21caa212c2665e3ccc2eebbd2f4389bb`.
Package SHA-256: `b5914109643ed970048646f8407f43e7db1aeb52185a4fc4b3dd8438335e2a47`.

## CI security and image gates

`.github/workflows/security.yml` adds RustSec advisory checks for both Cargo lockfiles, plus Trivy repository secret and misconfiguration checks. It runs on relevant pushes, pull requests, manual dispatch and weekly. Audit tools are version-pinned; the Trivy setup action is commit-pinned. Findings are retained as reports. RustSec failures and HIGH/CRITICAL repository findings fail their jobs.

The manual full `Linos CI` now scans the actual built AMD64/ARM64 root filesystems with Trivy. HIGH/CRITICAL OS package findings, including unfixed ones, block the job. Functional checks still run after a scan failure so the scan does not hide boot/enrollment results. Scanner errors also block the gate. Reports are retained for 14 days.

`Linos Kernel Release` now permits manual qualification that reuses the existing qualified kernel artifacts from run `44213747067`. Both architecture artifacts were confirmed present and unexpired before dispatch. Manual qualification does not publish. Never use the `kernel:release` marker merely to run tests; that path can publish after its gates.

The previous full CI failure at `40ac0c2` was Clippy's constant-size `chunks_exact` lint on both architectures. The first pushed checkpoint includes the `as_chunks` fix. New CI results must be checked; older failures or local passes are not evidence of the new run's outcome.

Run links and current outcomes are recorded below after dispatch. A clean scanner result covers known advisories and configured scanner rules only. Custom kernel vulnerability coverage and production hardening still need explicit review. Native GitHub secret scanning was disabled when inspected; repository scanning here is provided by the new workflow.

## Next session: Server beta and Desktop

1. Read the exact CI runs below, triage every failure and advisory, and preserve the distinction between scanner failure and confirmed vulnerability. Do not suppress findings simply to obtain a green badge.
2. Complete a fresh immutable native Server image with the new package/dependency set and a new build identity. Qualify enrollment/login, SSH, reboot persistence, installation/recovery and updates on the final beta candidate. Current local testing installed the package into disposable copies of an older native image.
3. Record remaining hardware/platform support and known limitations. Production signed updates, data migration, recovery/install safety, sustained workloads and physical hardware are not established by the tests above. Release Server beta only against an explicit candidate and supported test matrix.
4. Begin Desktop from `PRODUCT.md`, `DESIGN.md`, `docs/architecture/desktop-platform.md`, and `system/src/graphics/`. Desktop remains an X11 graphics experiment; it is not a qualified Desktop operating system. Settle the Desktop composition, session/input model, toolkit and supported hardware scope before expanding implementation. Keep Server regression and security CI running during Desktop development.

Reproduce the workload test with a fresh copied disk and the two staged `.deb` files described in the authorization review:

```sh
LINOS_CONSOLE_GUEST_DIR=target/server-workload-qualification \
LINOS_CONSOLE_BOOT_DIR=images/server/builds/amd64/20261123.4 \
CARGO_BUILD_JOBS=2 cargo test --locked --test console representative_server_workloads_in_guest \
  -- --ignored --nocapture --test-threads=1
```

Do not run multiple local VMs concurrently, place large images on RAM-backed storage, use Python automation, reset user VM disks, or assume a package installation test is a newly built image qualification.

## Dispatched CI

Qualification source commit: `a682af89e8327d5a17f29a55dde7c2ac2b56c015`, pushed after the local workload pass. Subsequent handoff-only commits do not change the tested application or CI configuration.

| Run | Observed status at handoff | Link |
| --- | --- | --- |
| RustSec and repository security | In progress | [45086107423](https://github.com/phe-rus/linos/actions/runs/45086107423) |
| Full AMD64/ARM64 Rust, image, OS CVE and boot checks | Queued / AMD64 job starting | [45086107602](https://github.com/phe-rus/linos/actions/runs/45086107602) |
| Native-kernel installed-system qualification | In progress | [45086107465](https://github.com/phe-rus/linos/actions/runs/45086107465) |

Refresh these exact runs before making release claims. CI success has not been asserted. The workflow named “Linos Kernel Release” is taking its qualification-only integration path for this commit, not its publishing path.

```sh
gh run view 45086107423
gh run view 45086107602
gh run view 45086107465
```

If generic `git fetch origin` fails because this checkout retains a deleted `kernel` refspec, use `git fetch origin refs/heads/main:refs/remotes/origin/main`. That explicit main fetch and both source pushes succeeded in this session; repository fetch configuration was left intact.
