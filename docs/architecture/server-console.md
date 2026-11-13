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
