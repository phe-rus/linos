# Linos Server console (first iteration)

The `linos` command is a native Rust system-administration entry point.
It is an **application**, not a login-shell replacement, terminal emulator,
prompt injection, or background overlay.

## UX contract

- `linos` on an interactive terminal presents a compact, numbered menu.
- `linos status`, `linos network`, `linos services`, `linos service ssh.service`, and
  `linos logs ssh.service` are direct commands.
- Standard output is plain when piped, redirected, used in automation, or when
  `TERM=dumb` or `NO_COLOR` is present.
- No user-wide `PS1`, `PROMPT_COMMAND`, terminal modes, aliases, startup files,
  environment variables, or screen buffers are modified.
- Claude Code, Codex, vim, ssh, tmux, or any other TUI owns the terminal while
  running. Linos does not wrap, decorate, resize, or interrupt those applications.
- The first screen intentionally avoids full-screen graphics, cursor gymnastics,
  animation, and persistent chrome. Design enhancements must preserve this property.

This is a functional first iteration of Server UX, not the final visual design.

## Development checks

```sh
cargo fmt --all -- --check
cargo clippy -p console --all-targets --locked -- -D warnings
cargo test -p console --locked
./build/package.sh
```

On a booted Server guest, run `linos` for the guided menu or
`linos status` for the noninteractive overview.

## Boot presentation

The Server image uses Linos' own `/etc/os-release` identity so systemd
announces Linos rather than Debian during startup. This does not misrepresent
the current Debian 13 base or rewrite its kernel identity. A quiet normal boot
uses `quiet loglevel=3 systemd.show_status=auto`: healthy startup details
are retained in the journal, while warnings and failures remain visible.
Run `journalctl -b` or `dmesg` after login for full diagnostics.
For a deliberately verbose boot, remove the quiet arguments and add
`loglevel=7 systemd.show_status=yes` in the test QEMU kernel command line.

The first boot has **no default username or password**. Enrollment prompts
for a new administrator name and password, and getty units are ordered after
the enrollment service so no competing login prompt appears during setup.
After enrollment, log in with the credentials just created.

## Leaving the local VM

In `./build/console.sh amd64`, the guest is connected through QEMU
`-nographic`. A Linux login prompt will not accept `clear`, Ctrl+C or
Ctrl+Z as a VM exit. To return to ChromeOS Debian, press **Ctrl+A**,
release the keys, then press **X**. This is QEMU's escape sequence, not
Ctrl+X. Alternatively Ctrl+A, release, C switches to the QEMU monitor;
type `quit` and press Enter. After logging in, `exit` leaves the guest
shell but does not stop the VM.

The `console.qcow2` overlay persists guest state. Running the same launcher
again does not reset onboarding. Archive or rename that overlay before
starting a fresh first-boot test; never reset a guest with data you need.

## First-boot guided setup

The first-time enrollment application now clears the normal-boot serial console
**only when stdin and stdout are interactive terminals** and displays a compact
line-connected setup flow. The layout is inspired by modern terminal wizards:
a simple welcome panel, a highlighted active question and completed steps.

The first prompt is simply **Username**. The user chooses an account name and
then sets the password through the existing system `passwd` command, keeping
PAM password handling intact. A successful setup shows a completion message
with the chosen username.

For `NO_COLOR`, `TERM=dumb`, or non-interactive streams, the same program
uses plain output without ANSI screen clearing. No shell configuration or
third-party terminal application is affected. Kernel messages remain
available through `journalctl -b` and `dmesg`; they are not deleted.

## Visible boot progress and handoff

Systemd shows genuine unit startup statuses with `systemd.show_status=yes`.
Routine kernel messages remain quiet and errors are not suppressed.
The first-boot wizard clears prior console output only after startup, when
it takes control of the interactive terminal. All welcome-panel borders
are calculated to a consistent width and checked by a Rust unit test.

## Interactive login and SSH

There is still no preset account or password in distributed images.
For a development VM, enter `linos` as the username during setup and
choose a password. Do not use a predictable password on a networked server.

SSH authenticates users before a server-side UI can be shown. After
authentication, local and SSH **interactive login shells** display a
small Linos welcome banner and offer the `linos` management command.
Non-interactive SSH commands, pipes, scripts, and nested TUIs receive
no shell-wide decoration. Set `LINOS_NO_WELCOME=1` to opt out.

## First-boot console handoff

Normal startup keeps real systemd service progress visible. Immediately before
interactive enrollment, Linos asks systemd PID 1 to stop writing startup
status lines to the console (SIGRTMIN+21). Only then does it clear the screen
and render the setup prompt. This avoids late `[ OK ] Started …` output
appearing inside a username or password field. Journal logging and error
handling are unchanged; after reboot the startup progress policy is restored
from the kernel command line. The handoff is limited to an interactive TTY.

The test suite checks the new UI width and plain-output fallback. A full QEMU
enrollment run verifies the first-boot flow, while serial-console behavior
should also be reviewed in a manual development VM.

## First-run configuration choices

The new `cliclack` setup wizard collects username, administrator password,
language, keyboard layout, timezone/location, hostname, and optional DHCP,
then reviews settings before applying. The account password is still entered
through system `passwd`, not through command-line arguments or logs.
Initial language choices are English, French and Chinese; locale generation
is required for the latter two. Keyboard choices are US, UK, French and German.
Timezones are limited to the supported installed zoneinfo entries.

Networking remains opt-in: choosing **Configure later** makes no network
changes. Selecting **Automatic DHCP** enables systemd-networkd on future
boots. No default administrator or SSH password is embedded in the image.
Interactive setup can be cancelled, leaving owner enrollment incomplete
rather than silently accepting unsafe defaults.

Noninteractive sessions keep the original minimal plain prompt and use
English/UTC/offline defaults. The wizard must not run inside SSH startup or
third-party terminal applications.
