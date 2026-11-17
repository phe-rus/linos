# Linos Kernel: policy and implementation

**Linos Kernel 1.0.0** is maintained within the `kernel/` directory on
the repository's **only development branch, `main`**. The operating system
is **Linos 13**. They have separate versions and release artifacts.

## What Linos Kernel actually is

Linos Kernel is currently a **downstream, source-built Linux kernel** using
a signed, pinned upstream Linux stable source with Linos-maintained
configuration. It is not an independently written kernel, nor does adding
`-linos` to its release string make it one. The project must earn a
distinct identity through tested behavior and maintained source patches.

### First kernel commitments

- **Secure by default:** Landlock application confinement, Yama ptrace
  controls, hardened userspace copying, source fortification, and unprivileged
  BPF disabled by default. The build must fail if any required policy does
  not survive Linux Kconfig dependency resolution.
- **Repairable by design:** serial and framebuffer consoles remain usable
  when the graphical interface is broken. Linos does not make basic recovery
  depend on its graphical runtime.
- **Compatible and inspectable:** preserve standard Linux syscall and
  userspace compatibility, upstream attribution, source provenance,
  a verifiable kernel configuration, and package checksums.
- **Practical hardware support:** AMD64 and ARM64 targets, virtio GPU, input,
  storage, filesystem, and console support, with QEMU qualification.
- **Host isolation:** kernel builds create artifacts in `kernel/build`
  and `kernel/dist`. No build script installs into the development
  host's `/boot` or `/lib/modules`.

These are initial policy choices, not proof that they work on every machine.
The CI build and QEMU boot test must pass before a version is released.

## Roadmap for genuinely Linos-specific kernel work

Review and implement upstream-compatible patch series for Linos system
behavior and resource-control defaults. Every patch must specify the
problem, exact changed behavior, test coverage, upstream impact,
and regression/recovery plan. Prefer upstreaming general improvements.
Do not invent a kernel subsystem merely for branding.

## One-branch release flow

Normal commits on `main` do **not** compile the kernel. A push touching
kernel-related files can register a skipped workflow; expensive jobs run
only if the **HEAD commit message** includes `kernel:release`.

That one release run calls separate reusable workflows: build both
architectures once, boot the resulting packages in QEMU, and publish
only after both pass. A successful release automatically creates
`linos-v1.0.0` or a later version from `kernel/source.conf`.

No `kernel` branch, no merge request, and no manual release tag.

This pipeline is separate from Linos 13 OS CI. Do not use
`kernel:release` until the kernel policy has been locally
validated or reviewed for qualification.

## First Linos-owned kernel service

The reviewed source overlay under `kernel/overlays/` adds a privileged
boot-readiness acknowledgement at `/sys/kernel/linos/boot_state`. It begins
as `starting`, and a local process with `CAP_SYS_BOOT` can write `ready`.
The state can be read without privilege. It does not claim rollback, watchdog
recovery or attestation. QEMU qualification must test both states using the
*packaged kernel*, not merely check its version string.

The signed upstream checkout remains unmodified: the build creates a
separate temporary Git worktree for applying Linos source additions.
Pressure stall reporting and cgroup controllers are required for future
measured resource management, not claimed to automatically improve latency.

## Qualification without publication

A commit containing `kernel:build` triggers the same signed-source kernel
build and packaged-kernel QEMU boot checks, but does **not** create a tag or
publish a GitHub Release. `kernel:release` triggers the full qualified
publication process only when the kernel is demonstrably ready.
