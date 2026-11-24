# Server status and response

**Status**: In Progress
**Date**: 24 November 2026

## Context and authorization

The owner requested a measured Server assessment, improvements, repeated CI and local image qualification before beta. Desktop and broader Assets/Store work are deferred. The baseline review records the evidence and larger remaining gaps. This bounded design implements that authorized improvement work.

## Decision

Preserve the current Home layout and explicit authorization. Drain at most 32 ready terminal events before drawing again, preserving event order and existing key handlers. Wait at most 20 ms when command output or a view result is pending, otherwise retain the existing 100 ms idle wait. This avoids a permanent faster idle loop.

Add a read only status report shared by the CLI and Home. It uses existing cancellable query policy for failed system services and native Linux readings for pressure, memory and root storage. Home loads it asynchronously. Never turn a missing check into a healthy result. Replace the unconditional server-ready greeting with a neutral invitation.

## Value sourcing

| Value | Source and derivation |
| --- | --- |
| Identity, uptime, setup | Existing overview functions, unchanged |
| Memory headroom | `/proc/meminfo` MemAvailable and MemTotal, reject missing, zero or out of range data |
| Low memory advice | Available at most 10 percent of total, a transparent heuristic rather than a forecast |
| Root space | Existing `statvfs` disk usage, available excludes reserved blocks |
| Low storage advice | Available at most 5 percent of total, a transparent heuristic |
| Pressure | `/proc/pressure/{cpu,memory,io}`, `some avg10` percentage, finite 0 through 100 only, missing/malformed is unavailable |
| Failed services | Successful bounded `systemctl --failed --type=service --no-legend --plain --no-pager`, show at most 12 lines plus total; nonzero exit/cancellation/timeout is unavailable |
| Support and certainty | Literal scope text: readings are a snapshot, no hardware certification, workload safety assessment or failure prediction |

## Requirements

AC-1. Queued keys keep existing order, cancellation, secret masking and confirmation semantics. Each input batch is bounded so output and resource refresh still run.

AC-2. Existing terminal interaction and ordinary administrator guest tests pass. Before/after latency and idle CPU observations are recorded without inventing a speedup.

AC-3. Both status surfaces report actual failed services or explicitly unavailable checks. They must not claim overall health from a blank or failed query.

AC-4. Missing, malformed and out of range readings are unavailable. Memory and disk advice names the observed condition and offers inspection, never automatic mutation. Pressure percentages are identified as waiting time, not CPU utilization.

AC-5. No new privileged helper, policy adaptation, background daemon, database, external dependency, predictive model or hardware certification is introduced. Status refresh is explicit navigation/CLI invocation; it is labeled as a snapshot.

## Build and verification

Implement the small report module, reuse the existing query and disk interfaces, and retain the current view layout. Cover parsing/unknown data, failed service inspection and absence of blanket health claims. Run workspace tests, strict Clippy, terminal guest checks and a repeated benchmark. Rebuild the package and then a unique native image for candidate qualification. Physical support and beta publication remain separate gates.

## Consequences

This improves explanation and measured interaction, not fleet management, backup/restore, cgroup policy, real board support or production recovery. Those remain explicit assessment gaps. Serial transport and host scheduling limit performance conclusions; candidate tests must use the final source and artifact hashes.
