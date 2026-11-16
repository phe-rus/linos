# Linos 13 kernel, hardware and release architecture

**Status:** Proposed architecture, not an implemented kernel or update mechanism. Updated 16 November 2026. This document defines the design contract before changing the booted kernel. It follows [PRODUCT.md](../../PRODUCT.md), [the architecture overview](overview.md) and [the master plan](../plans/linos13-master-plan.md).

## Scope: five device families from day one

The architecture **must account for Server, Desktop PC/laptop, TV, mobile phones, and cars now**. This is a design and compatibility requirement, not a claim that all five can currently boot Linos. Support will be enabled only for individually qualified devices.

| Family | Initial workload and interaction | Kernel/hardware concerns | Qualification gate |
| --- | --- | --- | --- |
| Server | Headless, serial, SSH, virtualization, storage and networking | Reliability, cgroups/PSI, filesystems, network, predictable maintenance | amd64/arm64 QEMU first, real boards later |
| Desktop PC/laptop | Mouse, keyboard, windows, multiple monitors, optional touch | DRM/KMS, GPU and input drivers, power and suspend, storage, latency | Bootable graphical session on named tested machines |
| TV | Remote-focused large-screen UI, media decoding and playback | DRM/KMS, video/audio codecs, HDMI/CEC where supported, GPU/SoC drivers, thermal limits | Device-specific media and remote tests |
| Mobile phone | Touch, rotation, radios, camera, sensors, suspend/resume | SoC drivers, power budget, modem/vendor integration, security, cellular, camera | Only explicitly supported phones; no generic Android-phone promise |
| Car | Infotainment, navigation, media, dashboard-adjacent apps | Automotive SoC, display, audio, vehicle data interfaces, long lifecycle, boot timing, isolation | **Non-safety-critical infotainment first**; safety functions are out of scope absent independent functional-safety engineering |

Cars are **not** simply a TV or phone layout. Linos automotive infotainment must not be able to actuate brakes, steering, propulsion, airbags or other safety-critical controls. Critical vehicle data and functionality require independently defined, verified isolation, threat modeling and applicable automotive safety/security processes. A general-purpose Linux kernel is not a safety certification.

## Architecture decision: one kernel engineering framework, multiple proven builds

**One policy source does not mean one binary for every device.** Share source provenance, security requirements, release contracts, CI, update verification and telemetry schema. Produce architecture/SoC/driver-specific kernels when necessary, not arbitrary kernels for each marketing edition.

- **Kernel source:** an explicitly pinned maintained upstream Linux longterm series, with full source checksum/signature and patch provenance.
- **Policy:** versioned required configuration symbols; common security/reliability baseline; supported architecture, device and workload fragments.
- **Hardware:** feature and firmware capabilities declared in a support manifest, compared against detected hardware and known tested results.
- **Runtime:** Rust Linos services apply edition-specific permissions, resource policies, presentation and application lifecycle **above** kernel interfaces. No assumption that kernel configuration alone produces visual smoothness.
- **Boot:** firmware/bootloader selects a compatible, verified kernel, modules, initramfs, root slot and device-tree/firmware bundle as one release.
- **Recovery:** independent authenticated recovery path allows verified rollback without overwriting personal data.

Start by rebuilding an upstream 6.12.y longterm kernel for existing QEMU targets because the current development images already boot 6.12.x Debian kernels. Evaluate newer longterm lines independently for device support, security backports and driver needs. Do not turn 6.12 into a perpetual constraint.

## Layer boundary

```text
Linos experiences: Server | Desktop PC | TV | Mobile | Car infotainment
       Native Rust framework, application lifecycle, permissions
Linos platform services: hardwared / policyd / updated / recovery
       Linux APIs: cgroup v2, PSI, DRM/KMS, input, networking, devices
Linos kernel integration: pinned source + config/patch/module provenance
       Qualified firmware, boot chain, board support and hardware
```

The names ending in `d` are **proposed roles, not existing executables or a requirement to create separate daemons prematurely**. Services should be split when their privilege and lifecycle requirements justify it.

## Proposed kernel source tree

