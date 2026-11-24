# Server security maintenance before beta

**Status**: Proposed
**Date**: 24 November 2026

## Context

The Server assessment and the repaired OS scan exposed a release requirement beyond application fixes. The AMD64 compatibility image scan at source `307defd` reports 85 package and advisory matches across 26 unique advisory identifiers. No reported row lists a fixed stable package version. The [triage report](../reviews/2026-11-24-server-security-triage.md) records vendor sources and uncertainty. This is not evidence that every listed binary has an exploitable code path, nor that missing package fixes make the image safe.

The owner requires CI qualification before a fresh local candidate, then physical support and beta gates. Desktop and broader Assets/Store work are deferred. No advisory exception, distribution migration or package maintenance pipeline is authorized by this proposed decision alone.

## Options

1. Keep the current gate and wait for Debian stable fixes. This preserves the existing maintenance model but leaves beta blocked for an unknown period. A package refresh alone is not currently sufficient.
2. Keep the Debian stable base and establish reviewed Linos security backports. This gives Linos responsibility for exact source inputs, patches, package versions, regression evidence and the lifetime of those patches. It is a significant release engineering commitment, not a version override.
3. Change the software base. This would require requalification of setup, authentication, packaging, kernel integration, recovery, updates and supported devices. It is not recommended as a quick scan repair.

## Recommended decision

Choose option 2 if you want to progress independently of the stable vendor schedule. Retain the raw strict scan and its reports while developing the maintenance path. Do not introduce broad ignore lists, ignore unfixed findings, lower severity gates or treat a vendor priority label as proof that a binary is unaffected.

First establish a per candidate inventory and applicability record. For each advisory, name the source and binary packages, installed version, code path, build options, device configuration and relevant vendor statement. Unknown applicability stays open. A demonstrated absent vulnerable implementation may justify a separately reviewed applicability statement. Default configuration alone does not prove absence if a supported setting or later package installation can expose it.

For an affected binary, obtain a vendor or upstream fix with authenticated source provenance, review it against the stable source, build a uniquely versioned package for each supported architecture and test the vulnerable behavior and surrounding functionality. Do not import unrelated testing/unstable binary packages into the image as a shortcut. A fix unavailable even upstream needs upstream coordination or a separately reviewed change, not an invented version number.

## Acceptance criteria

AC-1. Retain raw scanner reports, image hashes, source commit, package inventory, advisory database identity and tool version for both compatibility and native images. Scanner failure remains distinct from findings. Custom kernel vulnerability coverage is a separate source and configuration review.

AC-2. Every resolution is traceable to a vendor fix, reviewed backport or proven absence of the vulnerable implementation on the exact candidate. Keep open and unknown findings visible. Check revocation when package version, configuration, supported capability or source changes.

AC-3. Patched packages have authenticated source inputs and patches, deterministic recipe, unique version, dependency/provenance inventory and AMD64/ARM64 regression evidence. Signing and repository trust have an explicit owner, key lifecycle and rollback policy before publication.

AC-4. Machine readable applicability or fixed statements, if adopted, are scoped to exact package identities and candidate evidence. Validate scanner behavior with negative cases, changed versions and stale evidence. Keep the unfiltered report as well as the adjudicated result. This is a further implementation decision, not an enabled exception in this change.

AC-5. Run the final CI, fresh local image qualification, real device matrix and documented recovery gates after changes. No beta announcement precedes those results.

## Consequences

Option 2 makes Linos a package security maintainer in addition to an integrator. It requires ongoing patch monitoring and rebuilds after beta. The existing scope does not provide release signing infrastructure or actual physical test devices. Those remain necessary inputs, not guarantees inferred from green Rust tests.

## References

The triage report links the Debian tracker entry for each of the 26 advisory identifiers. [Trivy root filesystem scanning](https://trivy.dev/docs/latest/target/rootfs/) describes the scan target. [Trivy local VEX files](https://trivy.dev/docs/latest/supply-chain/vex/file/) documents an experimental mechanism for conveying applicability; it does not establish the truth of a claim or replace review. Its exact behavior would need validation against the pinned scanner before use.
