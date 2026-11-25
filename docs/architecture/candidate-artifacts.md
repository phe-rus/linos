# Candidate artifacts and execution

Stage 1 implementation record, 25 November 2026. The governing direction is the
[platform execution plan](../plans/2026-11-25-platform-contracts-and-execution.md).
Local checks pass. The changed workflows have not run in GitHub Actions yet.

## Kernel selection

`kernel/candidate.conf` records the reusable kernel run and its full source SHA.
Manual native qualification can supply both fields instead. Supplying just one
fails. There is no latest artifact search or automatic replacement on expiry.
Userspace qualification calls the installed OS workflow, not the kernel compiler.

`build/fetch-artifact.sh` checks the owner repository API for the exact run, source,
completed state, main branch and artifact. Kernel reuse additionally requires a
successful kernel push workflow and successful AMD64 and ARM64 QEMU jobs. It
checks the downloaded ZIP against the API artifact SHA256 digest before unpacking.
The installed OS job then checks the package checksums, architecture/version,
kernel input provenance against the current checkout and the provenance embedded
inside the package. A userspace SHA may differ from the kernel source SHA, but the
recorded kernel inputs must still match. Both identities travel with the candidate.

Same run release qualification downloads its own packages after the kernel boot
job, since that enclosing workflow cannot be complete yet. The existing explicit
`kernel:release` gate alone enables kernel publication. Manual dispatch and
`kernel:integration` do not publish.

## Immutable Server handoff

Both compatibility CI and native integration capture `server-candidate-ARCH`
immediately after assembly and before guest tests. The artifact contains
`candidate.tar.gz` and its checksum. The sparse archive contains:

| File | Purpose |
| --- | --- |
| `candidate-manifest` | Format, development state, edition, OS source SHA, architecture, build ID, run/attempt, dirty source indication, kernel run/source/package digest |
| `kernel-provenance` | Native kernel source/config/overlay provenance, or explicit compatibility marker |
| `kernel-manifest` | Actual kernel mode/release and boot file digests |
| `dpkg-status` | Exact installed package versions, architecture and available source package metadata from the rootfs |
| `release` | Build identity from the source release file |
| `rootfs.tar`, `disk.ext4`, `boot/kernel`, `boot/initramfs` | Original candidate bytes |
| `SHA256SUMS` | Digests for every listed candidate input |

`build/candidate.sh restore` accepts only the fixed regular file set, checks both
checksum layers, source/architecture/build metadata and boot hashes, then moves a
verified temporary directory into a new destination. Existing destinations are
never replaced. Scratch storage is beside the destination on workspace storage.

The compatibility installer media job downloads this artifact and restores it at
the original build path instead of rebuilding the Server package and OS image.
Recovery itself remains an independent build. Its offline Server payload uses
the original rootfs and boot bytes. The signed payload manifest records the parent
candidate manifest digest when candidate metadata is present. Payload verification
checks all those inputs, excluding the working disk that a guest may have changed.
Full candidate restore and verification always include the disk digest.

Native qualification also has a separate development handoff job. Even when the
producer fails its security gate, this job restores the retained AMD64 archive,
builds an offline payload and UEFI media without rebuilding Server, and checks
the media in OVMF. Its result cannot change the producer failure or permit release
publication. This exercises artifact transfer while the security policy is unresolved.

This does not make current development update tests immutable release acceptance:
those tests still inject rebuilt tools into disposable guest copies. They remain
development evidence under the existing handoff and security gates.

## Retention and promotion

Kernel packages, Server candidates and vulnerability reports have 14 day CI
retention. Boot failure diagnostics retain the existing seven day policy. A
candidate is uploaded before tests, including when a later security or functional
gate fails. Scanner output records the Trivy and database identity after scanning,
alongside the candidate manifest and archive checksum. Missing or failed scans
still fail the existing security gate.

Every Server archive is development material. Downloading it does not certify a
release. There is no OS release promotion command in this slice. Future OS
publication must select a fully qualified immutable archive and publish those
exact bytes, checksums, package inventory and qualification record without a
rebuild. Existing kernel publication is still separate from manual qualification.

## Local consumption

Use the matching source checkout, an explicit completed CI run and a new workspace
download directory. Set `run` to that run ID, then use:

```sh
sha="$(git rev-parse HEAD)"
build_id="$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' system/release/usr/lib/linos/release)"
bash build/candidate.sh download-development "$run" amd64 "$sha" target/ci-candidate
bash build/candidate.sh restore target/ci-candidate amd64 "$sha" "images/server/builds/amd64/$build_id"
LINOS_QEMU_CPUS=1 LINOS_QEMU_MEMORY=1024 bash tests/server.sh smoke amd64
```

The download command explicitly accepts development artifacts from failed runs,
prints their conclusion, and still verifies repository/run/source/digest. The
manifest and qualification logs determine kernel mode and what actually passed.
For an explicitly selected compatibility candidate, the existing test launcher
also requires `LINOS_ALLOW_COMPAT=1`. A smoke boot is not full candidate acceptance.

Inspect RAM, disk and active VMs first. Run only one VM locally. CI uses one Cargo
job, one guest vCPU and 1024 MiB for OS qualification, disk backed `/var/tmp`, and
sequential scenarios per runner. Native architecture jobs have separate runners.
Kernel compilation retains its explicit four job CI cap. Native integration runs
on the same ref queue instead of overlapping. Full compatibility CI remains manual.

## Evidence and remaining gate

At base source `c1d0c0607a3f63935928379f18ffca90771c6f82`, local checks verified:

* Candidate archive round trip preserves the original bytes after guest mutation.
* Wrong source/architecture, existing destinations, changed disks/boot inputs,
  unexpected or linked archive members and corrupted archives are rejected.
* Mock API tests reject wrong kernel source, failed runs, missing boot success,
  expired artifacts and a download digest mismatch. Negative logs are retained
  under `target/candidate-tests/`.
* Real kernel run `44213747067`, source
  `cc7da5bd06835c6aa879dee5209b303ba33932cb`, still passed both kernel boot jobs.
  Downloaded AMD64 artifact `14387214314` matched ZIP digest
  `6e5fb17a11e9206ed2c60f9c29f0f7414df7085c4e98f8ce087ca2b49c92d0c2`.
  Its package digest is
  `5ff18b7550d2fd322cf0eb498def361e81d601130dee882b5b7f972ba571fd02`.
  The existing provenance verifier accepted it against this checkout and the
  embedded provenance matched. No Linux compilation occurred.
* Six kernel tooling tests and 42 library/Recovery tests passed. Shell syntax and
  Actionlint 1.7.12 passed for all workflows, with its ShellCheck integration
  disabled because ShellCheck is unavailable locally.

Stage 1 remains in progress until CI runs the changed routing at its committed
source, demonstrates the downstream media handoff, and produces a native candidate
for a verified local download and focused VM boot. No OS build, VM boot, workflow
dispatch, commit, push or publication occurred in this implementation turn.
Security policy and physical qualification remain open. Storage is Stage 2.
