//! Format 3 parsing and official manifest authentication, without activation.
//! Artifact bytes, filesystem integrity and device ancestry are checked separately.
pub mod artifacts;
pub mod source;

use super::{MAX_RECORD_BYTES, hash, invalid, ordered};
use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::fd::AsRawFd,
    process::{Command, Stdio},
};

pub const DOMAIN: &[u8] = b"LINOS-DEPLOYMENT-V3\n";
pub const ARTIFACT_NAMES: [&str; 8] = [
    "system.img",
    "kernel",
    "initramfs",
    "inventory",
    "applications",
    "services",
    "accounts",
    "extensions",
];
const KEYS: [&str; 29] = [
    "kind",
    "format",
    "origin",
    "base_manifest_sha256",
    "plan_sha256",
    "source_sha",
    "build_id",
    "release",
    "architecture",
    "hardware_profile",
    "kernel_release",
    "config_schema_min",
    "config_schema_max",
    "system_bytes",
    "system_sha256",
    "kernel_bytes",
    "kernel_sha256",
    "initramfs_bytes",
    "initramfs_sha256",
    "inventory_bytes",
    "inventory_sha256",
    "applications_bytes",
    "applications_sha256",
    "services_bytes",
    "services_sha256",
    "accounts_bytes",
    "accounts_sha256",
    "extensions_bytes",
    "extensions_sha256",
];
const MAX_SUMS_BYTES: usize = 2048;
const MAX_KEY_BYTES: usize = 4096;
const MIB: u64 = 1024 * 1024;

fn decimal(value: &str) -> io::Result<u64> {
    if value.is_empty()
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value.len() > 1 && value.starts_with('0')
    {
        return Err(invalid("Expected canonical unsigned decimal"));
    }
    value.parse().map_err(invalid)
}

fn token(value: &str) -> io::Result<String> {
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(invalid("Invalid deployment identity token"));
    }
    Ok(value.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Ci,
    Local {
        base_manifest_sha256: String,
        plan_sha256: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub bytes: u64,
    pub sha256: String,
}

/// Syntactically valid metadata. This type alone carries no trust decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    pub origin: Origin,
    pub source_sha: String,
    pub build_id: String,
    pub release: u64,
    pub architecture: String,
    pub hardware_profile: String,
    pub kernel_release: String,
    pub config_schema_min: u32,
    pub config_schema_max: u32,
    /// Fixed order matches ARTIFACT_NAMES, never manifest supplied paths.
    pub artifacts: [Artifact; 8],
}

impl Deployment {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(bytes, KEYS)?;
        if f[0] != "linos-deployment" || f[1] != "3" {
            return Err(invalid("Unsupported deployment kind or format"));
        }
        let origin = match (f[2], f[3], f[4]) {
            ("ci", "none", "none") => Origin::Ci,
            ("local", base, plan) => Origin::Local {
                base_manifest_sha256: hash(base)?,
                plan_sha256: hash(plan)?,
            },
            _ => return Err(invalid("Inconsistent deployment origin and provenance")),
        };
        if f[5].len() != 40
            || !f[5]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("Source commit must be lowercase SHA1"));
        }
        let release = decimal(f[7])?;
        if release == 0 || !matches!(f[8], "amd64" | "arm64") {
            return Err(invalid("Unsupported release or architecture"));
        }
        let config_schema_min = u32::try_from(decimal(f[11])?).map_err(invalid)?;
        let config_schema_max = u32::try_from(decimal(f[12])?).map_err(invalid)?;
        if config_schema_min > config_schema_max {
            return Err(invalid("Configuration schema range is reversed"));
        }
        let mut artifacts = Vec::with_capacity(8);
        for (index, limit) in [
            4096 * MIB,
            128 * MIB,
            128 * MIB,
            16 * MIB,
            16 * MIB,
            16 * MIB,
            16 * MIB,
            16 * MIB,
        ]
        .into_iter()
        .enumerate()
        {
            let bytes = decimal(f[13 + index * 2])?;
            if bytes == 0 || bytes > limit {
                return Err(invalid("Deployment artifact size outside bounds"));
            }
            artifacts.push(Artifact {
                bytes,
                sha256: hash(f[14 + index * 2])?,
            });
        }
        Ok(Self {
            origin,
            source_sha: f[5].to_owned(),
            build_id: token(f[6])?,
            release,
            architecture: f[8].to_owned(),
            hardware_profile: token(f[9])?,
            kernel_release: token(f[10])?,
            config_schema_min,
            config_schema_max,
            artifacts: artifacts
                .try_into()
                .map_err(|_| invalid("Missing artifact"))?,
        })
    }

    pub fn require_target(
        &self,
        architecture: &str,
        hardware_profile: &str,
        kernel_release: &str,
    ) -> io::Result<()> {
        if self.architecture != architecture
            || self.hardware_profile != hardware_profile
            || self.kernel_release != kernel_release
        {
            return Err(invalid("Deployment does not match the expected target"));
        }
        Ok(())
    }
}

/// Authenticated official metadata only. Does not assert artifact or boot readiness.
#[derive(Debug)]
pub struct OfficialManifest {
    deployment: Deployment,
    id: String,
}