```text
kernel/
  README.md
  sources.lock.json          # kernel source tag, digest, signing key, toolchain digest
  config/
    common/                  # security, namespaces, cgroups, core filesystems, tracing
    arch/
      amd64/                 # architecture/boot fundamentals
      arm64/
    workload/
      server/                # throughput and long-running services
      interactive/           # latency-sensitive UI workload
      media/                 # media/audio/video paths
      automotive-infotainment/
    hardware/
      qemu-amd64/
      qemu-arm64/
      rk3588/                # candidate only, requires driver/firmware validation
      pc-generic/            # only for tested x86 device classes
      <qualified-board-id>/
  profiles/                  # versioned hardware capability and support manifests
  patches/
    common/
    hardware/
  scripts/
    fetch-verify.sh
    configure.sh
    build.sh
    package.sh
    verify-config.sh
  tests/
    config/
    qemu/
    integration/
    performance/
  docs/
    symbol-policy.md
    compatibility.md
    measurements.md
```

No Linux source tarballs or untracked binary kernel images should be committed to the application repository. Store source references, reviewable patches, configuration and release metadata in Git; fetch and verify the pinned source in CI. Binary artifacts are built and distributed independently.

Build composition order:

1. Pinned upstream kernel defconfig for the architecture;
2. Mandatory common symbols;
3. Architecture and boot requirements;
4. Workload policy;
5. Exact device/firmware profile;
6. `olddefconfig`, resolution report and mandatory symbol verification.

A fragment is not necessarily the final configuration: unknown, renamed or dependency-disabled options may be silently dropped. Store the generated resolved `.config` and check every required symbol after `olddefconfig`. Fail closed on unresolved security and compatibility rules.

Do not hard-code 'desktop performance' or 'automotive real-time' as a collection of guessed scheduler flags. Compare kernel configurations and userspace profiles under measured workloads and device thermal/power constraints. All tweaks need a reason, benchmark, regression gate and fallback.

## Hardware support manifests

A profile is a contract for **a particular tested hardware class/board**, not a generic name such as 'mobile'. Example (illustrative schema only):

```json
{
  "schema": 1,
  "id": "qemu-amd64",
  "architecture": "amd64",
  "device_family": ["server", "desktop"],
  "boot": "uefi-or-direct-qemu",
  "kernel_profile": "qemu-amd64",
  "drivers": ["virtio_blk", "virtio_net"],
  "firmware": [],
  "tested": {
    "server_boot": true,
    "desktop_graphics": false,
    "suspend_resume": false
  }
}
```

The `tested` fields above are *illustrative*; release automation must fill them from actual evidence. Physical devices also require boot chain, GPU, radios, sensors, firmware licensing/provenance and power qualification. Do not imply universal compatibility from a matching CPU architecture.

## GitHub as the initial update and release origin

GitHub Actions can build, test, sign and publish Linos artifacts; GitHub Releases can host **initial downloadable payloads**. The device must validate the release independently of GitHub's transport security and tags.

Suggested download namespace (illustrative; not a live URL):

```text
GitHub repository phe-rus/linos
  Releases / linos13-development-<build>
    manifest.json
    manifest.sig
    linos-kernel-<arch>-<profile>.tar.zst
    linos-system-<arch>-<edition>.img.zst
    recovery-<arch>-<profile>.img
    SHA256SUMS
    SBOM.spdx.json
    provenance.json
```

In production, prefer a stable signed **channel manifest** endpoint (e.g. `updates.pherus.org/linos/13/stable/<target>/manifest.json`) that may reference GitHub Release assets, CDN mirrors or another hosting provider. Changing hosting must not require an operating-system update. GitHub latest-release tags and its REST API are discovery hints, not security roots.

Every signed manifest must specify: schema version, generation, edition, architecture, hardware profile, kernel and boot ABI, userspace compatibility, minimum supported updater, artifact sizes and digest, firmware/DTB/module requirements, rollback compatibility, expiry and monotonic release/version ordering. Public keys are embedded in a verified OS trust store, with documented rotation and revocation. Test signature failure, wrong device/profile, replay/downgrade, partial download and interrupted write.

### Update state machine

```text
Discover signed channel manifest
  -> Verify signature, hardware, edition, version, dependencies
  -> Download to bounded staging with resume
  -> Verify digests and artifact provenance
  -> Stage inactive System slot (and compatible kernel/boot assets)
  -> Record pending boot transaction
  -> Reboot and validate boot/service health
  -> Commit good slot or automatically roll back
```

