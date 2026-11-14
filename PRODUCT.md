# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

Linos is an operating-system family, not a web application. The `adaptive` value is the closest Impeccable platform classification: a shared product philosophy with device-specific experiences for Server, Desktop and future supported form factors. This label does not mean those editions already exist or that a single responsive UI should be used everywhere.

## Users

The long-term primary audience is **everyday people who want to use a device without having to become operating-system experts**. Linos should be credible for someone who has never used a terminal, and still respect the needs of advanced users, developers and system administrators.

The first active product surface is **Linos 13 Server**: people setting up and administering a local machine, virtual machine or eventually supported single-board computer, including users who are not experienced Linux administrators. A system must not become less coherent or less approachable because its edition is Server.

Future audiences include people using laptops, desktops, televisions, mobile devices and other supported hardware. These future editions are product intentions, not currently validated shipping products.

## Product Purpose

**Linos rethinks Linux as a complete consumer operating-system experience.**

Linux already has tremendous capabilities. Linos exists to make those capabilities genuinely approachable, predictable, responsive, comfortable and desirable for people to use. It aims to remove the expectation that someone must assemble, configure or learn the internals of a Linux distribution to have an experience comparable in care to mainstream consumer systems.

Linos is not intended as a Debian spin or a Linux distribution whose defining difference is a theme, installer or package collection. It owns the ambition of its operating-system structure, interaction models, kernel and runtime policies, application integration, native development experience, updates, recovery and complete lifecycle.

**Linos 13** is the first generation, with Server as the starting edition and a broader family of device experiences planned.

**Product promise:** *Made to feel at home.*

Success means that people can pick up a supported Linos device, understand what it offers, complete everyday tasks without specialist instruction, trust it to behave consistently, and willingly choose and recommend it. Advanced capability remains available without being a prerequisite for basic use.

## Positioning

The intended differentiation is not a feature checklist or a claim that Linos already exceeds existing operating systems. It is **consumer-grade attention to the entire system journey on an open Linux foundation**: setup, ownership, navigation, software installation, permissions, resource use, visual/input response, recovery, updates and everyday interaction are intended to feel like one deliberately designed product.

Apple's care for integrated hardware-software interaction, UI physics, wording, defaults, responsiveness and continuity is an explicit quality reference. Linos is not copying Apple's appearance, APIs or closed ecosystem.

Stable upstream repositories initially supply software, **not the Linos product architecture**. Third-party applications will ultimately have explicit native/integrated/compatible levels so the operating system can deliver a consistent experience without falsely claiming every unmodified Linux binary looks or behaves the same.

## Operating Context

- Device discovery, image boot and recovery.
- Guided offline first boot and local account ownership, followed by authentication and Home.
- Server overview, services, network status, logs, filesystem browsing, hardware details, resources, settings and software discovery.
- Native Linux shell compatibility for advanced work, alongside a simpler Linos-first management experience.
- Rust development, reproducible releases, multi-architecture CI, QEMU boot testing and eventual supported-device tests.
- Future native applications, integrated software distribution and developer publishing through **Linus store**.
- Future **Assets** file manager based on the strengths of Finder and Windows File Explorer, developed with its own Linos behaviour.
- Physical devices with different input methods and power/performance capabilities. Cross-device consistency means common product contracts, not identical layouts.

## Capabilities and Constraints

**Demonstrated in supplied development evidence:**

- amd64/arm64 QEMU Server boot paths.
- Cliclack first-boot setup, local enrollment and PAM-backed login.
- Rust/Ratatui Home, slash navigation, native Shell switching, read-only Assets browsing and system resource displays.
- Terminal resizing in the observed amd64 development environment.
- The last reported full workspace quality check passed 54 Rust tests at commit `4db71a0`; that does **not** validate later source changes.

**Partly implemented or source-only:**

