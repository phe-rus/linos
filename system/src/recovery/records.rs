//! Strict versioned storage metadata. Parsing alone does not authenticate media
//! or inspect mounts.
pub mod deployment;
pub mod state;
pub mod transaction;

use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_VALUE_BYTES: usize = 4096;
const LAYOUT_KEYS: [&str; 13] = [
    "kind",
    "format",
    "volume_uuid",
    "disk_guid",
    "esp_partuuid",
    "recovery_partuuid",
    "linos_partuuid",
    "filesystem",
    "filesystem_uuid",
    "encryption",
    "luks_uuid",
    "device_key_sha256",
    "baseline_manifest_sha256",
];

fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn ordered<'a, const N: usize>(bytes: &'a [u8], keys: [&str; N]) -> io::Result<[&'a str; N]> {
    if bytes.is_empty()
        || bytes.len() > MAX_RECORD_BYTES
        || !bytes.ends_with(b"\n")
        || !bytes
            .iter()
            .all(|b| *b == b'\n' || (b' '..=b'~').contains(b))
    {
        return Err(invalid(
            "Record requires bounded ASCII text and a final newline",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(invalid)?;
    let mut lines = text[..text.len() - 1].split('\n');
    let mut values = [""; N];
    for (index, expected) in keys.iter().enumerate() {
        let (key, value) = lines
            .next()
            .and_then(|line| line.split_once('='))
            .ok_or_else(|| invalid(format!("Missing record field: {expected}")))?;
        if key != *expected || value.is_empty() || value.len() > MAX_VALUE_BYTES {
            return Err(invalid(format!(
                "Invalid or out of order field: {expected}"
            )));
        }
        values[index] = value;
    }
    if lines.next().is_some() {
        return Err(invalid("Record contains extra or duplicate fields"));
    }
    Ok(values)
}

pub(super) fn uuid(value: &str) -> io::Result<String> {
    let valid = value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        && value != "00000000-0000-0000-0000-000000000000";
    if !valid {
        return Err(invalid(
            "Layout UUID must be canonical, lowercase and nonzero",
        ));
    }
    Ok(value.to_owned())
}

fn hash(value: &str) -> io::Result<String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("Record digest must be lowercase SHA256"));
    }
    Ok(value.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Encryption {
    None,
    Luks2 { uuid: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    volume_uuid: String,
    disk_guid: String,
    partitions: [String; 3],
    filesystem_uuid: String,
    encryption: Encryption,
    device_key_sha256: String,
    baseline_manifest_sha256: String,
}

impl Layout {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(bytes, LAYOUT_KEYS)?;
        if f[0] != "linos-layout" || f[1] != "2" || f[7] != "ext4" {
            return Err(invalid("Unsupported layout kind, version or filesystem"));
        }
        let partitions = [uuid(f[4])?, uuid(f[5])?, uuid(f[6])?];
        if partitions[0] == partitions[1]
            || partitions[0] == partitions[2]
            || partitions[1] == partitions[2]
        {
            return Err(invalid("Layout partition UUIDs must be distinct"));
        }
        let encryption = match (f[9], f[10]) {
            ("none", "none") => Encryption::None,
            ("luks2", value) => Encryption::Luks2 { uuid: uuid(value)? },
            _ => return Err(invalid("Unknown or inconsistent encryption fields")),
        };
        Ok(Self {
            volume_uuid: uuid(f[2])?,
            disk_guid: uuid(f[3])?,
            partitions,
            filesystem_uuid: uuid(f[8])?,
            encryption,
            device_key_sha256: hash(f[11])?,
            baseline_manifest_sha256: hash(f[12])?,
        })
    }

    /// Recognizing encrypted metadata is not enabling the unqualified boot path.
    pub fn require_plain(&self) -> io::Result<()> {
        if self.encryption != Encryption::None {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "LUKS2 deployment is not qualified",
            ));
        }
        Ok(())
    }

    pub fn volume_uuid(&self) -> &str {
        &self.volume_uuid
    }
    pub fn disk_guid(&self) -> &str {
        &self.disk_guid
    }
    pub fn esp_partuuid(&self) -> &str {
        &self.partitions[0]
    }
    pub fn recovery_partuuid(&self) -> &str {
        &self.partitions[1]
    }
    pub fn linos_partuuid(&self) -> &str {
        &self.partitions[2]
    }
    pub fn filesystem_uuid(&self) -> &str {
        &self.filesystem_uuid
    }
    pub fn encryption(&self) -> &Encryption {
        &self.encryption
    }
    pub fn device_key_sha256(&self) -> &str {
        &self.device_key_sha256
    }
    pub fn baseline_manifest_sha256(&self) -> &str {
        &self.baseline_manifest_sha256
    }

    pub fn encode(&self) -> String {
        let (encryption, luks_uuid) = match &self.encryption {
            Encryption::None => ("none", "none"),
            Encryption::Luks2 { uuid } => ("luks2", uuid.as_str()),
        };
        format!(
            "kind=linos-layout\nformat=2\nvolume_uuid={}\ndisk_guid={}\nesp_partuuid={}\nrecovery_partuuid={}\nlinos_partuuid={}\nfilesystem=ext4\nfilesystem_uuid={}\nencryption={encryption}\nluks_uuid={luks_uuid}\ndevice_key_sha256={}\nbaseline_manifest_sha256={}\n",
            self.volume_uuid,
            self.disk_guid,
            self.partitions[0],
            self.partitions[1],
            self.partitions[2],
            self.filesystem_uuid,
            self.device_key_sha256,
            self.baseline_manifest_sha256
        )
    }
}

