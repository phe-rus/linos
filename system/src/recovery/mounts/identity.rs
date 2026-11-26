//! Read only native block inventory and loader ESP consistency. No trust keys,
//! Secure Boot attestation, boot entry selection or installation is performed.
use super::{Device, Layout, invalid};
use crate::recovery::records::uuid;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, FileTypeExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_DEVICES: usize = 256;
const MAX_PROBE: usize = 64 * 1024;
const LOADER_ESP: &str =
    "/sys/firmware/efi/efivars/LoaderDevicePartUUID-4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";
const ESP_TYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
const LINUX_TYPE: &str = "0fc63daf-8483-4772-8e79-3d69d8477de4";

type Tags = BTreeMap<String, String>;

fn tags(bytes: &[u8]) -> io::Result<Tags> {
    if bytes.len() > MAX_PROBE || bytes.contains(&0) || !bytes.is_empty() && !bytes.ends_with(b"\n")
    {
        return Err(invalid(
            "Native block probe output is oversized or truncated",
        ));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid("Invalid native block probe text"))?;
    let mut result = Tags::new();
    for line in text.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| invalid("Malformed native block probe field"))?;
        if key.is_empty()
            || value.is_empty()
            || value.len() > 4096
            || result.len() == 64
            || !key
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || result.insert(key.to_owned(), value.to_owned()).is_some()
        {
            return Err(invalid("Duplicate or invalid native block probe field"));
        }
    }
    Ok(result)
}

fn descriptor_path(file: &File) -> String {
    format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd())
}

fn probe(file: &File) -> io::Result<Tags> {
    let output = Command::new("/usr/sbin/blkid")
        .args(["-p", "-c", "/dev/null", "-o", "export"])
        .arg(descriptor_path(file))
        .env("LC_ALL", "C")
        .env("BLKID_CONF", "/dev/null")
        .stdin(Stdio::null())
        .output()?;
    match output.status.code() {
        Some(0) => tags(&output.stdout),
        // Unrecognized/empty devices may legitimately have no identities.
        Some(2) if output.stdout.is_empty() && output.stderr.is_empty() => Ok(Tags::new()),
        _ => Err(invalid(
            "Native block probe failed or found colliding signatures",
        )),
    }
}

fn text(path: &Path, limit: usize) -> io::Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("Native identity input exceeds its bound"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("Invalid native identity text"))
}

fn loader_uuid(bytes: &[u8]) -> io::Result<String> {
    // efivarfs has four attribute bytes before the NUL terminated UTF16 GUID.
    if bytes.len() != 4 + 37 * 2 || bytes[bytes.len() - 2..] != [0, 0] {
        return Err(invalid(
            "Loader ESP variable is missing, truncated or not a single GUID",
        ));
    }
    let units: Vec<_> = bytes[4..bytes.len() - 2]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let value = String::from_utf16(&units).map_err(|_| invalid("Invalid loader ESP UTF16"))?;
    uuid(&value.to_ascii_lowercase())
}

fn firmware() -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(LOADER_ESP)?.take(129).read_to_end(&mut bytes)?;
    loader_uuid(&bytes)?;
    Ok(bytes)
}

#[derive(Debug, Clone)]
struct Block {
    device: Device,
    parent: Option<Device>,
    partition: Option<u32>,
    tags: Tags,
}

impl Block {
    fn get(&self, name: &str) -> Option<&str> {
        self.tags.get(name).map(String::as_str)
    }
    fn guid(&self, name: &str, expected: &str) -> bool {
        self.get(name)
            .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    }
}

pub(super) struct Identity {
    pub volume: Device,
    pub esp: Device,
}