Data lives separately from System A/B and must not be reformatted during routine updates. State/database migrations need compatibility or backup rules across rollback. On systems with very limited storage, define a distinct verified alternative and don't pretend full A/B is free.

**Kernel, initramfs, signed modules, device tree, firmware dependencies and OS image must be selected coherently.** A single uncoordinated `apt upgrade` must not replace the supported boot kernel inside a release-managed Linos slot. Debian packages are allowed as *build inputs* until the platform can enforce a controlled update boundary.

### Versioning

- `Linos 13`: product generation.
- `development/alpha/beta/stable`: independently signed channel, promotion only after gates.
- `build_id`: unique image/CI provenance, not sufficient anti-rollback protection.
- `kernel_upstream`: exact Linux source version/commit.
- `linos_kernel_revision`: Linos kernel configuration/patch revision.
- `hardware_profile_revision`: compatibility policy.
- `system_release`: signed coherent bootable artifact set.

Keep **single-source release metadata** and generate visible UI/about strings from the signed manifest and booted slot, rather than manually incrementing dates in source while CI builds another commit.

## CI and hardware validation

1. Static config and provenance verification; kernel security symbol contract.
2. Two-architecture QEMU kernel builds and boot tests first; Rust checks continue.
3. Integration tests for boot, device detection, modules, network/storage, login, Home and recovery; interactive tests via a proper serial PTY driver, not only log matching.
4. Real-time/resource measurement where representative: scheduling latency, PSI under stress, boot, memory, I/O, graphics/input frame pacing, power and thermals on supported hardware.
5. Device-family test matrices: GPU/multimonitor/suspend (PC), video decode and remote (TV), radios and battery (phone), infotainment displays and controlled vehicle-data simulation (car).
6. Signed release publication only after policy checks and required supported-device gates; retain CI logs, profiles, artifacts, test evidence, SBOMs and manifest digests.
7. Automated updater test: wrong signatures/profile rejected, interrupted download/write recovered, bad boot rolled back, user data retained, channel migration predictable.

**No unsupported claims:** CI QEMU passes do not certify physical hardware, GPU smoothness, cellular connectivity or automotive safety.

## Implementation sequence without risking current Server builds

**K0 — Specification and validation:** publish this contract, define config schema/support statuses and identify kernel source/boot boundary. Do not modify current working image builder yet.

**K1 — QEMU kernel prototype:** fetch and verify a maintained upstream LTS source, generate a resolved `.config` for amd64/arm64, package kernel/modules/initramfs, boot QEMU, compare against Debian-kernel baseline. All source provenance recorded.

**K2 — System image integration:** modify `build/image.sh` to select an explicitly validated `LINOS_KERNEL_PROVIDER=linos`; retain `debian` as a temporary, tested fallback. Verify release artifact and rootfs kernels agree.

**K3 — Signed releases and update client:** implement signed channel manifest and download checks in Rust, a dry-run update plan, then A/B staging in disposable GPT QEMU images with boot-health/rollback. Keep production updates disabled until validated.

**K4 — Hardware profiles:** qualify specific x86 PC/laptop and arm64 boards; add Desktop display pipeline tests, then TV, phone and automotive infotainment device support as hardware becomes available. Ship capabilities only on supported targets.

**K5 — Performance policy:** revise scheduling, input/rendering, GPU and power behaviour through benchmarks; version and enforce these as part of each signed release. A single kernel tuning does not constitute a polished Linos experience.

**Release rule:** no update is 'supported' unless the *exact* kernel, modules, firmware, bootloader and userspace combination has relevant target evidence and a recoverable path.

## Current repository reality

As checked on 16 November 2026, `build/image.sh` installs `linux-image-${ARCH}` from Debian. The current Server image and QEMU smoke checks are the functioning baseline. The recovery design includes EFI, Recovery, System A, System B and Data partitions, and some offline payload checksum/signature validation exists. The Linos kernel build, key distribution policy, supported-device manifests, updater, A/B activation and complete rollback workflow are **not implemented**.

GitHub is a sensible initial artifact host, but a verified Linos updater must not rely on a mutable GitHub tag, the downloadable checksum file alone or the GitHub account's continued availability.
