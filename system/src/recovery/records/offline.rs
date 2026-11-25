//! Format 3 transport metadata and sequential image reconstruction.
use super::{
    deployment::{self, Artifact, OfficialManifest, sha256_file, verify_signature},
    hash, invalid,
    metadata::Fields,
    recovery::OfficialRecovery,
};
use rustix::fs::{self, Mode, OFlags};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::fs::MetadataExt},
};

pub const DOMAIN: &[u8] = b"LINOS-OFFLINE-V3\n";
pub const NAMES: [&str; 10] = [
    "deployment-manifest",
    "deployment-SHA256SUMS",
    "deployment-SHA256SUMS.sig",
    "kernel",
    "initramfs",
    "inventory",
    "applications",
    "services",
    "accounts",
    "extensions",
];
const FIELDS: [&str; 10] = [
    "deployment_manifest",
    "deployment_checksums",
    "deployment_signature",
    "kernel",
    "initramfs",
    "inventory",
    "applications",
    "services",
    "accounts",
    "extensions",
];
const GIB: u64 = 1024 * 1024 * 1024;

pub struct Offline {
    architecture: String,
    hardware_profile: String,
    deployment_id: String,
    recovery_id: String,
    system: Artifact,
    parts: Vec<Artifact>,
    artifacts: [Artifact; 10],
}

impl Offline {
    pub fn authenticate(
        bytes: &[u8],
        signature: &[u8],
        trusted_key: &[u8],
        recovery: &OfficialRecovery,
    ) -> io::Result<Self> {
        if bytes.len() > 64 * 1024 || signature.len() != 64 || trusted_key.len() > 4096 {
            return Err(invalid("Offline authentication input exceeds bounds"));
        }
        let mut message = DOMAIN.to_vec();
        message.extend_from_slice(bytes);
        verify_signature(&message, signature, trusted_key)?;
        let mut f = Fields::new(bytes)?;
        if f.next("kind")? != "linos-offline" || f.next("format")? != "3" {
            return Err(invalid("Unsupported offline transport"));
        }
        let architecture = f.next("architecture")?.to_owned();
        let hardware_profile = f.next("hardware_profile")?.to_owned();
        let deployment_id = hash(f.next("deployment_id")?)?;
        let recovery_id = hash(f.next("recovery_manifest_sha256")?)?;
        let system = Artifact {
            bytes: f.number("system_bytes", 4 * GIB)?,
            sha256: hash(f.next("system_sha256")?)?,
        };
        let count = f.number("count", 4)?;
        if system.bytes == 0
            || count != system.bytes.div_ceil(GIB)
            || architecture != recovery.architecture()
            || hardware_profile != recovery.hardware_profile()
            || recovery_id != recovery.id()
        {
            return Err(invalid(
                "Offline identity, image length or Recovery binding is inconsistent",
            ));
        }
        let mut parts = Vec::new();
        for index in 0..count {
            let bytes = f.number(&format!("part.{index:04}.bytes"), GIB)?;
            let digest = hash(f.next(&format!("part.{index:04}.sha256"))?)?;
            if bytes != (system.bytes - index * GIB).min(GIB) {
                return Err(invalid("Offline chunk lengths are not canonical"));
            }
            parts.push(Artifact {
                bytes,
                sha256: digest,
            });
        }
        let mut artifacts = Vec::new();
        for (index, name) in FIELDS.iter().enumerate() {
            let limit = match index {
                0 => 64 * 1024,
                1 => 2048,
                2 => 64,
                3 | 4 => 128 * 1024 * 1024,
                _ => 16 * 1024 * 1024,
            };
            let bytes = f.number(&format!("{name}_bytes"), limit)?;
            let digest = hash(f.next(&format!("{name}_sha256"))?)?;
            if bytes == 0 || index == 2 && bytes != 64 {
                return Err(invalid("Offline artifact length is invalid"));
            }
            artifacts.push(Artifact {
                bytes,
                sha256: digest,
            });
        }
        f.finish()?;
        Ok(Self {
            architecture,
            hardware_profile,
            deployment_id,
            recovery_id,
            system,
            parts,
            artifacts: artifacts
                .try_into()
                .map_err(|_| invalid("Missing transport artifact"))?,
        })
    }
    pub fn deployment_id(&self) -> &str {
        &self.deployment_id
    }
    pub fn recovery_id(&self) -> &str {
        &self.recovery_id
    }
    pub fn system(&self) -> &Artifact {
        &self.system
    }
}

pub struct Payload {
    transport: Offline,
    deployment: OfficialManifest,
    files: [File; 10],
    parts: Vec<File>,
}

fn verified(directory: &File, name: &str, expected: &Artifact) -> io::Result<File> {
    let file = File::from(fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.len() != expected.bytes
        || sha256_file(&file)? != expected.sha256
    {
        return Err(invalid(
            "Offline artifact length/hash or file identity is invalid",
        ));
    }
    Ok(file)
}

fn contents(file: &File, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        file.as_raw_fd()
    ))?
    .take(limit + 1)
    .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("Offline metadata exceeds its bound"));
    }
    Ok(bytes)
}

