# Server support and qualification matrix

24 November 2026. This records evidence and gaps, not a declaration of beta readiness. Server is the only edition in scope. The owner requests homelab PCs, physical servers, Raspberry Pi and Orange Pi families. The eventual supported list must name exact configurations that passed testing.

| Target | Evidence | Current support claim |
| --- | --- | --- |
| AMD64 QEMU/KVM, virtio storage, serial console | Native kernel development boot, enrollment, Home, sudo, loopback workloads and managed update tests | Development qualification, candidate specific results in current handoff |
| ARM64 QEMU virt | CI native kernel boot and persistence; compatibility image enrollment path | Development qualification, not ARM board certification |
| AMD64 UEFI physical PC or homelab PC | QEMU/OVMF is firmware simulation, no named physical device results | Unqualified physical target |
| Physical server | No named server, storage controller, NIC, firmware or sustained workload matrix | Unqualified physical target |
| Raspberry Pi family | No named board/revision, firmware, device tree, boot media or driver qualification | Unqualified target, not every ARM64 image can boot it |
| Orange Pi family | Orange Pi 5 Plus profile exists with `validation=unverified`; other models have no qualification | Unqualified target, model specific boot and drivers required |

## What counts as a qualified physical configuration

Record model and board revision, architecture, RAM, firmware versions and boot method, kernel/provenance, storage controller/media, NIC/Wi-Fi hardware and drivers, peripherals, power supply and the exact image hash. A different board or controller is a new configuration until the support policy explicitly groups it using evidence.

Required evidence includes cold boot and reboot, safe enrollment and actual PAM login, local and SSH administration, correct device and resource reporting, networking and reconnect, storage integrity, backup and restore, sustained representative workloads, recovery access and safe update/fallback with identity, accounts and service data preserved. Test power behavior and thermal limits on the device, including recovery after interrupted work under a safe disposable test setup. Document unsupported capabilities and failures.

Do not write a physical disk from agent automation merely because it is present. The existing physical installation path remains disabled. Device qualification requires an owner designated disposable target and an appropriate boot/install path; a VM disk simulation does not authorize or qualify a physical installer.

## Beta gates

CI and local image gates must pass for one fixed candidate. The documentation must define the intended audience, supported journeys, known limitations and recovery instructions. Critical company services, a storage appliance and a virtualization host require their own workload and operational evidence. Standard Linux package compatibility does not imply those managed roles are integrated or supported.

The requested physical support gate remains open until actual device configurations are available and tested. Publishing a VM only beta would narrow the owner's stated scope and requires an explicit scope decision. No beta or production release is established by this document.
