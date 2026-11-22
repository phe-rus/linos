# Server authorization and inspection reliability

23 November 2026. Continuation of [the initial interaction checkpoint](2026-11-23-server-interaction.md). Existing uncommitted work was preserved. No production deployment, existing user VM change, commit or publication was performed.

## Changes

Task, Settings and power confirmations now require an ordinary Y press. Ctrl+Y, Alt+Y, other shortcut modifiers, repeated keys and release events cannot approve an action. Shift remains accepted for an uppercase Y. A failing regression first demonstrated that Ctrl+Y confirmed a task action; self-protection kept that probe harmless. The fix is shared by all three confirmation paths. The real terminal driver also sends Ctrl+Y to test task dialogs and verifies that they remain open.

External inspection reads now share `system/src/console/query.rs`. Each command receives a five-second deadline, a combined four-MiB stdout/stderr budget, closed stdin and an isolated process group. Both output pipes are drained without blocking each other. Cancellation, timeout, excessive output and launch failures produce errors instead of partial successful data. Exit status remains the command's actual status.

A navigation owns its cancellation token and result receiver. Replacing the view or leaving Home cancels that inspection helper. Cleanup stops its process group and reaps the leader. The leader remains unreaped until cleanup, preventing the group identifier from being reused for an unrelated process. A test specifically cancels a descendant retaining a pipe after its leader exits. Shell jobs retain their normal Linux lifecycle; this query policy applies to inspection helpers.

Store inventory now parses complete output within the query budget instead of truncating the source at 16,000 characters. An unavailable or incomplete inventory is reported as unavailable, with no invented zero package count or size. A thousand-package fixture verifies the larger inventory path, and cancelled/malformed inventory tests verify failure reporting. Hardware's filesystem probes remain asynchronous; they are not subprocess queries subject to the command deadline.

## Hostname resolution

The ordinary-user guest test exposed an actual integration defect: after a successful hostname change, subsequent sudo commands warned that the new local hostname could not be resolved. The base image used `hosts: files dns`, and its old local host mapping did not cover the new name.

The Server package now depends on `libnss-myhostname`. Its standard Debian installation configures the NSS module. This keeps the current machine name resolvable offline and preserves administrator-maintained host entries during renames, following [systemd's documented hostname resolver behavior](https://www.freedesktop.org/software/systemd/man/latest/nss-myhostname.html). No custom privileged configuration helper or new host-file rewrite was introduced. A guest assertion reproduced the warning before the dependency was installed; it passes afterward, along with `getent ahosts` for the changed hostname.

## Authorization qualification

A fresh disposable guest creates a temporary administrator account in the standard sudo group. Its random password is provisioned with terminal echo disabled, registered for artifact redaction and checked for absence in the raw terminal output. The root debug shell is used only to prepare the fixture; Home itself then runs as the ordinary user. This exercises sudo's real authorization and authentication stack, not a simulated prompt.

The test verifies:

- Home runs under a nonzero UID.
- A confirmed attempt to terminate a root-owned fixture process fails with kernel permission denial. The fixture remains alive afterward.
- Cancelling an actual sudo password request leaves the hostname unchanged and restores ordinary command input.
- Hostname and timezone changes each require explicit confirmation and fresh sudo authentication after `sudo -k`.
- Password input remains masked and is absent from captured raw output.
- Commands after authorization succeed without repeated administrator prompts.
- Settings reads back the applied values, and the changed hostname resolves locally without sudo warnings.

The separate full interaction guest covers every navigation view, resource refresh, stored preferences, editing/history, hidden input, clear, failed commands, actual task signals, modified-key confirmation rejection, native interruption/return and logout.

Both explicit guest tests install the rebuilt development `.deb` and resolver dependency inside fresh copies of the native `20261123.4` AMD64 image. They run sequentially with one vCPU, 1024 MiB RAM and no network. The resolver package is staged in advance for offline installation. Guest shutdown completes normally. This establishes package installation and the tested interactions on that guest base; it does not establish a newly assembled immutable image, enrollment/login PAM, ARM64 or physical-device qualification.

## Build and evidence

- Rust workspace: 133 passing tests. Two explicit VM tests are ignored in the ordinary workspace run and were each run separately.
- Formatting, strict all-target Clippy and diff whitespace checks pass.
- Release console and the full local development Server package were rebuilt. The package manifest declares `Depends: libnss-myhostname`.
- The prior local development package was backed up under `target/console-auth-deps/` before rebuilding.
- Ordinary-user screenshots and redacted terminal output: `target/console-qualification/guest-authorization/`.
- Full guest screenshots and terminal output: `target/console-qualification/guest/`.
- Host interaction snapshots and output: `target/console-qualification/host/`.
- Workspace results: `target/console-qualification/workspace-tests.log`.

Artifacts from this checkpoint:

| Artifact | SHA-256 |
| --- | --- |
| `target/release/linos` | `70ecf9f7be76dce87bbdc11a2e6ec6fc21caa212c2665e3ccc2eebbd2f4389bb` |
| `system/dist/linos13_dev_v1.0.0.deb` | `b5914109643ed970048646f8407f43e7db1aeb52185a4fc4b3dd8438335e2a47` |

The `.deb` is still development version 1.0.0, architecture amd64. It is a local artifact containing the current combined working tree, not a published version or an update installed into the owner's existing VM.

## Reproduce

Host checks:

```sh
CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

Guest prerequisites: a fresh disk copy under `target/`, matching boot artifacts and KVM. While the copied disk is offline, stage the rebuilt release package as `/root/linos-test-release.deb` and the matching resolver package as `/root/linos-test-nss.deb`. The harness installs them inside the guest. Current dependency fixture: Debian trixie amd64 `libnss-myhostname` 257.13-1~deb13u1, downloaded with APT into `target/console-auth-deps/`. No package was installed into the host OS.

Run these sequentially, with a fresh disk copy for each test:

```sh
LINOS_CONSOLE_GUEST_DIR=target/console-qualification \
LINOS_CONSOLE_BOOT_DIR=images/server/builds/amd64/20261123.4 \
CARGO_BUILD_JOBS=2 cargo test --locked --test console home_in_disposable_guest \
  -- --ignored --nocapture --test-threads=1

LINOS_CONSOLE_GUEST_DIR=target/console-auth-qualification \
LINOS_CONSOLE_BOOT_DIR=images/server/builds/amd64/20261123.4 \
CARGO_BUILD_JOBS=2 cargo test --locked --test console ordinary_administrator_authorization_in_guest \
  -- --ignored --nocapture --test-threads=1
```

## Next qualification

Next work is a fresh immutable image build with these package dependencies, standard enrollment/login and SSH sessions on that image, then explicit workload cases for network services, data/storage, containers and sustained pressure. Database hosting, real device latency, storage failure and production update/package transactions remain unqualified. Query deadlines cover normal process execution and cancellation, not recovery from an uninterruptible kernel or failed storage device.

Keep the established Linos design and the distinction between measured usage, authorization and observed action results. Avoid claiming an action completed merely because a confirmation was accepted or a signal was sent.
