# The vision for Linos

**One operating-system family. Different devices. A system that feels like yours.**

Linos began with a question: what would Linux feel like if installing it, signing in, managing it, updating it, recovering it and using it were designed as parts of one experience, rather than a collection of unrelated tools?

Linos is a Linux-based operating-system project. It is not a theme, a desktop environment, a replacement shell, or a new name for Debian. Our ambition is to build and eventually own the integration between the kernel, system services, system management, user experience, updates, recovery, identity and device-specific capabilities. We intend to remain compatible with useful Linux software while being deliberate about which distribution conventions we keep.

**Linos 13 Server is the first step, not the limit of the project.**

## The platform we're building

A future Linos device should feel recognisably Linos, while presenting the right environment for its role.

| Linos edition | Intended experience | Current maturity |
| --- | --- | --- |
| **Server** | A native terminal-first environment for setup, services, packages, storage, networking, security, monitoring and recovery | Active development preview |
| **Desktop** | A complete graphical desktop with its own interaction model and applications, using the same system foundations | Long-term design |
| **Single-board and embedded systems** | Hardware-aware boot, firmware, energy and peripheral controls appropriate to supported boards | Initial discovery and target profiles |
| **Other device families** | Future device-specific experiences built on common foundations, not a stretched desktop layout | Exploratory vision |

A universal OS does **not** mean the same interface, bootloader, driver or power policy is appropriate for every device. Shared contracts and per-device implementations matter more than uniform branding.

## Eight design commitments

### 1. The complete journey matters

The experience begins before the first login: hardware discovery, installation, recovery, owner setup and initial preferences. It continues through day-to-day use, maintenance, troubleshooting and replacement. These stages should form a coherent journey, not require users to learn separate design languages for every operation.

### 2. Human-friendly without hiding Linux

Ordinary commands, real Linux tools and administrator access remain available. Linos Home is a helpful environment, not a locked-down substitute for a shell. When a command needs special privileges, normal authorization rules still apply.

### 3. The system belongs to its owner

Local setup and administration work without an internet account. Optional Passid integration may be useful later, but the device should remain usable and recoverable when cloud services are unavailable.

### 4. Hardware-aware by design

An ARM single-board computer, an AMD64 server and a battery-powered laptop have different constraints. Linos should discover the actual hardware, display clear facts, apply only supported defaults, and avoid pretending that unidentified hardware has certified compatibility.

### 5. Performance backed by measurement

Resource management should use supported Linux facilities such as cgroup v2, CPU scheduling, I/O controls and pressure stall information. A server must not lose work because an OS aggressively pauses background processes. We will measure boot time, latency, power use and reliability before claiming improvements.

### 6. Recovery is part of the operating system

Recovery should be accessible, intelligible and tested before risky installation or upgrade operations are enabled. The long-term goal is verified updates, guarded rollback, repair and reinstall procedures that preserve user data where feasible.

### 7. One considered language of interaction

The console and future graphical experiences should share a vocabulary: calm composition, contextual navigation, legible status, explicit actions, useful progress, correct platform naming and predictable behaviour. **Linos**, **Linus store** and other names use their canonical casing. Acronyms like CPU, RAM, EFI and API may remain uppercase.

### 8. Open development with honest evidence

A screenshot is evidence of a particular running build, not proof of universal device compatibility. A unit test is not a boot test. A Debian kernel is not yet a Linos-tuned kernel. We'll distinguish implemented, demonstrated, experimentally working and proposed systems in documentation.

## What we've demonstrated

Our initial Server work has produced QEMU-bootable images, a native Cliclack first-boot setup, local account provisioning and sign-in, a Ratatui Home interface, a responsive terminal layout, a read-only Assets browser and resource usage views. Our [visual showcase](showcase/README.md) documents these with captured screenshots.

Development also includes a PTY-backed command composer, hardware discovery and **Linus store**, an early package browsing and simulation interface using Debian's dpkg/APT ecosystem. The command composer still has a documented incorrect password-state issue; the newer hardware and store views await boot verification.

These are substantial steps toward the interaction model, **not** proof of a production-ready operating system.

## What Linos must eventually own

Linos is currently bootstrapped using Debian 13 packages, systemd, PAM and Linux kernel packages. These are useful compatibility foundations. The target is to own Linos-specific policies, tested integration, release infrastructure, installation contracts, kernel configuration profiles, hardware support decisions and lifecycle maintenance.

A future kernel policy should pin and verify configuration fragments and driver support on each upgrade. It must not confuse *using* a Linux kernel with *having built* a kernel tuned and validated for Linos.

Similarly, Linus store starts with familiar package infrastructure but aims to provide a cohesive, safe management experience with permissions, transaction previews, install progress, history, recovery and, eventually, Linos-managed repositories.

## What would make Linos different?

Our ambition is not to out-feature Debian, Ubuntu or Fedora overnight. Those distributions have mature release engineering, security processes, package ecosystems and hardware coverage.

Linos is exploring the integration of an operating system with a **product-level experience across the whole device lifecycle**, beginning with a native server-first interface. It must earn its claims through reliable engineering, not just a distinctive screen.

## Read the evidence

- [Visual showcase](showcase/README.md): real captured interfaces.
- [Current engineering status](architecture/current-status.md): evidence and known limitations.
- [Development roadmap](architecture/roadmap.md): stages and release gates.
- [Architecture](architecture/overview.md): layers, boundaries and safeguards.

> **Status: development preview.** Linos 13 is not yet suitable for production server workloads or installation on important personal hardware.
