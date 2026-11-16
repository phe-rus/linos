# Source layout

Linos 13 is one operating-system project, not a separate project for every executable.

```text
linos/
  crates/
    infs/           # one package with console, detect, enroll, login, recovery bins
    init/           # separate early device provisioning component
  kernel/           # design, policy and source documentation
  hardware/         # device profiles, firmware expectations
  system/           # installed defaults and OS services
  packages/         # OS package inputs
  build/
    kernel/         # research kernel builds and QEMU integration
    hardware/       # boot media, UEFI, Recovery, installer and payloads
    system/         # package/rootfs/image builders, console and Server tests
  assets/
  docs/
  .github/workflows/
```

The `infs` package produces distinct executables with their existing names and install paths. They are one Rust package, not five separately versioned packages. `init` stays separate because of early provisioning and privilege boundaries.

Developer build scripts are grouped by purpose. This **source layout** is different from the proposed USB boot layout, which has the root folders `EFI/` and `linos.pherus.org/`. Moving a shell script must never change the installed image without a test.

Required checks: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, Bash syntax checks, Server amd64/arm64 QEMU, Recovery QEMU and kernel research QEMU. Do not assert any CI success until observed.
