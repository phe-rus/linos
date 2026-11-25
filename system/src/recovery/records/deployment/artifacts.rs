//! Read only validation of pinned regular files. Callers must retain their volume
//! lock and use these descriptors, then validate actual loop and mount ancestry.
use super::{ARTIFACT_NAMES, Artifact, OfficialManifest, descriptor_path, invalid, sha256_file};
use rustix::fs::{Mode, OFlags, openat};
use std::{
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    process::{Command, Stdio},
};

/// Checked bytes and a clean ext4 image at validation time, not boot approval or
/// protection against a subsequent write by the owner. Revalidate before use.
#[derive(Debug)]
pub struct VerifiedArtifacts {
    files: [File; 8],
    expected: [Artifact; 8],
    deployment_id: String,
}

impl OfficialManifest {
    /// The directory descriptor must come from the caller's validated volume
    /// path under its journal lock. This checks the directory itself, not its
    /// ancestry, selected transaction, metadata schemas or the installed layout.
    /// Runtime root callers require root ownership; local fixtures use their UID.
    pub fn verify_artifacts(&self, directory: &File) -> io::Result<VerifiedArtifacts> {
        let metadata = directory.metadata()?;
        if !metadata.is_dir()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
        {
            return Err(invalid(
                "Deployment directory must be caller owned and private",
            ));
        }
        let mut files = Vec::with_capacity(8);
        for name in ARTIFACT_NAMES {
            let file = File::from(openat(
                directory,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            regular(&file)?;
            files.push(file);
        }
        let result = VerifiedArtifacts {
            files: files
                .try_into()
                .map_err(|_| invalid("Missing artifact descriptor"))?,
            expected: self.deployment().artifacts.clone(),
            deployment_id: self.id().to_owned(),
        };
        result.revalidate()?;
        Ok(result)
    }
}

impl VerifiedArtifacts {
    /// Read only fixed signed metadata names through the retained descriptor.
    /// Rehash the returned bytes, so a stale earlier verification cannot silently
    /// authorize modified account/service data.
    pub fn metadata(&self, name: &str) -> io::Result<Vec<u8>> {
        let index = ARTIFACT_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .filter(|index| *index >= 3)
            .ok_or_else(|| invalid("Not a fixed deployment metadata artifact"))?;
        let expected = &self.expected[index];
        let file = &self.files[index];
        regular(file)?;
        let mut bytes = Vec::new();
        File::open(descriptor_path(file))?
            .take(expected.bytes + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != expected.bytes || super::sha256(&bytes)? != expected.sha256 {
            return Err(invalid("Metadata bytes changed after authentication"));
        }
        Ok(bytes)
    }
    pub fn deployment_id(&self) -> &str {
        &self.deployment_id
    }

    /// Always use this retained descriptor for loop attachment, never reopen a
    /// mutable pathname after validation. Loop attachment is not implemented here.
    pub fn system_image(&self) -> &File {
        &self.files[0]
    }

    pub fn file(&self, name: &str) -> Option<&File> {
        ARTIFACT_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .map(|index| &self.files[index])
    }

    pub fn revalidate(&self) -> io::Result<()> {
        for (file, expected) in self.files.iter().zip(&self.expected) {
            verify_bytes(file, expected)?;
        }
        clean_ext4(self.system_image())?;
        // The native checker must not have changed any authenticated image bytes.
        verify_bytes(self.system_image(), &self.expected[0])
    }
}

fn regular(file: &File) -> io::Result<std::fs::Metadata> {
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(invalid(
            "Artifact must be a caller owned regular file without links or other writers",
        ));
    }
    Ok(metadata)
}

fn verify_bytes(file: &File, expected: &Artifact) -> io::Result<()> {
    let before = regular(file)?;
    if before.len() != expected.bytes {
        return Err(invalid(
            "Artifact length does not match authenticated manifest",
        ));
    }
    if sha256_file(file)? != expected.sha256 {
        return Err(invalid(
            "Artifact digest does not match authenticated manifest",
        ));
    }
    let after = regular(file)?;
    if before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(invalid("Artifact changed while being checked"));
    }
    Ok(())
}

fn header_value<'a>(header: &'a str, key: &str) -> io::Result<Option<&'a str>> {
    let mut result = None;
    for line in header.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name == key
            && result.replace(value.trim()).is_some()
        {
            return Err(invalid("Duplicate filesystem header field"));
        }
    }
    Ok(result)
}

fn clean_header(header: &str) -> io::Result<()> {
    let features = header_value(header, "Filesystem features")?
        .ok_or_else(|| invalid("Missing filesystem features"))?;
    if header_value(header, "Filesystem state")? != Some("clean")
        || features
            .split_whitespace()
            .any(|feature| matches!(feature, "needs_recovery" | "journal_dev"))
    {
        return Err(invalid(
            "System image is unclean or requires journal recovery",
        ));
    }
    // Refuse an external journal before e2fsck can try to locate another device.
    if header_value(header, "Journal UUID")?.is_some()
        || header_value(header, "Journal device")?.is_some()
        || features
            .split_whitespace()
            .any(|feature| feature == "has_journal")
            && !header_value(header, "Journal inode")?
                .is_some_and(|inode| inode.parse::<u32>().is_ok_and(|number| number > 0))
    {
        return Err(invalid(
            "External or unidentified filesystem journal is unsupported",
        ));
    }
    Ok(())
}

fn clean_ext4(file: &File) -> io::Result<()> {
    let path = descriptor_path(file);
    let kind = Command::new("/usr/sbin/blkid")
        .args(["-p", "-o", "value", "-s", "TYPE", &path])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()?;
    if !kind.status.success() || kind.stdout != b"ext4\n" {
        return Err(invalid("System image is not an ext4 filesystem"));
    }
    let header = Command::new("/usr/sbin/dumpe2fs")
        .args(["-h", &path])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()?;
    if !header.status.success() {
        return Err(invalid("Cannot inspect system filesystem"));
    }
    clean_header(std::str::from_utf8(&header.stdout).map_err(invalid)?)?;
    let status = Command::new("/usr/sbin/e2fsck")
        .args(["-f", "-n", &path])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(invalid(
            "System filesystem check failed; repair and journal replay are forbidden",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
