# Repository structure decision

The earlier multi-package, multi-layer proposal is superseded by the single-package Linos source layout described in [repository-layout.md](repository-layout.md). This document is retained only so existing links resolve.

For rationale, consult the actual root `Cargo.toml`, `system/`, `hardware/`, `kernel/`, `build/`, and `tests/`. Do not recreate separate Rust packages for each console or boot feature without an independently justified runtime or security boundary.