fn validate(layout: &Layout, blocks: &[Block], loader: &str) -> io::Result<(Identity, Device)> {
    layout.require_plain()?;
    if blocks.is_empty() || blocks.len() > MAX_DEVICES || loader != layout.esp_partuuid() {
        return Err(invalid(
            "Loader ESP does not match the installed layout or inventory is unavailable",
        ));
    }
    let mut devices = BTreeSet::new();
    if blocks
        .iter()
        .any(|block| !devices.insert((block.device.major, block.device.minor)))
    {
        return Err(invalid("Duplicate native block device identity"));
    }
    let mut selected = Vec::new();
    for (index, (id, name, kind, filesystem)) in [
        (layout.esp_partuuid(), "EFI", ESP_TYPE, "vfat"),
        (layout.recovery_partuuid(), "Recovery", LINUX_TYPE, "ext4"),
        (layout.linos_partuuid(), "Linos", LINUX_TYPE, "ext4"),
    ]
    .into_iter()
    .enumerate()
    {
        let matches: Vec<_> = blocks
            .iter()
            .filter(|block| block.guid("PART_ENTRY_UUID", id))
            .collect();
        if matches.len() != 1 {
            return Err(invalid("Partition UUID is missing or duplicated"));
        }
        let block = matches[0];
        let parent = block
            .parent
            .ok_or_else(|| invalid("Managed device has no observed whole disk parent"))?;
        if block.partition != Some(index as u32 + 1)
            || block.get("PART_ENTRY_NUMBER") != Some((index + 1).to_string().as_str())
            || block.get("PART_ENTRY_SCHEME") != Some("gpt")
            || block.get("PART_ENTRY_DISK")
                != Some(format!("{}:{}", parent.major, parent.minor).as_str())
            || block.get("PART_ENTRY_NAME") != Some(name)
            || !block.guid("PART_ENTRY_TYPE", kind)
            || block.get("TYPE") != Some(filesystem)
        {
            return Err(invalid(
                "Native partition role, filesystem or ancestry differs from layout 2",
            ));
        }
        selected.push(block);
    }
    let parent = selected[0].parent.unwrap();
    if selected.iter().any(|block| block.parent != Some(parent))
        || blocks
            .iter()
            .filter(|block| block.parent == Some(parent))
            .count()
            != 3
    {
        return Err(invalid(
            "Managed partitions do not form one three partition disk",
        ));
    }
    let disks: Vec<_> = blocks
        .iter()
        .filter(|block| block.guid("PTUUID", layout.disk_guid()))
        .collect();
    if disks.len() != 1
        || disks[0].device != parent
        || disks[0].parent.is_some()
        || disks[0].partition.is_some()
        || disks[0].get("PTTYPE") != Some("gpt")
    {
        return Err(invalid(
            "GPT disk GUID is missing, duplicated or on the wrong parent",
        ));
    }
    if !selected[2].guid("UUID", layout.filesystem_uuid())
        || blocks
            .iter()
            .filter(|block| block.guid("UUID", layout.filesystem_uuid()))
            .count()
            != 1
    {
        return Err(invalid(
            "Linos filesystem UUID is missing, duplicated or mismatched",
        ));
    }
    Ok((
        Identity {
            volume: selected[2].device,
            esp: selected[0].device,
        },
        parent,
    ))
}

fn names() -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir("/sys/class/block")? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| invalid("Invalid kernel block device name"))?;
        if names.len() == MAX_DEVICES
            || name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(invalid(
                "Kernel block inventory is unsupported or exceeds its bound",
            ));
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

fn device(path: &Path) -> io::Result<Device> {
    Device::parse(text(&path.join("dev"), 64)?.trim_end())
}

fn node(name: &str, expected: Device) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(format!("/dev/{name}"))?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_block_device() || Device::from_dev(metadata.rdev()) != expected {
        return Err(invalid("Block node differs from sysfs identity"));
    }
    // O_NONBLOCK can open an empty optical drive successfully. Force a native
    // media check so the caller can distinguish ENOMEDIUM/ENXIO from probe
    // failures without interpreting tool error text or ignoring I/O errors.
    file.read_at(&mut [0u8; 1], 0)?;
    Ok(file)
}

