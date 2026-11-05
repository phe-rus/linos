use std::{
    collections::BTreeSet,
    fs,
    io::{self, ErrorKind},
    path::{Component, Path},
};

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    InspectionOnly,
    Mounted,
    ReadOnly,
    BootDiskUnknown,
    TooSmall,
}

pub fn decide(root_known: bool, mounted: bool, readonly: bool, sectors: u64) -> Decision {
    const MIN_SECTORS: u64 = 16 * 1024 * 1024 * 1024 / 512;
    if !root_known {
        Decision::BootDiskUnknown
    } else if mounted {
        Decision::Mounted
    } else if readonly {
        Decision::ReadOnly
    } else if sectors < MIN_SECTORS {
        Decision::TooSmall
    } else {
        Decision::InspectionOnly
    }
}

fn parent_disk_from_sysfs(path: &Path) -> Option<String> {
    let mut parts = path.components();
    while let Some(part) = parts.next() {
        if part == Component::Normal("block".as_ref()) {
            return parts.next().and_then(|item| match item {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            });
        }
    }
    None
}

fn mount_devices(mountinfo: &str) -> (Option<String>, BTreeSet<String>) {
    let mut root = None;
    let mut protected = BTreeSet::new();
    for line in mountinfo.lines() {
        let fields: Vec<_> = line.split_whitespace().take(5).collect();
        if fields.len() != 5 {
            continue;
        }
        let (dev, mountpoint) = (fields[2], fields[4]);
        let parent = fs::canonicalize(format!("/sys/dev/block/{dev}"))
            .ok()
            .and_then(|path| parent_disk_from_sysfs(&path));
        if mountpoint == "/" {
            root = parent.clone();
        }
        if let Some(parent) = parent {
            protected.insert(parent);
        }
    }
    (root, protected)
}

pub fn inspect(device: &str, sys_block: &Path) -> io::Result<()> {
    let name = device
        .strip_prefix("/dev/")
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Expected a /dev/ device"))?;
    if !super::visible_block_device(name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Not a supported whole-disk device name",
        ));
    }

    let root = sys_block.join(name);
    let read_number = |field: &str| -> io::Result<u64> {
        fs::read_to_string(root.join(field))?
            .trim()
            .parse::<u64>()
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
    };
    let sectors = read_number("size")?;
    let readonly = read_number("ro")? != 0;
    let removable = read_number("removable")? != 0;
    let mountinfo = fs::read_to_string("/proc/self/mountinfo")?;
    let (root_disk, mounted) = mount_devices(&mountinfo);

    // An unrecognized root disk may be layered through dm-crypt, RAID,
    // or a container. Never identify another drive as safe in that case.
    let root_known = root_disk
        .as_deref()
        .is_some_and(super::visible_block_device);
    let result = decide(root_known, mounted.contains(name), readonly, sectors);

    println!("Linos Recovery: installation inspection");
    println!("  Device: {device}");
    println!(
        "  Capacity: {:.1} GiB",
        sectors as f64 * 512.0 / 1_073_741_824.0
    );
    println!("  Removable: {removable}");
    println!("  Read-only: {readonly}");
    println!("  Active system disk identified: {root_known}");
    println!("  Planning status: {result:?}");
    println!("  Installation status: DISABLED");
    println!("No partition table, filesystem or storage contents were changed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_uncertain_boot_device() {
        assert_eq!(
            decide(false, false, false, u64::MAX),
            Decision::BootDiskUnknown
        );
    }

    #[test]
    fn blocks_mounted_and_readonly_targets() {
        assert_eq!(decide(true, true, false, u64::MAX), Decision::Mounted);
        assert_eq!(decide(true, false, true, u64::MAX), Decision::ReadOnly);
    }

    #[test]
    fn enforces_minimum_space_and_never_enables_installation() {
        assert_eq!(decide(true, false, false, 1024), Decision::TooSmall);
        assert_eq!(
            decide(true, false, false, u64::MAX),
            Decision::InspectionOnly
        );
    }

    #[test]
    fn sysfs_parent_disk_identification() {
        let path = Path::new("/devices/pci0000:00/block/vda/vda2");
        assert_eq!(parent_disk_from_sysfs(path).as_deref(), Some("vda"));
        assert_eq!(
            parent_disk_from_sysfs(Path::new("/devices/virtual/tty")).as_deref(),
            None
        );
    }
}
