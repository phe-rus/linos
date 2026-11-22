# Verified updates for development images

`linos-update-image` stages a signed system image into an inactive image slot
and publishes a counted systemd-boot entry. It is a development backend, not an
installer for physical disks. It accepts a private workspace of regular files,
never mounts filesystems and never opens a block device.

Rust owns validation, copying, journal transitions and boot entry publication.
Debian's OpenSSL supplies Ed25519 verification and SHA-256. There are no new
Cargo dependencies or Python tools.

Managed development guests also use `linos-update-health` and Debian's native
`systemd-bless-boot` helper from the `systemd-boot` package. The image builder
includes that package for AMD64. The Rust health tool and service are currently
injected only into the signed disposable test images, not the normal Server
package.

## Signed image payload

This format is separate from the older format 1 offline tar installation payload.
A format 2 directory contains `system.img`, `kernel`, `initramfs`, `manifest`,
`SHA256SUMS` and `SHA256SUMS.sig`. Its exact manifest fields are:

```text
format=2
edition=server
architecture=amd64
hardware_profile=qemu-amd64-uefi
release=2
build=your-build-id
system_bytes=3221225472
kernel_bytes=<exact byte count>
initramfs_bytes=<exact byte count>
```

The release is a positive u64 and must advance the healthy slot's signed release.
The system image is bounded at 4 GiB, kernel and initramfs at 128 MiB each.
The build identifier is bounded and permits only letters, digits, dot, underscore
and hyphen. Wrong targets, duplicate fields, unsupported fields and sizes fail.

Generate checksums in this exact order:

```bash
sha256sum system.img kernel initramfs manifest > SHA256SUMS
openssl pkeyutl -sign -rawin -inkey /outside/payload/private.pem \
  -in SHA256SUMS -out SHA256SUMS.sig
```

Supply an Ed25519 public key outside the payload directory. Authentication
checks the signature over exact checksum bytes, then the signed manifest digest.
Copying is bounded by signed lengths and verifies destination hashes before
publishing state. A source file changing during copying cannot authorize
unverified destination bytes. Payload-supplied keys and unsigned input fail.

## Image workspace and commands

```text
workspace/
  image-layout                 explicit System A/B GPT partition UUIDs
  update-state                 locked, durable journal
  slots/a/                     system.img and authenticated baseline artifacts
  slots/b/                     authenticated staged candidate artifacts
  efi/linos/<release>-<transaction>-<slot>/{kernel,initramfs}
  efi/loader/loader.conf
  efi/loader/entries/linos-<release>-<slot>[+tries].conf
```

Build with `cargo build --locked --bin linos-update-image`. Commands are:

```text
linos-update-image initialize ROOT BASELINE_PAYLOAD PUBLIC_KEY ROOT_A_PARTUUID ROOT_B_PARTUUID
linos-update-image stage ROOT UPDATE_PAYLOAD PUBLIC_KEY
linos-update-image activate ROOT PUBLIC_KEY
linos-update-image status ROOT
linos-update-image layout ROOT_A_PARTUUID ROOT_B_PARTUUID DATA_PARTUUID ESP_PARTUUID
linos-update-image export-guest ROOT EMPTY_STATE_DIRECTORY PUBLIC_KEY DATA_PARTUUID ESP_PARTUUID
```

ROOT must be an absolute, caller-owned directory without other writers.
Initialization requires an empty directory and distinct UUIDs from the disposable
GPT builder. The workspace owns only image files and its EFI tree. Staging swaps
only the inactive slot after verifying private copies and flushing them. Release
specific boot assets preserve the healthy kernel and initramfs. The journal
records Staged only after those files are durable.

Activation verifies the staged transaction, rechecks candidate content, preserves
the healthy fallback entry and records Trial before exposing the new boot entry.
An interrupted activation can resume without starting a second journal trial.
Retries recognize firmware-renamed `+1-1` and `+0-2` entries and never reset them.
After publication, a durable transaction receipt also prevents recreating a
missing entry with fresh attempts. A missing published entry requires inspection.
The default glob is `linos-*.conf`, with release numbers controlling ordering.
Unknown Linos entries or altered fallback assets block activation.