fn verify_gpt(file: &File) -> io::Result<()> {
    let output = Command::new("/usr/sbin/sgdisk")
        .arg("--verify")
        .arg(descriptor_path(file))
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()?;
    let report = std::str::from_utf8(&output.stdout)
        .map_err(|_| invalid("Invalid GPT verification text"))?;
    if !output.status.success()
        || !output.stderr.is_empty()
        || !report
            .lines()
            .any(|line| line.starts_with("No problems found."))
    {
        return Err(invalid("GPT primary/backup verification failed"));
    }
    Ok(())
}

fn unchanged(block: &Block, file: &File) -> io::Result<()> {
    if probe(file)? != block.tags {
        return Err(invalid(
            "Native block identifiers changed during inspection",
        ));
    }
    Ok(())
}

pub(super) fn inspect(layout: &Layout) -> io::Result<Identity> {
    let before_firmware = firmware()?;
    let before_names = names()?;
    let mut blocks = Vec::new();
    let mut opened = BTreeMap::new();
    for name in &before_names {
        let sys = fs::canonicalize(PathBuf::from("/sys/class/block").join(name))?;
        let observed = device(&sys)?;
        let partition = match text(&sys.join("partition"), 32) {
            Ok(value) => Some(super::number(value.trim_end())?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let parent = if partition.is_some() {
            Some(device(
                sys.parent()
                    .ok_or_else(|| invalid("Missing sysfs partition parent"))?,
            )?)
        } else {
            None
        };
        let file = match node(name, observed) {
            Ok(file) => file,
            // An empty removable drive or unattached loop has no identity to match.
            Err(error)
                if matches!(error.raw_os_error(), Some(libc::ENOMEDIUM | libc::ENXIO))
                    && partition.is_none() =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        let info = probe(&file)?;
        if device(&sys)? != observed {
            return Err(invalid("Sysfs device changed during inventory"));
        }
        blocks.push(Block {
            device: observed,
            parent,
            partition,
            tags: info,
        });
        opened.insert((observed.major, observed.minor), file);
    }
    let (identity, disk) = validate(layout, &blocks, &loader_uuid(&before_firmware)?)?;
    verify_gpt(
        opened
            .get(&(disk.major, disk.minor))
            .ok_or_else(|| invalid("Missing retained GPT disk"))?,
    )?;
    for block in &blocks {
        unchanged(
            block,
            opened
                .get(&(block.device.major, block.device.minor))
                .ok_or_else(|| invalid("Missing retained inventory device"))?,
        )?;
    }
    if names()? != before_names || firmware()? != before_firmware {
        return Err(invalid(
            "Block inventory or loader identity changed during inspection",
        ));
    }
    Ok(identity)
}

pub(super) fn open_partition(partuuid: &str) -> io::Result<File> {
    uuid(partuuid)?;
    let before = names()?;
    let mut selected = None;
    for name in &before {
        let sys = fs::canonicalize(PathBuf::from("/sys/class/block").join(name))?;
        if !sys.join("partition").is_file() {
            continue;
        }
        let number = device(&sys)?;
        let file = node(name, number)?;
        let tags = probe(&file)?;
        if tags
            .get("PART_ENTRY_UUID")
            .is_some_and(|id| id.eq_ignore_ascii_case(partuuid))
        {
            if selected.is_some()
                || tags.get("PART_ENTRY_SCHEME").map(String::as_str) != Some("gpt")
            {
                return Err(invalid(
                    "Partition UUID is duplicated or not a GPT partition",
                ));
            }
            selected = Some((file, tags));
        }
    }
    let (file, tags) =
        selected.ok_or_else(|| invalid("Partition UUID was not observed in native inventory"))?;
    if names()? != before || probe(&file)? != tags {
        return Err(invalid("Partition identity changed during bootstrap"));
    }
    Ok(file)
}

#[cfg(test)]
mod tests;
