//! Layout 2 mount and loop inspection only. No attachment, mounts or health
//! approval. Journal selection, schemas and boot receipt validation remain separate.
mod identity;

use super::records::{Layout, deployment::artifacts::VerifiedArtifacts};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    },
};

const VOLUME: &str = "/run/linos/volume";
const MAX_MOUNTINFO: usize = 1024 * 1024;
const MAX_MOUNTS: usize = 4096;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    pub major: u32,
    pub minor: u32,
}

impl Device {
    fn parse(value: &str) -> io::Result<Self> {
        let (major, minor) = value
            .split_once(':')
            .ok_or_else(|| invalid("Missing device number"))?;
        Ok(Self {
            major: number(major)?,
            minor: number(minor)?,
        })
    }

    fn from_dev(value: u64) -> Self {
        Self {
            major: rustix::fs::major(value),
            minor: rustix::fs::minor(value),
        }
    }
}

fn number(value: &str) -> io::Result<u32> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("Invalid mount number"));
    }
    value.parse().map_err(|_| invalid("Mount number overflow"))
}

fn path(value: &str) -> io::Result<String> {
    let mut bytes = Vec::new();
    let mut index = 0;
    while index < value.len() {
        let b = value.as_bytes()[index];
        if b == b'\\' {
            let code = value
                .as_bytes()
                .get(index + 1..index + 4)
                .ok_or_else(|| invalid("Truncated mount escape"))?;
            bytes.push(match code {
                b"040" => b' ',
                b"011" => b'\t',
                b"012" => b'\n',
                b"134" => b'\\',
                _ => return Err(invalid("Unknown mount escape")),
            });
            index += 4;
        } else {
            if b <= b' ' || b == 127 {
                return Err(invalid("Unescaped mount path character"));
            }
            bytes.push(b);
            index += 1;
        }
    }
    let result = String::from_utf8(bytes).map_err(|_| invalid("Non UTF8 mount path"))?;
    if !result.starts_with('/')
        || result.len() > 4096
        || result != "/"
            && result[1..]
                .split('/')
                .any(|component| matches!(component, "" | "." | ".."))
    {
        return Err(invalid("Mount path is not canonical and absolute"));
    }
    Ok(result)
}

#[derive(Debug, Clone)]
struct Mount {
    id: u32,
    parent: u32,
    device: Device,
    root: String,
    point: String,
    options: String,
    filesystem: String,
    super_options: String,
}

fn options(value: &str) -> io::Result<()> {
    let mut unique = BTreeSet::new();
    for option in value.split(',') {
        if option.is_empty() || !unique.insert(option) {
            return Err(invalid("Invalid or duplicate mount option"));
        }
    }
    if unique.contains("ro") == unique.contains("rw") {
        return Err(invalid("Missing or conflicting mount access mode"));
    }
    Ok(())
}

impl Mount {
    fn has(&self, option: &str) -> bool {
        self.super_options.split(',').any(|value| value == option)
    }

    fn require(
        &self,
        device: Device,
        root: &str,
        filesystem: &str,
        readonly: bool,
    ) -> io::Result<()> {
        let mode = if readonly { "ro" } else { "rw" };
        if self.device != device
            || self.root != root
            || self.filesystem != filesystem
            || !self.options.split(',').any(|value| value == mode)
            || !self.super_options.split(',').any(|value| value == mode)
        {
            return Err(invalid(
                "Mount does not match its required source and access mode",
            ));
        }
        Ok(())
    }
}

struct Table {
    mounts: Vec<Mount>,
}

