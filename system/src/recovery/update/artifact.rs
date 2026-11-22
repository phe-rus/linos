//! Authenticate image metadata, then verify bounded copies in private staging.
use super::journal::open_directory;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) const NAMES: [&str; 4] = ["system.img", "kernel", "initramfs", "manifest"];
const MAX_IMAGE: u64 = 4 * 1024 * 1024 * 1024;
const MAX_BOOT: u64 = 128 * 1024 * 1024;

pub(super) fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn regular(path: &Path) -> io::Result<File> {
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() || before.nlink() != 1 {
        return Err(invalid("Artifact is not a standalone regular file"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.dev() != before.dev()
        || metadata.ino() != before.ino()
    {
        return Err(invalid(
            "Artifact must remain a regular file without hard links",
        ));
    }
    Ok(file)
}

pub(super) fn bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let file = regular(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("Artifact exceeds size limit"));
    }
    Ok(bytes)
}

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    open_directory(path)?.sync_all()
}

pub(super) fn mkdir(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => fs::set_permissions(path, fs::Permissions::from_mode(0o700))?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    open_directory(path)?;
    Ok(())
}

pub(super) fn digest(path: &Path) -> io::Result<String> {
    let output = Command::new("/usr/bin/openssl")
        .args(["dgst", "-sha256", "-r"])
        .stdin(regular(path)?)
        .output()?;
    let text = String::from_utf8(output.stdout).map_err(invalid)?;
    let hash = text
        .split_whitespace()
        .next()
        .ok_or_else(|| invalid("Missing digest"))?;
    if !output.status.success() || !valid_hash(hash) {
        return Err(invalid("SHA-256 failed"));
    }
    Ok(hash.to_owned())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub(super) fn fields(bytes: &[u8]) -> io::Result<BTreeMap<&str, &str>> {
    let text = std::str::from_utf8(bytes).map_err(invalid)?;
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| invalid("Malformed metadata field"))?;
        if value.is_empty() || fields.insert(key, value).is_some() {
            return Err(invalid("Empty or duplicate metadata field"));
        }
    }
    Ok(fields)
}

pub(super) fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[derive(Debug)]
pub(super) struct Payload {
    pub release: u64,
    sizes: [u64; 3],
    hashes: [String; 4],
    source: PathBuf,
    // Keep the directory descriptor alive and address all files through it.
    _directory: File,
    sums: Vec<u8>,
    signature: Vec<u8>,
    manifest: Vec<u8>,
}

