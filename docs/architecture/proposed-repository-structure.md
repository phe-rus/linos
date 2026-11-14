# Proposed Linos repository structure

This is a **target structure, not a claim that these paths all exist today**. Keep working code in place until a coherent feature migration is accompanied by tests, packaging updates and QEMU smoke results. The Rust workspace currently uses `crates/*`, and `main` remains the only development branch.

```text
linos/
├── PRODUCT.md                   # Impeccable product truth (exists)
├── DESIGN.md                    # Existing design-system record (planned: run document)
├── README.md
├── Cargo.toml                   # Rust workspace (exists)
├── Cargo.lock
├── crates/
│   ├── console/                 # Current Linos Server Home (exists)
│   ├── login/                   # Local sign-in (exists)
│   ├── enroll/                  # First boot (exists)
│   ├── init/                    # Device provisioning (exists)
│   ├── detect/                  # Hardware detection (exists)
│   ├── recovery/                # Recovery CLI (exists)
│   │
│   ├── platform/                # Proposed shared Linos service contracts
│   ├── hardware/                # Proposed device capabilities and profiles
│   ├── policy/                  # Proposed permissions and resource profiles
│   ├── lifecycle/               # Proposed app and service lifecycle contracts
│   ├── updates/                 # Proposed authenticated update orchestration
│   ├── store/                   # Proposed Linus store service/domain
│   ├── assets/                  # Proposed shared Assets domain
│   ├── ui/                      # Proposed native Linos UI primitives
│   ├── sdk/                     # Proposed developer-facing Rust SDK
│   └── run/                     # Proposed single-file Rust development workflow
│
├── system/                      # Current OS defaults/services/packages (exists)
│   ├── base/                    # Current init/login/identity assets
│   ├── profiles/                # Proposed device/edition integration policies
│   └── services/                # Proposed installed native service definitions
│
├── kernel/                      # Proposed Linos-owned kernel engineering
│   ├── README.md
│   ├── sources.lock             # Exact source version, patches, digests
│   ├── config/
│   │   ├── common/              # Baseline and security requirements
│   │   ├── editions/            # Server, Desktop, TV, Mobile policies
│   │   └── hardware/            # Target-specific config fragments
│   ├── patches/                 # Reviewed, versioned changes only
│   ├── scripts/                 # Deterministic configuration/build checks
│   └── tests/                   # Kernel config/ABI/boot/performance tests
│
├── packages/                    # Current release packaging (exists)
│   ├── release/
│   └── manifests/               # Proposed release and app manifests
│
├── build/                       # Current image, package, smoke tooling (exists)
├── images/                      # Current image target tree; outputs ignored
├── tests/                       # Proposed cross-component integration tests
│   ├── boot/
│   ├── identity/
│   ├── console/
│   ├── packages/
│   ├── hardware/
│   ├── updates/
│   └── performance/
│
├── tests-screenshots/           # Proposed versioned test evidence when useful
├── docs/
│   ├── vision.md                # Exists
│   ├── architecture/            # Current architecture, roadmap and status
│   ├── plans/
│   │   └── linos13-master-plan.md
│   ├── design/                  # Proposed flow research and UI contracts
│   │   ├── journeys/
│   │   ├── components/
│   │   ├── interaction/
│   │   └── accessibility/
│   ├── platform/                # Proposed system APIs and app contracts
│   ├── kernel/                  # Proposed support matrix and benchmarks
│   ├── releases/                # Proposed release notes and qualification
│   └── showcase/                # Proposed GitHub-native screenshots/evidence
│
└── .github/
    ├── workflows/               # Existing CI plus planned qualification jobs
    └── ISSUE_TEMPLATE/          # Proposed reproducible bug/UX reports
```

## How the parts relate

```text
                             Linos product principles
                               PRODUCT.md / DESIGN.md
                                         │
             ┌───────────────────────────┼──────────────────────────┐
             │                           │                          │
    Linos-owned experiences      Native app contracts       OS/platform policy
    Home · Assets · Settings     Rust SDK · UI primitives    Hardware · services
    Resources · Linus store      Runtime · permissions      Kernel · resources
             │                           │                          │
             └───────────────────────────┼──────────────────────────┘
                                         │
                    Linux compatibility and upstream software
                     (inputs and mechanisms, not product logic)
```

## Suggested crate boundaries

- **`console`** owns terminal presentation, view state, navigation and input. It should not own package transaction policy, hardware compatibility logic or secrets.
- **`hardware`** owns typed, testable detection and capability descriptions, not visual rendering. Current `detect` should be reviewed before adding a second overlapping crate.
- **`policy`** owns declarative, versioned resource and privilege decisions, with explicit dry-run and audit records. Never place a global automatic `sudo` bypass in Home.
- **`platform`** owns stable cross-edition API/types, not a giant all-purpose crate. Split only where functionality is demonstrably independent.
- **`store`** owns software metadata, provenance, dependency planning, installation state and future publishing contracts. A thin view in `console` presents it. Current APT-backed `store.rs` is a starting prototype.
- **`assets`** owns file metadata, browsing and confirmed mutations with permission boundaries; UI adapters belong to each edition.
- **`ui`** provides Linos-native Rust components and design-system tokens once audited; Ratatui-specific components must not be assumed to implement future GUI rendering.
- **`sdk` and `run`** make Linos-native application development approachable; reusable APIs need stability/compatibility tests.
- **`updates`** owns release manifests, staged operations, verification and rollback, not ad hoc arbitrary package scripts.

Keep source and tests clearly separated at cross-component and end-to-end boundaries; internal Rust unit tests may remain next to implementation where idiomatic unless the repository adopts an explicit integration-only testing convention.

## Migration strategy

1. **Do not restructure during the P0 PTY bug fix.** Keep risk small until console authentication and command handling are reliable.
2. **Create interfaces and tests before extraction.** Extract `hardware.rs` and `store.rs` from `console` only when supported through shared typed contracts.
3. **Introduce `kernel/` as a new independently testable subsystem**, without deleting the functioning Debian boot path until a Linos kernel passes equivalent tests.
4. **Refactor packaging only once new manifests and artifact checks exist.** Preserve the existing image file and recovery contracts.
5. **Keep edition-specific UI separate from common policy**, so Desktop and TV do not inherit Server's terminal layout.
6. **Move one bounded feature per validated change** on `main`, with formatter, Clippy, tests, package, QEMU smoke and rollback notes.
7. **Avoid placeholder crates.** This tree is a guide to future separation, not a mandate to create empty directories.

## Names, conventions and validation

- Package and crate names: lowercase `linos-*`; Rust modules and paths: snake_case.
- Product display: **Linos**, **Linos 13**, **Assets**, **Linus store**.
- All user-facing language should be plain, specific and action-oriented.
- Prefer typed Rust integration, explicit security boundaries, consistent error objects and no silent recovery.
- Current package/image builders and CI must be updated whenever a file is extracted or installed.
- Each new native UI surface must have interaction, keyboard, accessibility, small/large terminal and error-state acceptance tests.
- Each kernel/hardware change must have evidence for supported architectures and configuration provenance.

## What not to do

Do not replace systemd/PAM/APT just to make Linos superficially independent. Independence means Linos defines verified product behaviour and compatible interfaces. Replacing mature security-sensitive mechanisms without demonstrable benefit makes the consumer experience worse.

Do not claim that changing the kernel guarantees Apple-like motion. Deliver that through coordinated input, graphics, application and kernel engineering and measured end-to-end performance.