impl Table {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.is_empty()
            || bytes.len() > MAX_MOUNTINFO
            || !bytes.ends_with(b"\n")
            || bytes.contains(&0)
        {
            return Err(invalid("Mount table is missing, oversized or truncated"));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("Invalid mount table text"))?;
        let mut mounts = Vec::new();
        let mut ids = BTreeSet::new();
        for line in text.lines() {
            if mounts.len() == MAX_MOUNTS || line.len() > 16384 {
                return Err(invalid("Mount table limit exceeded"));
            }
            let fields: Vec<_> = line.split(' ').collect();
            let separator = fields
                .iter()
                .position(|field| *field == "-")
                .ok_or_else(|| invalid("Missing mount separator"))?;
            if separator < 6 || fields.len() != separator + 4 || fields.contains(&"") {
                return Err(invalid("Malformed mount table entry"));
            }
            let id = number(fields[0])?;
            let parent = number(fields[1])?;
            if id == 0 || parent == 0 || id == parent && fields[4] != "/" || !ids.insert(id) {
                return Err(invalid("Duplicate or invalid mount identity"));
            }
            options(fields[5])?;
            options(fields[separator + 3])?;
            mounts.push(Mount {
                id,
                parent,
                device: Device::parse(fields[2])?,
                root: path(fields[3])?,
                point: path(fields[4])?,
                options: fields[5].to_owned(),
                filesystem: fields[separator + 1].to_owned(),
                super_options: fields[separator + 3].to_owned(),
            });
        }
        Ok(Self { mounts })
    }

    fn at(&self, point: &str) -> io::Result<&Mount> {
        let mut found = self.mounts.iter().filter(|mount| mount.point == point);
        let result = found
            .next()
            .ok_or_else(|| invalid("Required mount is absent"))?;
        if found.next().is_some() {
            return Err(invalid("Stacked mounts make the required source ambiguous"));
        }
        if point != "/" {
            let parent = self
                .mounts
                .iter()
                .filter(|mount| below(point, &mount.point))
                .max_by_key(|mount| mount.point.len())
                .ok_or_else(|| invalid("Required mount has no visible parent"))?;
            if result.parent != parent.id
                || self
                    .mounts
                    .iter()
                    .filter(|mount| mount.point == parent.point)
                    .count()
                    != 1
            {
                return Err(invalid(
                    "Required mount is hidden or has an unexpected parent",
                ));
            }
        }
        Ok(result)
    }
}

fn below(point: &str, ancestor: &str) -> bool {
    point != ancestor
        && (ancestor == "/"
            || point
                .strip_prefix(ancestor)
                .is_some_and(|tail| tail.starts_with('/')))
}

/// Linux UAPI linux/loop.h, LOOP_GET_STATUS64. All fields are initialized before
/// ioctl, and only the kernel's numeric identity is used, never lo_file_name.
#[repr(C)]
struct LoopInfo64 {
    device: u64,
    inode: u64,
    rdevice: u64,
    offset: u64,
    size_limit: u64,
    number: u32,
    encrypt_type: u32,
    encrypt_key_size: u32,
    flags: u32,
    file_name: [u8; 64],
    crypt_name: [u8; 64],
    encrypt_key: [u8; 32],
    init: [u64; 2],
}

#[derive(Debug, Clone, Copy)]
struct Loop {
    device: Device,
    backing_device: Device,
    backing_inode: u64,
    offset: u64,
    size_limit: u64,
    readonly: bool,
    encrypt_type: u32,
}

fn loop_status(file: &File) -> io::Result<Loop> {
    let metadata = file.metadata()?;
    if !metadata.file_type().is_block_device() {
        return Err(invalid("System loop reference is not a block device"));
    }
    let device = Device::from_dev(metadata.rdev());
    if device.major != 7 {
        return Err(invalid(
            "System reference is not an unpartitioned Linux loop device",
        ));
    }
    // SAFETY: LoopInfo64 is the 232 byte Linux UAPI structure, composed entirely
    // of integer fields. Zero initializes every field including padding. This
    // getter ioctl writes within that buffer and never configures a loop device.
    let mut info: LoopInfo64 = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::ioctl(file.as_raw_fd(), 0x4c05, &mut info) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if device.minor != info.number {
        return Err(invalid("Loop number differs from its device identity"));
    }
    Ok(Loop {
        device,
        backing_device: Device::from_dev(info.device),
        backing_inode: info.inode,
        offset: info.offset,
        size_limit: info.size_limit,
        readonly: info.flags & 1 != 0,
        encrypt_type: info.encrypt_type,
    })
}

#[derive(Debug, Clone, Copy)]
struct Expected {
    image_device: Device,
    image_inode: u64,
    volume_device: Device,
    esp_device: Device,
    generation: u64,
}