impl Payload {
    pub fn authenticate(source: &Path, key: &Path, scratch: &Path) -> io::Result<Self> {
        if fs::canonicalize(key)?.starts_with(fs::canonicalize(source)?) {
            return Err(invalid(
                "Trust key must be supplied independently of the payload",
            ));
        }
        let directory = open_directory(source)?;
        let source = PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            directory.as_raw_fd()
        ));
        let sums = bounded(&source.join("SHA256SUMS"), 512)?;
        let signature = bounded(&source.join("SHA256SUMS.sig"), 64)?;
        let manifest = bounded(&source.join("manifest"), 1024)?;
        if signature.len() != 64 {
            return Err(invalid("Ed25519 signature must be 64 bytes"));
        }
        // Snapshot the separately configured key, never use a key from the payload.
        write_new(&scratch.join("trusted.pem"), &bounded(key, 1024)?)?;
        write_new(&scratch.join("SHA256SUMS"), &sums)?;
        write_new(&scratch.join("SHA256SUMS.sig"), &signature)?;
        write_new(&scratch.join("manifest"), &manifest)?;
        let key_info = Command::new("/usr/bin/openssl")
            .args(["pkey", "-pubin", "-outform", "DER", "-in"])
            .arg(scratch.join("trusted.pem"))
            .output()?;
        const ED25519_PREFIX: &[u8] = &[
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        if !key_info.status.success()
            || key_info.stdout.len() != 44
            || !key_info.stdout.starts_with(ED25519_PREFIX)
        {
            return Err(invalid("Trust key must be Ed25519"));
        }
        let status = Command::new("/usr/bin/openssl")
            .args(["pkeyutl", "-verify", "-rawin", "-pubin", "-inkey"])
            .arg(scratch.join("trusted.pem"))
            .arg("-in")
            .arg(scratch.join("SHA256SUMS"))
            .arg("-sigfile")
            .arg(scratch.join("SHA256SUMS.sig"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            return Err(invalid("Update signature verification failed"));
        }
        let lines: Vec<_> = std::str::from_utf8(&sums)
            .map_err(invalid)?
            .lines()
            .collect();
        if lines.len() != NAMES.len() {
            return Err(invalid("Unexpected checksum file count"));
        }
        let mut hashes = [const { String::new() }; 4];
        for (index, (line, name)) in lines.iter().zip(NAMES).enumerate() {
            let (hash, found) = line
                .split_once("  ")
                .ok_or_else(|| invalid("Malformed checksum"))?;
            if found != name || !valid_hash(hash) {
                return Err(invalid("Unexpected checksum name or digest"));
            }
            hashes[index] = hash.to_owned();
        }
        if digest(&scratch.join("manifest"))? != hashes[3] {
            return Err(invalid("Signed manifest digest mismatch"));
        }
        let values = fields(&manifest)?;
        if values.len() != 9
            || values.get("format") != Some(&"2")
            || values.get("edition") != Some(&"server")
            || values.get("architecture") != Some(&"amd64")
            || values.get("hardware_profile") != Some(&"qemu-amd64-uefi")
        {
            return Err(invalid("Unsupported update target or metadata fields"));
        }
        let number = |name| -> io::Result<u64> {
            values
                .get(name)
                .ok_or_else(|| invalid("Missing numeric manifest field"))?
                .parse()
                .map_err(invalid)
        };
        let release = number("release")?;
        let sizes = [
            number("system_bytes")?,
            number("kernel_bytes")?,
            number("initramfs_bytes")?,
        ];
        if release == 0
            || sizes
                .iter()
                .zip([MAX_IMAGE, MAX_BOOT, MAX_BOOT])
                .any(|(&size, limit)| size == 0 || size > limit)
        {
            return Err(invalid("Release or artifact size outside supported bounds"));
        }
        let build = values
            .get("build")
            .ok_or_else(|| invalid("Missing build identifier"))?;
        if build.len() > 64
            || !build
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(invalid("Invalid build identifier"));
        }
        let result = Self {
            release,
            sizes,
            hashes,
            source,
            _directory: directory,
            sums,
            signature,
            manifest,
        };
        Ok(result)
    }

    pub fn copy_verified(&self, destination: &Path) -> io::Result<()> {
        for (index, name) in NAMES[..3].iter().enumerate() {
            let mut input = regular(&self.source.join(name))?.take(self.sizes[index] + 1);
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(destination.join(name))?;
            let copied = io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            if copied != self.sizes[index] || digest(&destination.join(name))? != self.hashes[index]
            {
                return Err(invalid("Copied artifact size or signed digest mismatch"));
            }
        }
        write_new(&destination.join("manifest"), &self.manifest)?;
        write_new(&destination.join("SHA256SUMS"), &self.sums)?;
        write_new(&destination.join("SHA256SUMS.sig"), &self.signature)?;
        sync_directory(destination)
    }

    pub fn verify_content(&self) -> io::Result<()> {
        for (index, name) in NAMES[..3].iter().enumerate() {
            if regular(&self.source.join(name))?.metadata()?.len() != self.sizes[index]
                || digest(&self.source.join(name))? != self.hashes[index]
            {
                return Err(invalid(
                    "Installed candidate no longer matches signed artifacts",
                ));
            }
        }
        Ok(())
    }
}
