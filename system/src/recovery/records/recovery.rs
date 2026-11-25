//! Recovery signing role is separate from deployment and transport signatures.
use super::{
    deployment::{Artifact, sha256, sha256_file, verify_signature},
    hash, invalid, ordered,
    state::decimal,
};
use std::{fs::File, io, os::unix::fs::MetadataExt};

pub const DOMAIN: &[u8] = b"LINOS-RECOVERY-V1\n";
const KEYS: [&str; 17] = [
    "kind",
    "format",
    "source_sha",
    "build_id",
    "architecture",
    "hardware_profile",
    "kernel_release",
    "recovery_img_bytes",
    "recovery_img_sha256",
    "kernel_bytes",
    "kernel_sha256",
    "initramfs_bytes",
    "initramfs_sha256",
    "usb_uki_bytes",
    "usb_uki_sha256",
    "efi_loader_bytes",
    "efi_loader_sha256",
];

/// Only authenticated manifests can be used to authorize Recovery artifacts.
pub struct OfficialRecovery {
    id: String,
    source_sha: String,
    build_id: String,
    architecture: String,
    hardware_profile: String,
    kernel_release: String,
    artifacts: [Artifact; 5],
}

impl OfficialRecovery {
    pub fn authenticate(bytes: &[u8], signature: &[u8], trusted_key: &[u8]) -> io::Result<Self> {
        if bytes.len() > 64 * 1024 || signature.len() != 64 || trusted_key.len() > 4096 {
            return Err(invalid("Recovery authentication input exceeds bounds"));
        }
        let mut message = DOMAIN.to_vec();
        message.extend_from_slice(bytes);
        verify_signature(&message, signature, trusted_key)?;
        let f = ordered(bytes, KEYS)?;
        if f[0] != "linos-recovery"
            || f[1] != "1"
            || !matches!(f[4], "amd64" | "arm64")
            || f[2].len() != 40
            || !f[2]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("Unsupported Recovery identity"));
        }
        for token in [f[3], f[5], f[6]] {
            if !token
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
            {
                return Err(invalid("Invalid Recovery identity token"));
            }
        }
        let mut artifacts = Vec::new();
        for (index, limit) in [
            3 * 1024 * 1024 * 1024u64,
            128 * 1024 * 1024,
            128 * 1024 * 1024,
            384 * 1024 * 1024,
            16 * 1024 * 1024,
        ]
        .into_iter()
        .enumerate()
        {
            let bytes = decimal(f[7 + index * 2])?;
            if bytes == 0 || bytes > limit {
                return Err(invalid("Recovery artifact length exceeds its bound"));
            }
            artifacts.push(Artifact {
                bytes,
                sha256: hash(f[8 + index * 2])?,
            });
        }
        Ok(Self {
            id: sha256(bytes)?,
            source_sha: f[2].into(),
            build_id: f[3].into(),
            architecture: f[4].into(),
            hardware_profile: f[5].into(),
            kernel_release: f[6].into(),
            artifacts: artifacts
                .try_into()
                .map_err(|_| invalid("Missing Recovery artifact"))?,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn architecture(&self) -> &str {
        &self.architecture
    }
    pub fn hardware_profile(&self) -> &str {
        &self.hardware_profile
    }
    pub fn kernel_release(&self) -> &str {
        &self.kernel_release
    }
    pub fn source_sha(&self) -> &str {
        &self.source_sha
    }
    pub fn build_id(&self) -> &str {
        &self.build_id
    }
    pub fn artifact(&self, name: &str) -> io::Result<&Artifact> {
        let index = [
            "recovery.img",
            "kernel",
            "initramfs",
            "usb_uki",
            "efi_loader",
        ]
        .iter()
        .position(|expected| *expected == name)
        .ok_or_else(|| invalid("Unknown Recovery artifact"))?;
        Ok(&self.artifacts[index])
    }
    pub fn verify_file(&self, name: &str, file: &File) -> io::Result<()> {
        let expected = self.artifact(name)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.mode() & 0o022 != 0
            || metadata.len() != expected.bytes
            || sha256_file(file)? != expected.sha256
        {
            return Err(invalid(
                "Recovery artifact differs from its independently authenticated manifest",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::recovery::records::signing_fixture::Signer;
    pub fn bytes() -> Vec<u8> {
        let mut text = format!(
            "kind=linos-recovery\nformat=1\nsource_sha={}\nbuild_id=fixture\narchitecture=amd64\nhardware_profile=qemu-amd64-uefi\nkernel_release=test-linos\n",
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
                "{name}_bytes=1\n{name}_sha256={}\n",
                "b".repeat(64)
            ));
        }
        text.into_bytes()
    }
    #[test]
    fn recovery_requires_its_own_signing_domain_and_exact_schema() {
        let signer = Signer::new();
        let bytes = bytes();
        let recovery =
            OfficialRecovery::authenticate(&bytes, &signer.sign(DOMAIN, &bytes), &signer.public)
                .unwrap();
        assert_eq!(recovery.architecture(), "amd64");
        for domain in [
            b"LINOS-DEPLOYMENT-V3\n".as_slice(),
            b"LINOS-OFFLINE-V3\n",
            b"",
        ] {
            assert!(
                OfficialRecovery::authenticate(
                    &bytes,
                    &signer.sign(domain, &bytes),
                    &signer.public
                )
                .is_err()
            );
        }
        let wrong = Signer::new();
        assert!(
            OfficialRecovery::authenticate(&bytes, &signer.sign(DOMAIN, &bytes), &wrong.public)
                .is_err()
        );
        let text = String::from_utf8(bytes).unwrap();
        for (from, to) in [
            ("kernel_bytes=1", "kernel_bytes=01"),
            ("recovery_img_bytes=1", "recovery_img_bytes=3221225473"),
            ("format=1", "format=2"),
        ] {
            let bad = text.replace(from, to);
            assert!(
                OfficialRecovery::authenticate(
                    bad.as_bytes(),
                    &signer.sign(DOMAIN, bad.as_bytes()),
                    &signer.public
                )
                .is_err()
            );
        }
    }
}