/// Read one bounded regular file without following a final symlink or reading a
/// FIFO/device. This is metadata inspection, not trusted-directory validation.
pub fn read_layout(path: &Path) -> io::Result<Layout> {
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() || before.nlink() != 1 || before.len() > MAX_RECORD_BYTES as u64 {
        return Err(invalid("Layout must be a bounded standalone regular file"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    let actual = file.metadata()?;
    if !actual.is_file()
        || actual.nlink() != 1
        || actual.dev() != before.dev()
        || actual.ino() != before.ino()
    {
        return Err(invalid("Layout file identity changed during open"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Layout::parse(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::symlink,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    const VOLUME: &str = "11111111-1111-1111-1111-111111111111";
    const DISK: &str = "22222222-2222-2222-2222-222222222222";
    const ESP: &str = "33333333-3333-3333-3333-333333333333";
    const RECOVERY: &str = "44444444-4444-4444-4444-444444444444";
    const LINOS: &str = "55555555-5555-5555-5555-555555555555";
    const FILESYSTEM: &str = "66666666-6666-6666-6666-666666666666";
    const LUKS: &str = "77777777-7777-7777-7777-777777777777";

    fn fixture() -> String {
        format!(
            "kind=linos-layout\nformat=2\nvolume_uuid={VOLUME}\ndisk_guid={DISK}\nesp_partuuid={ESP}\nrecovery_partuuid={RECOVERY}\nlinos_partuuid={LINOS}\nfilesystem=ext4\nfilesystem_uuid={FILESYSTEM}\nencryption=none\nluks_uuid=none\ndevice_key_sha256={}\nbaseline_manifest_sha256={}\n",
            "a".repeat(64),
            "b".repeat(64)
        )
    }

    fn replace_field(text: &str, key: &str, value: &str) -> String {
        text.lines()
            .map(|line| {
                if line.split_once('=').unwrap().0 == key {
                    format!("{key}={value}\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect()
    }

    #[test]
    fn plain_layout_preserves_every_identity_and_canonical_bytes() {
        let text = fixture();
        let layout = Layout::parse(text.as_bytes()).unwrap();
        assert_eq!(layout.volume_uuid(), VOLUME);
        assert_eq!(layout.disk_guid(), DISK);
        assert_eq!(layout.esp_partuuid(), ESP);
        assert_eq!(layout.recovery_partuuid(), RECOVERY);
        assert_eq!(layout.linos_partuuid(), LINOS);
        assert_eq!(layout.filesystem_uuid(), FILESYSTEM);
        assert_eq!(layout.encryption(), &Encryption::None);
        assert_eq!(layout.device_key_sha256(), "a".repeat(64));
        assert_eq!(layout.baseline_manifest_sha256(), "b".repeat(64));
        assert_eq!(layout.encode(), text);
        layout.require_plain().unwrap();
    }

    #[test]
    fn rejects_missing_duplicate_unknown_and_reordered_fields() {
        let text = fixture();
        let lines: Vec<_> = text.split_inclusive('\n').collect();
        for index in 0..lines.len() {
            let mut missing = lines.clone();
            missing.remove(index);
            assert!(Layout::parse(missing.concat().as_bytes()).is_err());
            let mut duplicate = lines.clone();
            duplicate.insert(index, lines[index]);
            assert!(Layout::parse(duplicate.concat().as_bytes()).is_err());
            let mut unknown = lines.clone();
            unknown.insert(index, "unknown=value\n");
            assert!(Layout::parse(unknown.concat().as_bytes()).is_err());
            if index + 1 < lines.len() {
                let mut reordered = lines.clone();
                reordered.swap(index, index + 1);
                assert!(Layout::parse(reordered.concat().as_bytes()).is_err());
            }
        }
        assert!(Layout::parse(format!("{text}unknown=value\n").as_bytes()).is_err());
    }

    #[test]
    fn rejects_noncanonical_text_and_record_limits() {
        let text = fixture();
        let mut malformed = vec![
            String::new(),
            text.trim_end().to_owned(),
            format!("{text}\n"),
            format!("# layout\n{text}"),
            text.replace('\n', "\r\n"),
            text.replace("format=2", "format=2\0"),
            text.replace("ext4", "ext４"),
            text.replace("format=2", "format=\t2"),
            replace_field(&text, "volume_uuid", &"a".repeat(MAX_VALUE_BYTES + 1)),
        ];
        malformed.push("a".repeat(MAX_RECORD_BYTES + 1));
        for bad in malformed {
            assert!(Layout::parse(bad.as_bytes()).is_err());
        }
        for key in LAYOUT_KEYS {
            assert!(Layout::parse(replace_field(&text, key, "").as_bytes()).is_err());
        }
        // Exercise the generic parser's exact scalar limits independently of
        // layout's more restrictive typed values.
        let max_value = format!("key={}\n", "x".repeat(MAX_VALUE_BYTES));
        assert!(ordered(max_value.as_bytes(), ["key"]).is_ok());
        let too_long = format!("key={}\n", "x".repeat(MAX_VALUE_BYTES + 1));
        assert!(ordered(too_long.as_bytes(), ["key"]).is_err());
    }

    #[test]
    fn rejects_wrong_kinds_versions_filesystems_and_identifiers() {
        let text = fixture();
        for (key, value) in [
            ("kind", "linos-deployment"),
            ("format", "1"),
            ("format", "3"),
            ("format", "02"),
            ("format", "+2"),
            ("format", "2 "),
            ("filesystem", "btrfs"),
            ("filesystem", "EXT4"),
        ] {
            assert!(Layout::parse(replace_field(&text, key, value).as_bytes()).is_err());
        }
        for key in [
            "volume_uuid",
            "disk_guid",
            "esp_partuuid",
            "recovery_partuuid",
            "linos_partuuid",
            "filesystem_uuid",
        ] {
            for value in [
                "none",
                "00000000-0000-0000-0000-000000000000",
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa",
                "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA",
                "aaaaaaaa_aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                "gaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                "../layout",
            ] {
                assert!(
                    Layout::parse(replace_field(&text, key, value).as_bytes()).is_err(),
                    "{key}={value}"
                );
            }
        }
        for key in ["device_key_sha256", "baseline_manifest_sha256"] {
            for value in [
                "none".to_owned(),
                "a".repeat(63),
                "a".repeat(65),
                "A".repeat(64),
                "g".repeat(64),
            ] {
                assert!(Layout::parse(replace_field(&text, key, &value).as_bytes()).is_err());
            }
        }
    }

    #[test]
    fn refuses_each_pair_of_aliased_partitions() {
        let text = fixture();
        for (key, value) in [
            ("recovery_partuuid", ESP),
            ("linos_partuuid", ESP),
            ("linos_partuuid", RECOVERY),
        ] {
            assert!(Layout::parse(replace_field(&text, key, value).as_bytes()).is_err());
        }
    }

    #[test]
    fn recognizes_luks_metadata_but_refuses_unqualified_activation() {
        let text = replace_field(&fixture(), "encryption", "luks2");
        let encrypted = replace_field(&text, "luks_uuid", LUKS);
        let layout = Layout::parse(encrypted.as_bytes()).unwrap();
        assert_eq!(
            layout.encryption(),
            &Encryption::Luks2 {
                uuid: LUKS.to_owned()
            }
        );
        assert_eq!(layout.encode(), encrypted);
        assert_eq!(
            layout.require_plain().unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        for (mode, id) in [
            ("none", LUKS),
            ("luks2", "none"),
            ("luks1", LUKS),
            ("luks2", "00000000-0000-0000-0000-000000000000"),
        ] {
            let bad = replace_field(
                &replace_field(&fixture(), "encryption", mode),
                "luks_uuid",
                id,
            );
            assert!(Layout::parse(bad.as_bytes()).is_err());
        }
    }

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "linos-layout-records-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn reads_only_bounded_standalone_regular_files() {
        let directory = Directory::new();
        let path = directory.0.join("layout");
        let text = fixture();
        fs::write(&path, &text).unwrap();
        assert_eq!(read_layout(&path).unwrap().encode(), text);
        assert!(read_layout(&directory.0).is_err());
        assert!(read_layout(&directory.0.join("missing")).is_err());
        let link = directory.0.join("symlink");
        symlink(&path, &link).unwrap();
        assert!(read_layout(&link).is_err());
        let hardlink = directory.0.join("hardlink");
        fs::hard_link(&path, &hardlink).unwrap();
        assert!(read_layout(&path).is_err());
        assert!(read_layout(&hardlink).is_err());
        fs::remove_file(&hardlink).unwrap();
        fs::write(&path, vec![b'x'; MAX_RECORD_BYTES + 1]).unwrap();
        assert!(read_layout(&path).is_err());
        fs::write(&path, "").unwrap();
        assert!(read_layout(&path).is_err());
    }

    #[test]
    fn rejects_fifo_without_waiting_for_a_writer() {
        let directory = Directory::new();
        let path = directory.0.join("fifo");
        let status = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(read_layout(&path).is_err());
    }
}
