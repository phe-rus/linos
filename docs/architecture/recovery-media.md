# Recovery media with offline Server payload

Two bootable-product responsibilities remain separate:

- Server builds supply the installation payload with `edition=server`.
- Linos Server Recovery provides the CLI recovery environment and may embed
  that payload to support installation without a network connection.

`build/recovery-image.sh` now accepts `LINOS_INSTALL_PAYLOAD_DIR` only as an
**opt-in build-time input**. It verifies the payload SHA-256 list, checks
edition and architecture, and copies the files into:

```text
/usr/share/linos/install/
├── rootfs.tar.gz
├── kernel
├── initramfs
├── manifest
└── SHA256SUMS
```

When no payload directory is supplied, Recovery remains a small standalone
diagnostic image. The media CI job tests both forms.

`linos-recovery payload` validates the embedded development manifest and
checksums, then reports that **authenticity is not verified and installation
is disabled**. It does not extract or execute the payload.

The `installer-media` GitHub Actions job builds Server, packages its
offline payload, embeds it in Recovery, checks its presence in the recovery
filesystem, and boots that media in AMD64 QEMU/OVMF.

## Unfinished

- Signed payload and key-rotation policy.
- Rust installer deployment backend and explicit user authorization.
- Full boot-source detection for mapped and encrypted block devices.
- Persistent Data provisioning, Recovery installation, System B activation.
- Hardware-specific firmware validation and complete first-use setup.

Local installation must remain possible offline. Passid and passme linking
are optional, with exact capitalization and identifiers preserved.
