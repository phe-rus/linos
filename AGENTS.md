# Linos owner requirements

- Python is prohibited in Linos code, tooling, tests and agent task automation,
  including temporary scripts and one-off commands. This is a non-negotiable
  owner requirement. Use Rust for system implementations and shell for orchestration.
- Keep local builds and QEMU tests within the host's available RAM. Run VMs
  sequentially, limit compiler jobs, and keep large temporary images on
  disk-backed storage rather than a RAM-backed `/tmp`.
