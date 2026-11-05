# linos13

**linos13** is an experimental Linux-based operating system developed by Pherus. Its goal is a reliable and cohesive experience for servers and personal devices, inspired by the system design of macOS, iPadOS, iOS and ChromeOS without reproducing their interfaces or abandoning Linux compatibility.

linos13 uses the Linux kernel, Debian packages and `apt`. The first development target is a headless server system. A graphical environment is a later, separate project.

> **Status:** early development prototype, not a production-ready operating system. The Orange Pi 5 Plus is a development host, not the only supported target.

## Marauder · Sequence 9

| Field | Value |
| --- | --- |
| System | linos13 |
| Release name | Marauder |
| Sequence | 9 |
| Generation | 1.0.0 |
| Build | 20261025.3 |
| Channel | development |
| Debian base | 13 (trixie) |

Release names and **Sequence** are the human-facing identity. The sequence counts down from 9 to 0 within a release generation; the generation, build identifier, kernel version and Debian base have separate technical roles. Sequence 9 does not mean Linux kernel 9.

Canonical release metadata is in `packages/release/usr/lib/linos13/release`. The console banner is in `system/base/identity/issue`. Standard `/usr/lib/os-release` integration is still pending, so systemd and some tools may identify the Debian base.

## Working today

- Rust `detect` executable reports host architecture, kernel, model, firmware, CPU count and RAM.
- `mmdebstrap` assembles a minimal Debian 13-based root filesystem and installs a linos13 release package.
- The image builder extracts the selected Debian kernel and initramfs and assembles a 3 GiB ext4 image.
- ARM64 QEMU boot has reached `multi-user.target`, started systemd services and shown a serial login prompt.
- Rust `init` creates an independent, persistent hostname in a specified *offline* root directory. It rejects `/`, preserves existing values, records an `unclaimed` provisioning state and tests restart recovery.
- Thirteen offline initialization tests are defined (pending verification of the latest changes on the development host). Code has passed Cargo formatting and Clippy checks.

**Not yet implemented:** boot-time provisioning, account/owner creation, Passid linking, atomic updates, rollback, secure boot, disk encryption defaults, hardware-independent installer, or resource management. A login prompt does not mean a usable administrator account has been provisioned. Do not deploy this prototype as a production server.

## Source layout

```text
boot/                     Boot and firmware integration
build/                    Build scripts
crates/detect/            Hardware discovery
crates/init/              Offline first-installation prototype
docs/architecture/        System design and roadmap
kernel/                   Kernel configs and patch space
packages/release/         Release metadata Debian package
system/base/identity/     Console banner
system/profiles/          Future configuration profiles
system/security/          Future security policies
system/updates/           Future update management
tests/                    Boot, integration and performance tests
images/                   Generated output, ignored by Git
```

## Development

The current build process runs on a Debian 13 development host. Install appropriate versions of Rust, `mmdebstrap`, `dpkg-deb`, `e2fsprogs` and QEMU for your host.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Create the package output directory before packaging:

```bash
mkdir -p packages/dist
dpkg-deb --build packages/release packages/dist/linos13-release_1.0.0_all.deb
./build/image.sh
```

The builder detects the host package architecture with `dpkg --print-architecture`, unless `linos13_ARCH` is set. Selecting an architecture is not equivalent to full cross-compilation or universal boot support. Builds are saved under `images/server/builds/<arch>/<build-id>/`; existing build directories are never overwritten automatically.

For the successful ARM64 development build, run an isolated QEMU boot test:

```bash
qemu-system-aarch64 \
  -machine virt -cpu cortex-a76 -smp 4 -m 2048 \
  -nographic -snapshot \
  -kernel images/server/builds/arm64/20261025.3/boot/kernel \
  -initrd images/server/builds/arm64/20261025.3/boot/initramfs \
  -drive file=images/server/builds/arm64/20261025.3/disk.ext4,if=none,format=raw,id=rootfs \
  -device virtio-blk-device,drive=rootfs \
  -net none \
  -append "root=/dev/vda rootwait rw console=ttyAMA0 systemd.unit=multi-user.target"
```

The VM is disconnected from the network, and `-snapshot` prevents guest changes from writing back to the disk image. No insecure default credentials are created.

To try the standalone offline initialization tool:

```bash
mkdir -p tests/init-fixture
cargo run -p init -- --root "$PWD/tests/init-fixture"
```

This only creates/preserves `etc/hostname` in that test directory. It is **not** a systemd unit or live-boot initializer yet.

## Direction

linos13 aims for automatic hardware-aware setup across supported ARM64 and AMD64 machines, Debian package compatibility, safe scheduled updates, verified rollback, local-first device ownership, optional Passid integration and workload-aware system resource management. Administrative freedom should remain available with explicit safeguards.

See [Architecture](docs/architecture/overview.md) and [Roadmap](docs/architecture/roadmap.md).

Project: [phe-rus/linos13](https://github.com/phe-rus/linos13) · Contact: <pherus@pherus.org>