The two-attempt filename scheme and fallback ordering follow systemd's
[automatic boot assessment](https://systemd.io/AUTOMATIC_BOOT_ASSESSMENT/).
The default glob follows [loader.conf](https://www.freedesktop.org/software/systemd/man/latest/loader.conf.html).

## Verification and limits

Fast checks:

```bash
cargo test --locked --lib --bin recovery --bin linos-update-image --test update_image
cargo clippy --locked --lib --bin recovery --bin linos-update-image --test update_image -- -D warnings
```

`./tests/update.sh` accepts no arguments. It uses the current local AMD64 Server
artifacts, generates disposable Ed25519 keys, initializes and stages the image
workspace, builds the provisional five-partition GPT disk and boots it through
OVMF. It withholds health approval for two B boots and checks A fallback, then
starts a fresh trial, approves B and checks two further B boots. It compares
Data's sentinel, device ID and hostname across fallback and reboots, and checks
the expected journal generations. It also checks the active image is unchanged
by staging. Large temporary files are
removed by default; serial logs remain under `target/update-test-logs/`.
`LINOS_KEEP_UPDATE_TEST=1` retains the full disposable workspace for inspection.

The boot entries deliberately mask automatic success blessing and enrollment.
Managed tests add the explicit development health policy below. EFI variable
overrides, physical power loss, Secure Boot, production keys and key rotation
are not qualified. A successful developer image test does not enable physical
updates or qualify enrollment and account migration.

## Persistent guest state and health

The protected `/usr/lib/linos/update-layout` file is included in the system image
before payload signing. It binds four distinct partition UUIDs and the explicit
`development-boot` policy. The builder mounts Data at `/var/lib/linos` and EFI at
`/efi`. Identity initialization requires any configured Data mount, so mount
failure cannot silently create a replacement identity on a system slot.

`export-guest` snapshots the activated trial into an empty state directory. The
builder places this at Data's `updates/` directory with root ownership. Its
layout, deployment and journal records bind the original healthy slot, candidate,
release, transaction and journal generation. Export never overwrites guest state.

The root-only health command accepts no path or identity overrides. It compares
kernel boot parameters against the deployment, actual mount device numbers,
PARTUUID device nodes and common parent disk. Data and EFI must be distinct,
writable whole filesystems. Native identity initialization and the serial login
service must be active. Device ID must be valid, and the persistent hostname,
slot's hostname file and running kernel hostname must agree. Managed initializer
state restores hostname from Data across slots; unmanaged initialization keeps
its previous behavior. If the Linos kernel exposes boot readiness, that state
must already be `ready`.

Before native blessing, persist and flush a health intent for the candidate
transaction. Require the firmware's boot-count path to identify this candidate,
call `systemd-bless-boot good`, verify the stable entry and flush EFI. Only then
commit Healthy in the Data journal. A reboot after blessing but before journal
commit can finish from the durable intent without blindly blessing another entry.

If the original slot boots with an exhausted candidate entry, persist rollback
intent, replay only the matching failed-trial transitions, confirm the original
slot healthy and retire the failed entry. Repeated runs do not advance the
generation again. Manually selecting the old slot before exhaustion leaves the
trial pending.

Intent publication uses a private pending file, file sync, atomic rename and
directory sync. An interrupted pending file is discarded on retry. A verified
exhausted fallback may replace a matching, unfinished good intent with rollback;
an intent for another deployment remains an error. This permits fallback after
native blessing fails before renaming the candidate entry.

The workspace and guest journals are distinct after export. The guest reconciles
its own Data journal; importing that result back into the host staging workspace
for another transaction is not implemented. POSIX account databases, home
directories and service data are not migrated by this prototype. The current
development policy demonstrates boot viability, not a production health policy
for enrolled accounts.

### First staging checkpoint, 22 November 2026

The 38 focused Rust tests and strict Clippy for the library, Recovery, image tool
and image integration tests passed. OVMF selected System B on attempts one and
two, then System A on attempt three. Both first and fallback boots reached Linos
sign-in. Serial logs show the root mounted as `vda4` for B and `vda3` for A.
The 64 MiB Data filesystem and its sentinel were unchanged.

This reused the existing `20261114.7` compatibility guest artifacts, with Debian
kernel `6.12.117+deb13-amd64`, under disposable signed release numbers 1 and 2.
It does not qualify a newly rebuilt guest or the Linos-owned kernel. The final
host-only publication receipt guard was checked by the Rust integration tests.
Logs are retained locally in `target/update-test-logs/update-qemu.UpDQa2wr/`;
the large images and disposable private key were removed after inspection.

### Guest health checkpoint, 22 November 2026

The final run passed all six OVMF boots with the current Rust initializer and
health binary injected into disposable, newly signed copies of the existing
`20261114.7` compatibility guest. The kernel remains Debian
`6.12.117+deb13-amd64`; this is not a newly qualified Linos kernel or Server image.

The failure branch withheld health approval on two B boots. A then booted,
retained the same device ID and hostname from Data, and reconciled the journal
from Trial generation 2 to Healthy A generation 5. The success branch committed
B at generation 3 using native systemd blessing, then rebooted B twice and kept
the same journal generation, device ID and hostname. Data's sentinel remained
unchanged in both scenarios. Live approval also checked the running kernel
hostname against persistent Data and `/etc/hostname`.

All 43 targeted library and initializer tests passed, including interrupted
approval, repeated health checks, partial rollback and hostname restoration.
Targeted strict Clippy, formatting and shell syntax checks passed. The normal
Server package still ships four Rust executables; the managed health service and
binary are test-image inputs, and physical deployment remains disabled.

Logs are retained under `target/update-test-logs/update-qemu.pmRzabgC/`. Large
images and disposable private keys were removed after verification. The earlier
native-blessing protocol run is retained under
`target/update-test-logs/update-qemu.jIm8CoDV/`.

The blessing operation follows the native
[systemd boot assessment interface](https://www.freedesktop.org/software/systemd/man/latest/systemd-bless-boot.service.html).

### Local qualification with bounded memory

Builders and disk-copy tests honor `TMPDIR`. On hosts where `/tmp` is tmpfs,
use a private disk-backed directory such as `/var/tmp/linos-qualification`.
The build staging directory must remain traversable by mmdebstrap's user
namespace. The update test already keeps its large workspace under `target/`.

`LINOS_QEMU_MEMORY` (MiB) and `LINOS_QEMU_CPUS` override VM resources in Server
smoke, enrollment, firmware and update tests. `LINOS_QEMU_ACCEL=kvm` selects
AMD64 acceleration when accessible; default CI behavior remains TCG. Run one
VM at a time. These settings bound the guest configuration, not aggregate host
memory or a supported minimum device specification.

`LINOS_KEEP_ENROLL_TEST=1 tests/enrollment.sh amd64` retains its private test
directory and prints its path. The flow interrupts and resumes real enrollment,
then authenticates through PAM after reboot, checks native kernel readiness when
the image manifest requires Linos, records guest memory and exercises a bounded
128 MiB tmpfs allocation. This is a basic command viability check, not a latency
or out-of-memory benchmark.

Pass that fixture's `disk.ext4` to `tests/update.sh` using
`LINOS_UPDATE_ENROLLED_DISK`. The test signs copies of that enrolled image for
both slots and seeds Data with its real ownership and identity records. Health
waits for `linos-ready.service`; unclaimed Linos kernels cannot be approved by
silently bypassing readiness. Account databases are identical fixture inputs in
both slots. This does not implement account, home-directory or service-data
migration across distinct releases. Remove retained fixture disks after testing.

AMD64 installed-kernel CI runs the same enrollment, UEFI and managed update
sequence, reusing its already verified kernel package.

### Native Linos kernel checkpoint, 22 November 2026

Fresh Server build `20261122.2` with `6.18.63-linos` passed the complete local
sequence at 768 MiB guest RAM, one vCPU and KVM. Interrupted enrollment resumed,
PAM login and SSH restrictions passed, and the real kernel reported `ready`.
UEFI boot passed. All six update boots passed: two unapproved B trials, exhausted
A fallback, fresh B approval and two healthy B reboots. Data's sentinel, device ID
and hostname were preserved. The current Rust health intent fixes were included.

Both slots were signed copies of the same actually enrolled test image. This is
native kernel and development update qualification, not account migration between
different releases. The normal Server package still excludes the development
health service and updater binaries.

Compact results are in `target/qualification/`, with detailed six-boot serial and
journal logs retained under `target/update-test-logs/`. Large disposable update
images and keys are removed by the test. Fresh native Recovery construction and
firmware boot, CI qualification and release publication remain the next gates.