impl Payload {
    pub fn open(
        directory: &File,
        trusted_key: &[u8],
        recovery: &OfficialRecovery,
    ) -> io::Result<Self> {
        let transport = Offline::authenticate(
            &deployment::source::read_at(directory, "manifest", 64 * 1024)?,
            &deployment::source::read_at(directory, "manifest.sig", 64)?,
            trusted_key,
            recovery,
        )?;
        let mut files = Vec::new();
        for (name, expected) in NAMES.iter().zip(&transport.artifacts) {
            files.push(verified(directory, name, expected)?);
        }
        let deployment = OfficialManifest::authenticate(
            &contents(&files[0], 64 * 1024)?,
            &contents(&files[1], 2048)?,
            &contents(&files[2], 64)?,
            trusted_key,
        )?;
        if deployment.id() != transport.deployment_id
            || deployment.deployment().architecture != transport.architecture
            || deployment.deployment().hardware_profile != transport.hardware_profile
            || deployment.deployment().artifacts[0] != transport.system
            || deployment.deployment().artifacts[1..] != transport.artifacts[3..]
        {
            return Err(invalid(
                "Transport and original deployment authorization disagree",
            ));
        }
        let mut parts = Vec::new();
        for (index, expected) in transport.parts.iter().enumerate() {
            parts.push(verified(
                directory,
                &format!("system.part.{index:04}"),
                expected,
            )?);
        }
        Ok(Self {
            transport,
            deployment,
            files: files
                .try_into()
                .map_err(|_| invalid("Missing offline files"))?,
            parts,
        })
    }
    pub fn deployment(&self) -> &OfficialManifest {
        &self.deployment
    }
    pub fn transport(&self) -> &Offline {
        &self.transport
    }
    pub fn file(&self, name: &str) -> Option<&File> {
        NAMES
            .iter()
            .position(|expected| *expected == name)
            .map(|index| &self.files[index])
    }
    /// The caller creates a new private standalone regular file in its staging
    /// workspace. Never reconstruct in RAM or publish a partial image.
    pub fn reconstruct(&self, output: &mut File) -> io::Result<()> {
        let metadata = output.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.len() != 0
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
        {
            return Err(invalid(
                "Reconstruction requires a fresh private standalone regular file",
            ));
        }
        for (part, expected) in self.parts.iter().zip(&self.transport.parts) {
            let mut input = File::open(format!(
                "/proc/{}/fd/{}",
                std::process::id(),
                part.as_raw_fd()
            ))?
            .take(expected.bytes + 1);
            if io::copy(&mut input, output)? != expected.bytes {
                return Err(invalid("Offline chunk changed during reconstruction"));
            }
        }
        output.flush()?;
        output.sync_all()?;
        if output.metadata()?.len() != self.transport.system.bytes
            || sha256_file(output)? != self.transport.system.sha256
        {
            return Err(invalid(
                "Reconstructed image differs from signed deployment",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::records::{recovery, signing_fixture::Signer};
    #[test]
    fn transport_rejects_wrong_role_recovery_binding_and_noncanonical_chunking() {
        let signer = Signer::new();
        let recovery_bytes = recovery::tests::bytes();
        let recovery = OfficialRecovery::authenticate(
            &recovery_bytes,
            &signer.sign(recovery::DOMAIN, &recovery_bytes),
            &signer.public,
        )
        .unwrap();
        let mut text = format!(
            "kind=linos-offline\nformat=3\narchitecture=amd64\nhardware_profile=qemu-amd64-uefi\ndeployment_id={}\nrecovery_manifest_sha256={}\nsystem_bytes=4096\nsystem_sha256={}\ncount=1\npart.0000.bytes=4096\npart.0000.sha256={}\n",
            "a".repeat(64),
            recovery.id(),
            "b".repeat(64),
            "c".repeat(64)
        );
        for (index, name) in FIELDS.iter().enumerate() {
            text.push_str(&format!(
                "{name}_bytes={}\n{name}_sha256={}\n",
                if index == 2 { 64 } else { 1 },
                "d".repeat(64)
            ));
        }
        assert!(
            Offline::authenticate(
                text.as_bytes(),
                &signer.sign(DOMAIN, text.as_bytes()),
                &signer.public,
                &recovery
            )
            .is_ok()
        );
        assert!(
            Offline::authenticate(
                text.as_bytes(),
                &signer.sign(recovery::DOMAIN, text.as_bytes()),
                &signer.public,
                &recovery
            )
            .is_err()
        );
        for (from, to) in [
            ("count=1", "count=2"),
            ("part.0000.bytes=4096", "part.0000.bytes=4095"),
            ("system_bytes=4096", "system_bytes=4294967297"),
            ("architecture=amd64", "architecture=arm64"),
            ("part.0000.bytes", "part.0001.bytes"),
        ] {
            let bad = text.replace(from, to);
            assert!(
                Offline::authenticate(
                    bad.as_bytes(),
                    &signer.sign(DOMAIN, bad.as_bytes()),
                    &signer.public,
                    &recovery
                )
                .is_err()
            );
        }
        let bad = text.replace(recovery.id(), &"e".repeat(64));
        assert!(
            Offline::authenticate(
                bad.as_bytes(),
                &signer.sign(DOMAIN, bad.as_bytes()),
                &signer.public,
                &recovery
            )
            .is_err()
        );
    }
}
