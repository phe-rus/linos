//! Test only ephemeral keys. No private material is persisted or logged.
use rustix::fs::{MemfdFlags, memfd_create};
use std::{fs::File, io::Write, os::fd::AsRawFd, process::Command};

fn output(args: &[&str]) -> Vec<u8> {
    let output = Command::new("/usr/bin/openssl")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Native test signature operation failed"
    );
    output.stdout
}
fn snapshot(bytes: &[u8]) -> File {
    let mut file =
        File::from(memfd_create("linos-signature-fixture", MemfdFlags::CLOEXEC).unwrap());
    file.write_all(bytes).unwrap();
    file
}
fn path(file: &File) -> String {
    format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd())
}

pub(super) struct Signer {
    private: File,
    pub public: Vec<u8>,
}
impl Signer {
    pub fn deployment(&self) -> super::deployment::OfficialManifest {
        let mut text = format!(
            "kind=linos-deployment\nformat=3\norigin=ci\nbase_manifest_sha256=none\nplan_sha256=none\nsource_sha={}\nbuild_id=fixture\nrelease=1\narchitecture=amd64\nhardware_profile=qemu-amd64-uefi\nkernel_release=test\nconfig_schema_min=1\nconfig_schema_max=1\n",
            "a".repeat(40)
        );
        for name in [
            "system",
            "kernel",
            "initramfs",
            "inventory",
            "applications",
            "services",
            "accounts",
            "extensions",
        ] {
            text.push_str(&format!(
                "{name}_bytes=4096\n{name}_sha256={}\n",
                "b".repeat(64)
            ));
        }
        let mut sums = format!(
            "{}  manifest\n",
            super::deployment::sha256(text.as_bytes()).unwrap()
        );
        for name in super::deployment::ARTIFACT_NAMES {
            sums.push_str(&format!("{}  {name}\n", "b".repeat(64)));
        }
        let signature = self.sign(b"LINOS-DEPLOYMENT-V3\n", sums.as_bytes());
        super::deployment::OfficialManifest::authenticate(
            text.as_bytes(),
            sums.as_bytes(),
            &signature,
            &self.public,
        )
        .unwrap()
    }
    pub fn recovery(&self) -> super::recovery::OfficialRecovery {
        let mut text = format!(
            "kind=linos-recovery\nformat=1\nsource_sha={}\nbuild_id=fixture\narchitecture=amd64\nhardware_profile=qemu-amd64-uefi\nkernel_release=test\n",
            "a".repeat(40)
        );
        for name in [
            "recovery_img",
            "kernel",
            "initramfs",
            "usb_uki",
            "efi_loader",
        ] {
            text.push_str(&format!(
                "{name}_bytes=4096\n{name}_sha256={}\n",
                "b".repeat(64)
            ));
        }
        let signature = self.sign(super::recovery::DOMAIN, text.as_bytes());
        super::recovery::OfficialRecovery::authenticate(text.as_bytes(), &signature, &self.public)
            .unwrap()
    }
    pub fn new() -> Self {
        let private = snapshot(&output(&["genpkey", "-algorithm", "ED25519"]));
        let public = output(&["pkey", "-pubout", "-in", &path(&private)]);
        Self { private, public }
    }
    pub fn sign(&self, domain: &[u8], bytes: &[u8]) -> Vec<u8> {
        let input = snapshot(&[domain, bytes].concat());
        output(&[
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            &path(&self.private),
            "-in",
            &path(&input),
        ])
    }
}