impl OfficialManifest {
    /// The caller supplies a separately trusted CI/development public key. No key
    /// from a payload is discovered or promoted to trust by this interface.
    /// Local compositions require device key and provenance chain validation and
    /// remain unavailable through this official-only verifier.
    pub fn authenticate(
        manifest: &[u8],
        sums: &[u8],
        signature: &[u8],
        trusted_key: &[u8],
    ) -> io::Result<Self> {
        if manifest.len() > MAX_RECORD_BYTES
            || sums.len() > MAX_SUMS_BYTES
            || trusted_key.is_empty()
            || trusted_key.len() > MAX_KEY_BYTES
            || signature.len() != 64
        {
            return Err(invalid("Deployment authentication input outside bounds"));
        }
        let mut message = Vec::with_capacity(DOMAIN.len() + sums.len());
        message.extend_from_slice(DOMAIN);
        message.extend_from_slice(sums);
        verify_signature(&message, signature, trusted_key)?;

        // No manifest identity or checksum is trusted before the domain signature.
        let hashes = checksums(sums)?;
        let id = sha256(manifest)?;
        if id != hashes[0] {
            return Err(invalid("Signed deployment manifest digest mismatch"));
        }
        let deployment = Deployment::parse(manifest)?;
        if deployment.origin != Origin::Ci {
            return Err(invalid("Official key cannot authorize a local composition"));
        }
        for (artifact, digest) in deployment.artifacts.iter().zip(&hashes[1..]) {
            if artifact.sha256 != *digest {
                return Err(invalid("Manifest and signed checksum list disagree"));
            }
        }
        Ok(Self { deployment, id })
    }

    pub fn deployment(&self) -> &Deployment {
        &self.deployment
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}

fn checksums(bytes: &[u8]) -> io::Result<[String; 9]> {
    let text = std::str::from_utf8(bytes).map_err(invalid)?;
    if !text.ends_with('\n') {
        return Err(invalid("Checksum list needs a final newline"));
    }
    let mut lines = text[..text.len() - 1].split('\n');
    let mut digests = Vec::with_capacity(9);
    for name in std::iter::once("manifest").chain(ARTIFACT_NAMES) {
        let (digest, found) = lines
            .next()
            .and_then(|line| line.split_once("  "))
            .ok_or_else(|| invalid("Missing canonical checksum entry"))?;
        if found != name {
            return Err(invalid("Unexpected checksum name or order"));
        }
        digests.push(hash(digest)?);
    }
    if lines.next().is_some() {
        return Err(invalid("Surplus checksum entries"));
    }
    digests
        .try_into()
        .map_err(|_| invalid("Missing checksum entries"))
}

// OpenSSL Ed25519 raw verification needs a seekable input. Small sealed memory
// files snapshot bounded metadata without a caller controlled scratch directory.
// No system images or large artifacts are placed in memory here.
fn snapshot(bytes: &[u8]) -> io::Result<File> {
    let mut file = File::from(memfd_create(
        "linos-deployment-metadata",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )?);
    file.write_all(bytes)?;
    fcntl_add_seals(
        &file,
        SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
    )?;
    Ok(file)
}

fn descriptor_path(file: &File) -> String {
    format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd())
}

pub(crate) fn sha256(bytes: &[u8]) -> io::Result<String> {
    let input = snapshot(bytes)?;
    sha256_file(&input)
}

pub(crate) fn sha256_file(input: &File) -> io::Result<String> {
    let length = input.metadata()?.len();
    if length > 4096 * MIB {
        return Err(invalid("Deployment hash input exceeds the image limit"));
    }
    // Reopen the retained identity with an independent offset. Never follow the
    // original payload pathname, and never stream indefinitely if a file grows.
    let mut source = File::open(descriptor_path(input))?.take(length + 1);
    let mut child = Command::new("/usr/bin/openssl")
        .args(["dgst", "-sha256", "-binary"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let copied = io::copy(&mut source, child.stdin.as_mut().expect("piped hash input"));
    drop(child.stdin.take());
    if let Err(error) = copied {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let output = child.wait_with_output()?;
    if copied? != length {
        return Err(invalid("Deployment hash input length changed during read"));
    }
    if !output.status.success() || output.stdout.len() != 32 {
        return Err(invalid("Deployment SHA256 failed"));
    }
    Ok(output.stdout.iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn verify_signature(message: &[u8], signature: &[u8], key: &[u8]) -> io::Result<()> {
    let input = snapshot(message)?;
    let signature = snapshot(signature)?;
    let key = snapshot(key)?;
    let info = Command::new("/usr/bin/openssl")
        .args(["pkey", "-pubin", "-outform", "DER", "-in"])
        .arg(descriptor_path(&key))
        .output()?;
    const ED25519_PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    if !info.status.success() || info.stdout.len() != 44 || !info.stdout.starts_with(ED25519_PREFIX)
    {
        return Err(invalid("Deployment trust key must be Ed25519"));
    }
    let status = Command::new("/usr/bin/openssl")
        .args(["pkeyutl", "-verify", "-rawin", "-pubin", "-inkey"])
        .arg(descriptor_path(&key))
        .arg("-in")
        .arg(descriptor_path(&input))
        .arg("-sigfile")
        .arg(descriptor_path(&signature))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(invalid("Deployment domain signature verification failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
