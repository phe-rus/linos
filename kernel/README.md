# Linos kernel

**Linos** is the kernel. **Linos 13** is the operating-system release that uses Linos. Server, Desktop and future device families are OS editions and device qualifications, not separate kernels with unrelated source histories.

## Implementation and boundaries

The current Linos kernel is derived from verified upstream Linux, not a clean-room kernel and not yet a production release. It reuses Linux's scheduling, memory, filesystem, networking, virtualization, security and driver implementations. We build, configure, package, test and eventually modify this Linux-derived kernel under the Linos identity, retaining Linux license and attribution requirements.

The kernel's `uname -r` release uses a `-linos` suffix independent of the Linos OS generation number. The userspace's Linos 13 identity, editions, interfaces and applications remain in `system/`.

```text
kernel/
  source.conf              # verified upstream release candidates
  config/
    common.config          # minimum common kernel policy
    amd64.config           # amd64 QEMU boot prerequisites
    arm64.config           # arm64 QEMU boot prerequisites
  README.md

build/kernel.sh             # kernel build and temporary Server image assembly
tests/kernel.sh             # real Linos Server QEMU boot verification
.github/workflows/kernel-research.yml
```

We do not copy the entire Linux source tree into the OS repo. The selected signed upstream release is fetched into an isolated build workspace. Only reviewed original Linos patches should be added under `kernel/patches/` when they exist.

## Kernel build and release-candidate contract

```sh
# Source tree must be the exact checked-out signed upstream release tag;
# trusted maintainer keys must already be installed and fingerprint-verified.
bash build/kernel.sh build amd64 /path/to/upstream/linux /tmp/linos-amd64
# Native ARM64 builder with an ARM64 upstream checkout:
bash build/kernel.sh build arm64 /path/to/upstream/linux /tmp/linos-arm64
```

The build:
1. Selects an allowlisted upstream source tag from `source.conf`, checks the exact source commit, rejects tracked modifications and verifies its signature.
2. Applies `common.config` and the architecture-specific policy fragment, resolves Kconfig dependencies, and **fails** if required options were silently dropped.
3. Builds the Linux kernel and matching modules under the **Linos** release suffix.
4. Stages a `linos-kernel/` artifact with kernel image, final configuration, System.map, modules, provenance and SHA256SUMS.
5. Leaves the existing bootable Linos Server images and installed physical devices untouched.

This is a *release candidate*, not an authenticated or device-qualified production kernel. SHA-256 integrity checks do not replace signed release metadata or Secure Boot.

## Evidence and acceptance

The isolated kernel research workflow is configured for native amd64 and arm64 builders. Each should verify the source signature, build and stage modules, check the package hashes and boot a minimal QEMU initramfs. Each should then create a **disposable** Server image with the built Linos kernel and regenerated matching initramfs, and verify that the actual Linos Server sign-in is reached under that running kernel.

An earlier **amd64-only** kernel CI run passed source verification, kernel compilation, minimal QEMU boot and actual Linos Server boot. The new two-architecture tests must pass separately; adding them to CI is not proof of success.

Before Linos can become the default **Linos 13** kernel we still require:
- Both native architectures passing the full test, including storage/network and login compatibility.
- The regular image/UEFI builder selecting a validated Linos kernel as one versioned unit with modules, initramfs and firmware dependencies.
- Signed release manifests, UEFI boot integrity, independent recovery and rollback, and physical-device support qualification.
- Repeatable correctness, reliability, latency, throughput and power baselines against a conventional kernel.
- Actual reviewed Linos kernel changes, when measurements justify them. No scheduler/memory superiority is claimed yet.

**No percentage of kernel completion is meaningful without these independently verifiable gates.** The build-and-QEMU foundation is implemented; integrated release and hardware qualification are unfinished. Completion of this kernel foundation is a prerequisite to developing broader hardware intelligence or UI resource policies.

## Opt-in Server image integration

The normal `build/image.sh` invocation produces the existing Debian-kernel Linos 13 Server image. To test the Linos kernel **without replacing this fallback**, provide the completed signed-source Linos kernel build directory:

```sh
bash build/package.sh
LINOS_KERNEL_BUILD_DIR=/tmp/linos-kernel-out bash build/image.sh
```

The resulting `images/server/builds/<arch>/<build-id>/linos-kernel/` directory contains a separately bootable `kernel`, `initramfs`, `disk.ext4`, release/architecture metadata, and checksums. It is assembled from the same Server root filesystem, with matching kernel modules and an initramfs regenerated for that kernel. The normal `boot/` and `disk.ext4` outputs are left unchanged. A separate UEFI research image can be generated using `LINOS_KERNEL_TEST_IMAGE_DIR` with `build/media.sh`. This is not automatic production promotion, Secure Boot signing, or persistent A/B rollback.

Kernel CI exercises this **normal image builder integration** before completing its actual Linos Server QEMU and amd64 UEFI boot gates. For now, the Linos kernel candidate is explicitly selected for testing, not installed onto physical machines.

## Primary Server kernel (promoted from a verified candidate)

If `LINOS_KERNEL_BUILD_DIR` is set when building the Server image, `build/image.sh` now promotes that Linos kernel candidate to the **normal boot locations by default**. Boot kernel, initramfs, disk and packaged rootfs are selected together; the matching modules are included in the root filesystem used by the offline installer. The previous kernel's complete boot files and OS image remain in `fallback/` for manual recovery. Use `LINOS_KERNEL_PRIMARY=0` to keep a new candidate separate without promotion. No upstream source checkout is implicitly trusted: the candidate must be produced from the existing signed-source verification workflow.

```sh
LINOS_KERNEL_BUILD_DIR=/path/to/verified/kernel-build ./build/image.sh
```

Without a provided Linos kernel build, the existing Debian-based image build remains the compatibility path; this is deliberate until standalone Linos kernel distribution is available. This does **not** constitute automated rollback, signed production updates, or full removal of Debian packages from Linos 13.
