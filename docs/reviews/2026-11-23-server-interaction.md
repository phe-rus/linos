# Server interaction and resource controls

**Follow-up:** [Authorization, bounded queries and local package qualification](2026-11-23-server-authorization.md) is the newer checkpoint. The observations and hashes below describe the initial interaction slice.

23 November 2026. Local development continuation on an already dirty main checkout. Existing image, setup, update, store and console changes were preserved. No commit, publication or installed host configuration change was performed.

## Result

Home now accepts held Backspace, cursor movement, Delete, Home/End, Unicode input, editing shortcuts, command history and bounded bracketed paste. Ctrl+C clears unsubmitted input and can interrupt foreground work. Ctrl+D sends EOF to pending work. Paste never submits a command. A sliding input window keeps long commands and their cursor visible. Palette scrolling shows the selected action that Enter will actually execute.

The shell has an explicit prompt and exit-status handshake. Output resembling a shell prompt cannot falsely finish a command or disable password masking. The first prompt is consumed before the first command is sent. Terminal control strings are filtered across read boundaries, split UTF-8 is retained, carriage-return progress updates replace the current preview line, and each drain is bounded so noisy output yields to input. Shell completion and failure are visible. A terminated shell can be restarted by the next command. Session teardown hangs up its foreground work and lets the shell clean up jobs.

Known terminal programs such as editors, top, SSH and tmux receive the real terminal automatically. `/shell COMMAND` provides explicit handoff for other programs. Interactive job control isolates command interruption, and Home explicitly restores its foreground process group before redrawing. The terminal test verifies Ctrl+C interrupts native work and returns to Home. On return, Home restores its own screen and reports the command result. The embedded panel remains a text preview, not a complete terminal emulator. Arbitrary interactive applications may still need explicit native handoff.

Resources measures CPU usage from counter changes and labels load average separately. Memory, swap and storage come from actual kernel/filesystem readings. Missing values remain unavailable; unconfigured swap is not shown as a healthy percentage. Bars include numeric values and do not depend on color. No artificial workload-completion percentage is displayed.

`/tasks` shows live state, resident memory and whole-machine CPU share. Selection follows PID plus process start time across snapshots. Pause, resume, terminate and force kill require confirmation. Force kill warns about unsaved work. Linux kernel permissions authorize signals. A pidfd and identity recheck prevent acting on a recycled PID. PID 1, Home and its ancestor sessions are protected. There is no automatic privilege escalation for another user's tasks.

Settings now saves sign-in mode, color and refresh interval per user. Hostname and installed timezone changes require confirmation and use standard sudo authorization, with the result displayed in the command session. Invalid values are rejected before a change. Language and keyboard editing remain in system setup. Device time zone follows the actual zoneinfo link rather than assuming `/etc/timezone` is current.

Network, service, log, hardware and store reads run outside the key event loop. Each navigation owns its result receiver, so a late result cannot replace a newer view. This keeps typing usable during reads; it does not implement process cancellation or timeouts for every underlying external query.

## Philosophy review

The standard remains Linos's “Made to feel at home” and Server's “The Familiar Workbench.” Apple's [design principles](https://developer.apple.com/design/human-interface-guidelines/design-principles/) inform the comparison through familiarity, consistent behavior, clear feedback, accessibility and user control. Apple's [Settings guidance](https://developer.apple.com/design/human-interface-guidelines/settings) also distinguishes useful settings from unnecessary choices.

The concrete improvements are familiar editing, visible selection, real readings, numeric labels alongside bars, terminal-default foreground text, optional monochrome output, persistent preferences, explicit action review, safe cancellation and actual completion/failure feedback. This is a qualitative comparison, not an Apple certification or evidence of equal product reliability. End-user testing across terminal palettes, SSH clients and physical devices remains necessary.

## Verification

The three initial failing tests reproduced ignored held Backspace, incorrect cursor deletion and Ctrl+C retaining unsubmitted input. Those regressions pass after the fixes.

The final Rust workspace suite passes 123 tests with the VM test intentionally ignored in the normal run. The suite includes live shell cwd/environment, pipelines, Unicode and colored output, failed commands, prompt-like output during secret input, output filtering and split markers. Process tests observe a real test process enter and leave the stopped state, reject changed process identity, and reject stopping Home itself. Render tests cover 30×10, 36×14, 48×18, 80×24 and 120×32 monochrome layouts. Persistence tests reopen preferences from disk and check rejected values leave the previous setting intact.

`tests/console.rs` drives the compiled binary through a real Linux PTY. Its dev-only VT parser reconstructs the actual screen from incremental terminal output. It covers every navigation view, live CPU refresh, saved preferences, cancelled system changes, actual Backspace and arrow sequences, history, pipelines, secret masking, clear, failure feedback, confirmed task signals, native handoff and logout. Logs are checked for exposure of the test secret. This is terminal automation, not a visual review on a physical terminal.

The explicit AMD64 guest test runs sequentially with one vCPU, 1024 MiB RAM and no network. It boots the existing `20261123.4` native `6.18.63-linos` kernel with the new release console binary injected into a disk-backed disposable image copy. The test-only guest debug shell bypasses enrollment/PAM solely inside that copy. In addition to the shared terminal checks, it applies and reads back hostname and timezone changes and verifies the cancelled hostname was not applied. It powers the guest down at the end. This does not requalify enrollment, ordinary-user sudo/PAM, an immutable packaged image, ARM64 or physical hardware.

Release console SHA-256 for this checkpoint: `a6116b0a6ba0d84182674c97eafec914651d3e2b547dc88c7e49d252079042f5`.

Rust 1.96.1, formatting, strict all-target Clippy, workspace tests and release console compilation passed. No dependencies were added to the production binary. `vt100` is a test-only dependency.

Reproduce the host checks:

```sh
CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
CARGO_BUILD_JOBS=2 cargo build --release --locked --bin linos
```

For guest qualification, first prepare a disposable copy under `target/`, inject the just-built console while the image is offline, and provide the matching kernel/initramfs directory. Never pass an installed or in-use guest disk. The harness enforces a disposable image directory under `target/` and does not inspect or stop unrelated VMs.

```sh
LINOS_CONSOLE_GUEST_DIR=target/console-qualification \
LINOS_CONSOLE_BOOT_DIR=images/server/builds/amd64/20261123.4 \
CARGO_BUILD_JOBS=2 cargo test --locked --test console home_in_disposable_guest \
  -- --ignored --nocapture --test-threads=1
```

Screen snapshots and terminal logs are under `target/console-qualification/host/` and `target/console-qualification/guest/`. Workspace test output is in `target/console-qualification/workspace-tests.log`.

## Remaining qualification

There is no claim that every possible server use is tested. Networked workloads, database and container hosting, remote sessions, storage failure, sustained resource pressure, service restarts, concurrent administrators, accessibility with real assistive technology and production package/update operations need their own workload and failure matrices. Background reads still need stronger cancellation and bounded command lifetimes. Task controls send signals rather than managing service dependencies or cgroups. General job progress is unknown unless the command reports it.

The source is ready for review against this evidence. A packaged image rebuild, normal-user authorization checks and wider terminal/device qualification remain separate gates before release.
