# Kernel

Linos 13 is a Linux operating system, built as a coherent Linos product. Its first research kernel uses verified upstream Linux source, while Linos owns hardware qualification, the boot contract, releases, interface policies and ultimately independently engineered improvements. It is **not** a newly written kernel and is **not** a production kernel yet.

## Minimal source structure

- `build.sh` — research kernel build. No host installation or device writes.
- `README.md` — kernel contract and first engineering milestones.

Add directories such as `patches/`, `config/` or `tests/` **only when real patches, configuration policies or tests exist**. Hardware support is described in `hardware/` as reusable device profiles and driver/firmware dependencies. Existing Rust console, authentication and recovery code stays where it is until a tested migration exists.

## Initial Linux baselines (16 November 2026)

- Research: upstream stable tag `v7.3.4`.
- Qualification comparison: upstream LTS tag `v6.18.63`.
- Existing working Server fallback: the Debian-built 6.12 kernel installed by `build/image.sh`.

Those are distinct tracks. The latest source is not automatically faster, more compatible or safe for every device. Versions must be advanced deliberately and benchmarked.

## Reproducible research build (not a release)

Bring an upstream Linux git checkout of the exact release tag and configure GnuPG with a **verified trusted upstream maintainer signing key**. The builder refuses tags without a valid trusted signature and refuses a mismatched checkout. Do not fetch and automatically trust whatever key the download server returns.

```sh
# In a separate upstream Linux checkout already at v7.3.4:
./kernel/build.sh amd64 /path/to/linux /tmp/linos-kernel-amd64

# Optional LTS comparison, with source checkout at v6.18.63:
LINOS_KERNEL_RELEASE=6.18.63 ./kernel/build.sh amd64 /path/to/linux-lts /tmp/linos-kernel-lts
```

The build produces a kernel image, modules build output, resolved `.config`, and `provenance.txt`. Output is isolated outside the upstream source and the Linos repository. ARM64 requires a suitable native or cross-compilation toolchain; the first script assumes native compilers and does not yet configure cross-compilation. It does **not** install modules in the root filesystem, produce an initramfs, make an EFI image, sign a release, or pass boot tests.

### Kernel acceptance gate

1. Verify source signatures, source revision, complete resolved configuration, toolchain and module compatibility.
2. Boot the kernel with an assembled Linos initramfs and matching modules on disposable QEMU amd64/arm64 disks.
3. Compare networking, storage, logging, local sign-in, recovery and resource behavior against the existing Server image.
4. Measure correctness, latency distributions, CPU, I/O, memory and regression under repeatable workloads.
5. Only then allow the image builder to opt into the Linos-compiled kernel. Preserve the Debian-kernel route until the new path passes.
6. Record verified release artifacts with a signed manifest and rollback-compatible boot assets.

## Boot media contract for PCs

The user-facing USB shall have **two folders at the root**, as requested:

```text
FAT32 USB root/
  EFI/
    BOOT/
      BOOTX64.EFI
    Linos/
      ... signed UEFI boot configuration and image ...
  linos.pherus.org/
    ... signed installation/recovery payload ...
```

The contents above are a **design target**, not the current output of `build/uefi.sh`. This repo currently uses a systemd-boot QEMU UEFI image with a kernel/initramfs stored under `/linos` on the ESP. Avoid claiming the two-root-folder copy-to-USB experience already works.

For x86-64 PCs, FAT32 copying can be supported once the EFI entry is tested with UEFI firmware and the payload fits the filesystem's individual-file limit (< 4 GiB); otherwise split payload files or use a second partition without changing the two-folder user-facing entry. ExFAT is not a generic UEFI boot guarantee.

A Unified Kernel Image (UKI) can embed the signed kernel, initramfs and boot metadata; signed boot assets must not accept arbitrary replacement kernel modules or unsanctioned configuration in supported builds. Developer Mode changes diagnostic access, not secure-boot authority.

### Installed layout

The existing planned installed disk contains EFI, Recovery, System A, System B and Data. The **system** is protected and omitted from normal file-manager views; power-user and developer modes expose inspectable paths through authorization, without making protected system data writable by default. No user data is stored in a system slot. Boot checks and rollback must be validated before physical installations are enabled.

### Boot experience

Cold boot -> firmware -> verified bootloader/UKI -> kernel/initramfs -> system and devices -> Linos sign-in/Home. Present a Linos logo using the repository's licensed/approved branding assets during firmware/early-boot where supported, then transition without flickering into edition-specific sign-in. Quiet boot is a presentation policy: logs remain available via developer/recovery diagnostics and never hide fatal boot errors. Server retains serial and console visibility when needed. No borrowed Apple art is used.

## Mobile, TV, car

- **TV:** per-device boot chain, remotes, codecs and power qualification; may not implement UEFI.
- **Mobile:** supported device-specific bootloader and verified-boot requirements; device recognition can use USB modes, hardware reports and signed board profiles. An on-device or host-assisted wizard offers network-based driver discovery **only when compatible boot/firmware and trusted drivers exist**. No unknown-address probing, bypass of locked bootloaders, firmware guessing or writes to unqualified devices. No honest engineering team can guarantee *zero* bricking risk on every smartphone, especially across vendors. Until proven, mobile mode is read-only detection, capability planning and diagnostics.
- **Car:** non-safety-critical infotainment only; no powertrain/steering/braking actuation. Vehicle support requires separate qualification and OEM integration.

Hardware download must validate signed device profile, exact board/revision, driver/kernel interfaces, dependencies, authorized signing keys and recovery path. An internet connection does not guarantee a suitable driver exists. Devices can remain safely unsupported.

## App model

Debian packages initially bootstrap the OS and compatibility with existing applications. Users interact with Linos Home, Assets, Settings and Linus store, not raw APT. Developers should be able to compile portable Rust, Kotlin/Compose and React/TypeScript projects for the Linos application contract through their familiar toolchains, with adapters for platform-dependent APIs. Language alone cannot ensure an Android APK will run without Android-specific framework support.

## Future implementation

- Add an actual verified kernel build to CI, with trusted upstream signing keys pinned by fingerprint, resolved config checking and a complete module/initramfs packager.
- Introduce the two-root-folder copy-to-USB builder and UEFI QEMU boot acceptance.
- Make hardware-profile resolution an explicit safe step; track verified vs absent capabilities.
- Build the Linos graphical Desktop and preserve the existing Server boot path.
- Validate signed A/B updates and real-device support after the QEMU prototype is sound.
