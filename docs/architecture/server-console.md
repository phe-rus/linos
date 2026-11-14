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
cargo clippy -p linos-console --all-targets --locked -- -D warnings
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

## Server home, hostname and clean handoff

After enrollment, Linos automatically requests a clean reboot. The hostname
suggestion is `<username>-server` but can be changed before applying.
The current hostname is also activated using `hostnamectl`.
The enrollment screen sends ANSI erase-scrollback where supported before
rendering the Clack wizard. This is a best effort and cannot delete scrollback
stored by every terminal emulator or an external serial-console logger.

A local or SSH **interactive login shell** starts the `linos` home, with
`/status`, `/network`, `/services`, `/logs`, `/help`, `/shell`.
`/shell` returns to an ordinary Bash shell. Non-interactive SSH commands
and pipelines never launch the home. It does not intercept Claude Code,
Codex or other terminal applications invoked after exiting to the shell.

The standard system login program still authenticates users. A graphical
Clack login replacement is deliberately not implemented: replacing PAM/getty
without comprehensive security and recovery testing would be unsafe.

## Native Linos sign-in identity

The Server image replaces Debian's generic `/etc/motd` with a short Linos
message, and uses a Linos `/etc/issue` for the local getty. The kernel
continues identifying its real Debian-derived build in `uname`, kernel logs,
and diagnostics, rather than falsifying kernel provenance. The regular shell
uses a minimal Linos-colored prompt for interactive Bash only, after exiting
`linos` with `/shell`. Standard system directories are preserved.

## Unified session and power lifecycle

The Linos home offers **Return to shell**, **Sign out**, **Restart server** and
**Shut down server** using the same Clack interaction model as first-time
setup. Power actions require an explicit confirmation defaulting to **No**.
The OS still authorizes the action through systemd/polkit, and a denied
operation leaves the session running. Noninteractive power commands are
rejected by the Linos CLI to avoid accidental automation. Normal shell tools
remain available after **Return to shell**.

Sign out is communicated to the interactive login shell by a dedicated exit
code, allowing it to terminate the login session and clear the console.
Exiting the Bash login shell similarly clears the display. This behavior is
never applied to noninteractive SSH commands. The first-boot enrollment unit
skips future boots when its protected owner record already exists, so it
cannot print an unrelated enrollment message after sign-out.

## Ratatui Server home

The interactive Server home now uses Ratatui 0.30.2 with Crossterm events.
It is a responsive, full-screen TUI, while first-time enrollment, installation
questions and limited guided confirmations keep using Clack-style prompts.
Only the Linos process occupies the alternate terminal screen. On return to
Bash, it restores the terminal without intercepting Vim, Claude Code, Codex,
SSH commands, or other terminal applications.

Controls: Up/Down, Tab, Enter, 1-5 to switch views, / to type slash commands,
PageUp/PageDown to scroll content, q/Esc to return to Bash. Sign out exits
the login shell; shutdown/reboot display a confirmation defaulting to no
and delegate authorization to systemd. No default passwords are embedded.
Network, services and logs are initially read-only views.

For scripted commands, `linos status`, `linos network`, `linos services`
and `linos logs ssh.service` retain their stdout-based API. The TUI renders
read-only command output after filtering terminal control characters.

## Composer-first Server home

The Ratatui home follows a content-and-composer layout: a compact identity at
the top, a spacious workspace in the middle, optional filtered command
suggestions above the command field, and a composer fixed at the terminal
bottom. Unlike the earlier fixed left navigation, the `/` palette appears only
when typed. It shows up to four relevant commands, with Up/Down navigation.

It is a terminal application, not a shell wrapper or an imitation of Claude
Code's branding. Clack remains the toolkit for first-boot questions and
the local login presentation. Returning from the TUI restores the regular
terminal so Claude Code, Codex, Vim and other full-screen programs own it.