- Persistent PTY command composer, with a reported false `Password (hidden)` state that makes ordinary commands unusable in some sessions.
- Read-only hardware reporting and dpkg/APT-backed Linus store inventories, search and simulated operations in newer source, not yet verified in a newly booted guest.
- Settings are mostly informational; safe editable preferences need a proper authorization/confirmation model.
- The initial independent Recovery and installer work is not a verified production-safe physical installation workflow.

**Architectural constraints and open decisions:**

- The current development images depend on Debian packages and Debian-packaged kernels. Linos-owned kernel source/configuration, stable release policy and native application contracts are targets, not achieved facts.
- Rust is the preferred language for Linos-owned system components and the intended first-class native application platform.
- A simple single-file Rust compile-and-run workflow is planned, with explicit compilation, caching, dependency and trust rules. Rust source is not an interpreted format.
- Motion, input, graphics, scheduler, memory and I/O policies must be tested together; kernel tuning alone cannot deliver fluid visual behaviour.
- A supported hardware matrix is required before physical-device installation claims.
- Device ownership works locally. Cloud identity, including optional Passid, must not be required for ordinary use.
- Package installation, privileged changes and upgrades must maintain explicit permissions and recovery guarantees. Admin enrollment is **not** permission to run all commands as root.
- Exact native UI framework, app package/runtime contract, graphics stack, rendering budgets, supported release targets and kernel configuration are **open architecture decisions** pending prototypes and measurements.

## Brand Commitments

- Canonical name: **Linos**, generation **Linos 13**. Product line names use natural casing, including **Assets** and **Linus store**.
- Tagline: **Made to feel at home.**
- Clear, human wording; no shouting headings or needless technical jargon in end-user surfaces.
- Calm, considered, perceptibly responsive experiences that preserve the user's context and control.
- No superficial imitation of macOS, Windows, iOS or Android. Learn from the quality of their interactions, not their skins.
- Existing repository hero artwork and the actual captured development screenshots are evidence and brand assets; new claims, devices, capabilities or testimonials must not be fabricated.
- Treat the distinction between **technically usable** and **comfortable for everyday consumption** as central to product reviews.

## Evidence on Hand

- [Vision](docs/vision.md) records the confirmed consumer-first purpose.
- [Architecture](docs/architecture/overview.md) and [roadmap](docs/architecture/roadmap.md) record the technical reality and long-term platform targets.
- [Engineering status](docs/architecture/current-status.md) distinguishes demonstrated features from source-only features and known blockers.
- The supplied 14 November 2026 QEMU screenshots demonstrate first-boot setup, sign-in, Home, Assets, Settings, Resources and adaptive terminal geometry. **Those screenshots have not yet been committed as a GitHub-native showcase.**
- User-supplied Rust build logs show the 54-test baseline passed at `4db71a0`.
- The latest observed in-guest command session still displayed a false password state during ordinary shell use.
- No demonstrated production release, native graphical app framework, independent Linos kernel configuration pipeline, developer store publishing workflow or validated physical-device matrix exists yet.

## Product Principles

1. **Design for the non-specialist first.** A person should not need to understand Linux internals to use an ordinary feature.
2. **Care is end-to-end.** Visual design, wording, input response, timing, privileges, errors, updates and recovery are part of a single user experience.
3. **Complexity is the system's responsibility.** Provide sensible, safe defaults while retaining transparent advanced control.
4. **One product, appropriate device experiences.** Share clear conventions and contracts across editions; respect hardware, workload and input differences.
5. **Evidence before release claims.** Measure performance, test real tasks and keep built, tested, demonstrated and planned statuses distinct.

## Accessibility & Inclusion

Linos is meant for people with varied technical skill, input devices and accessibility needs. Native interfaces should support keyboard navigation, readable status and errors, terminal resizing, usable focus states, screen-reader-compatible semantics where supported, reduced-motion preferences and non-color-only status indication. These are design requirements, **not a claim of current accessibility compliance**.

Device-specific accessibility acceptance criteria and testing tools remain to be defined before alpha and beta release gates.
