# Linos repository architecture

Linos is one operating-system codebase, with one Rust Cargo package and a few defined entry points. The source tree is **not** the USB filesystem tree or the installed root filesystem.

```text
linos/
  .github/workflows/
  assets/brand/
  kernel/                 # Linux source integration policy and verified modifications
  hardware/profiles/      # device qualification information
  system/
    src/
      console/
      hardware.rs
      setup/
      recovery/
      init.rs
      login.rs
    identity/             # filesystem identity and welcome defaults
    services/             # installed systemd and OS integration units
    release/              # Debian-compatible bootstrap package contents
  build/
    kernel.sh             # kernel research and isolated image assembly
    package.sh            # Linos executable/system packaging
    image.sh              # base system image and rootfs
    recovery.sh           # recovery environment, offline payload and signatures
    media.sh              # UEFI image and bootable media
  tests/
    kernel.sh             # kernel QEMU Server smoke
    boot.sh               # UEFI and independent recovery tests
    server.sh             # Server, persistence and console tests
    enrollment.sh         # interrupted/complete enrollment
    enrollment.exp        # Expect test fixture
    installation.sh       # layout and disposable install simulation
  docs/
  Cargo.toml
  Cargo.lock
  README.md
```

We do not create empty directories for hypothetical features. A directory represents a stable responsibility, not a Rust package or separate product. The one root Cargo manifest builds nine executable targets: `linos`, `detect`, `enroll`, `linos-login`, `recovery`, `linos-update-image`, `linos-update-health`, `init`, and the experimental `linos-graphics`. The Server package builds only its four installed binaries. Explicit binary entry points preserve installation paths and privilege separation; source files are organized by related functionality.

The existing `system/release/` directory is **packaging input**, not the directory installed as the OS root. `build/*.sh` scripts produce artifacts in ignored output locations. The proposed removable PC installation media has `EFI/` and `linos.pherus.org/` at its root, distinct from this repository; the copy-to-USB experience is not production-ready.

CI must validate Rust formatting, lint, tests, shell scripts, QEMU Server, Recovery, offline installation and isolated research kernel boot. Source-code simplification does not imply successful validation until runs pass. Never write to a physical disk in tests.