fn validate(table: &Table, loop_info: Loop, expected: Expected) -> io::Result<()> {
    if expected.generation == 0
        || expected.image_inode == 0
        || loop_info.device.major != 7
        || loop_info.backing_device != expected.image_device
        || loop_info.backing_inode != expected.image_inode
        || expected.image_device != expected.volume_device
        || !loop_info.readonly
        || loop_info.offset != 0
        || loop_info.size_limit != 0
        || loop_info.encrypt_type != 0
        || loop_info.device == expected.volume_device
        || expected.esp_device == expected.volume_device
        || expected.esp_device == loop_info.device
    {
        return Err(invalid(
            "Loop does not map the exact verified whole image read only on the Linos volume",
        ));
    }
    let root = table.at("/")?;
    root.require(loop_info.device, "/", "ext4", true)?;
    if !root.has("noload") && !root.has("norecovery") {
        return Err(invalid("System root permits journal replay"));
    }
    table
        .at(VOLUME)?
        .require(expected.volume_device, "/", "ext4", false)?;
    table
        .at("/efi")?
        .require(expected.esp_device, "/", "vfat", false)?;
    let config = format!("/State/config/{}", expected.generation);
    let binds = [
        ("/etc", config.as_str(), expected.volume_device, false),
        ("/home", "/Users", expected.volume_device, false),
        ("/root", "/Users/root", expected.volume_device, false),
        ("/var", "/State/var", expected.volume_device, false),
        ("/usr/local", "/State/local", expected.volume_device, false),
        ("/srv", "/State/srv", expected.volume_device, false),
        ("/tmp", "/State/tmp/tmp", expected.volume_device, false),
        (
            "/var/tmp",
            "/State/tmp/var-tmp",
            expected.volume_device,
            false,
        ),
        ("/var/lib/dpkg", "/var/lib/dpkg", loop_info.device, true),
        (
            "/var/lib/apt/extended_states",
            "/var/lib/apt/extended_states",
            loop_info.device,
            true,
        ),
    ];
    for (point, source, device, readonly) in binds {
        table.at(point)?.require(device, source, "ext4", readonly)?;
    }
    for mount in &table.mounts {
        if [
            "/etc",
            "/var/lib/dpkg",
            "/var/lib/apt/extended_states",
            "/efi",
            VOLUME,
        ]
        .iter()
        .any(|point| below(&mount.point, point))
        {
            return Err(invalid(
                "Nested mount masks a protected configuration, package or volume source",
            ));
        }
        let writable_local = mount.point == "/usr/local" || below(&mount.point, "/usr/local");
        if !writable_local
            && ["/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64"]
                .iter()
                .any(|point| mount.point == *point || below(&mount.point, point))
        {
            return Err(invalid("Mount masks immutable system code"));
        }
    }
    Ok(())
}

fn read_mountinfo() -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open("/proc/self/mountinfo")?
        .take(MAX_MOUNTINFO as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_MOUNTINFO {
        return Err(invalid("Mount table exceeds inspection limit"));
    }
    Ok(bytes)
}

fn layout_copies(layout: &Layout) -> io::Result<()> {
    for path in [
        "/efi/EFI/Linos/volume/layout",
        "/run/linos/volume/State/var/lib/linos/storage/layout",
    ] {
        if super::records::read_layout(std::path::Path::new(path))? != *layout {
            return Err(invalid("Installed layout and EFI bootstrap copy disagree"));
        }
    }
    Ok(())
}

/// Inspect fixed native paths. The selected configuration generation must come
/// from the locked journal, not a boot argument. This does not approve health:
/// journal/entry, metadata schemas and receipts still require validation by the
/// boot path. No caller supplied sysfs/proc or firmware overrides.
pub fn inspect(layout: &Layout, artifacts: &VerifiedArtifacts, generation: u64) -> io::Result<()> {
    layout.require_plain()?;
    let before = read_mountinfo()?;
    let table = Table::parse(&before)?;
    // Fail on the baseline or an ordinary host before opening any device.
    let root = table.at("/")?;
    root.require(root.device, "/", "ext4", true)?;
    if !root.has("noload") && !root.has("norecovery") {
        return Err(invalid("Not a layout 2 read only system root"));
    }
    let loop_file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open("/dev/linos-system")?;
    let first_loop = loop_status(&loop_file)?;
    let image = artifacts.system_image().metadata()?;
    let expected = Expected {
        image_device: Device::from_dev(image.dev()),
        image_inode: image.ino(),
        volume_device: table.at(VOLUME)?.device,
        esp_device: table.at("/efi")?.device,
        generation,
    };
    validate(&table, first_loop, expected)?;
    layout_copies(layout)?;
    let identity = identity::inspect(layout)?;
    if identity.volume != expected.volume_device || identity.esp != expected.esp_device {
        return Err(invalid(
            "Mounted devices differ from the independently probed disk identity",
        ));
    }
    for point in [
        "/",
        VOLUME,
        "/efi",
        "/etc",
        "/home",
        "/root",
        "/var",
        "/usr/local",
        "/srv",
        "/tmp",
        "/var/tmp",
        "/var/lib/dpkg",
        "/var/lib/apt/extended_states",
    ] {
        if Device::from_dev(fs::metadata(point)?.dev()) != table.at(point)?.device {
            return Err(invalid("Observed path differs from the mount table"));
        }
    }
    validate(&table, loop_status(&loop_file)?, expected)?;
    layout_copies(layout)?;
    if before != read_mountinfo()? {
        return Err(invalid("Mount table changed during inspection"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
