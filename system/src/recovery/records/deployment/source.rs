//! Open one deployment through trusted directories and authenticate it with an
//! independently provisioned key. Payload files never select their own key.
use super::{OfficialManifest, artifacts::VerifiedArtifacts};
use crate::recovery::{
    records::{invalid, metadata::Requirements},
    update::journal::open_directory,
};
use rustix::fs::{self, Mode, OFlags};
use std::{
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
};

pub struct Loaded {
    manifest: OfficialManifest,
    artifacts: VerifiedArtifacts,
    requirements: Requirements,
}

pub fn read_at(directory: &File, name: &str, limit: usize) -> io::Result<Vec<u8>> {
    if name.contains('/') || matches!(name, "" | "." | "..") {
        return Err(invalid("Expected one fixed metadata filename"));
    }
    let file = File::from(fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
        || metadata.len() > limit as u64
    {
        return Err(invalid(
            "Metadata must be a bounded protected standalone regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("Metadata exceeds its bound"));
    }
    Ok(bytes)
}

pub fn trusted_key(path: &Path) -> io::Result<Vec<u8>> {
    let parent = open_directory(
        path.parent()
            .ok_or_else(|| invalid("Missing trust key parent"))?,
    )?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("Invalid trust key name"))?;
    read_at(&parent, name, 4096)
}

/// Can authenticate retained manifest evidence without opening a system image.
pub fn manifest(directory: &File, trusted_key: &[u8]) -> io::Result<OfficialManifest> {
    OfficialManifest::authenticate(
        &read_at(directory, "manifest", 64 * 1024)?,
        &read_at(directory, "SHA256SUMS", 2048)?,
        &read_at(directory, "SHA256SUMS.sig", 64)?,
        trusted_key,
    )
}

impl Loaded {
    pub fn open(path: &Path, trusted_key: &[u8]) -> io::Result<Self> {
        let directory = open_directory(path)?;
        let manifest = manifest(&directory, trusted_key)?;
        if path.file_name().and_then(|name| name.to_str()) != Some(manifest.id()) {
            return Err(invalid(
                "Deployment directory does not name its exact manifest identity",
            ));
        }
        let artifacts = manifest.verify_artifacts(&directory)?;
        let requirements = Requirements::load(&artifacts)?;
        Ok(Self {
            manifest,
            artifacts,
            requirements,
        })
    }
    pub fn manifest(&self) -> &OfficialManifest {
        &self.manifest
    }
    pub fn artifacts(&self) -> &VerifiedArtifacts {
        &self.artifacts
    }
    pub fn requirements(&self) -> &Requirements {
        &self.requirements
    }
}
