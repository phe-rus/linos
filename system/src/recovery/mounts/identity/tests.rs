use super::*;
use std::{
    io::{Seek, SeekFrom, Write},
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicU64, Ordering},
};

const DISK: Device = Device { major: 8, minor: 0 };
const ESP: Device = Device { major: 8, minor: 1 };
const RECOVERY: Device = Device { major: 8, minor: 2 };
const VOLUME: Device = Device { major: 8, minor: 3 };
const DISK_GUID: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const ESP_ID: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const RECOVERY_ID: &str = "cccccccc-cccc-cccc-cccc-cccccccccccc";
const VOLUME_ID: &str = "dddddddd-dddd-dddd-dddd-dddddddddddd";
const FS_ID: &str = "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee";

fn layout() -> Layout {
    Layout::parse(format!("kind=linos-layout\nformat=2\nvolume_uuid=11111111-1111-1111-1111-111111111111\ndisk_guid={DISK_GUID}\nesp_partuuid={ESP_ID}\nrecovery_partuuid={RECOVERY_ID}\nlinos_partuuid={VOLUME_ID}\nfilesystem=ext4\nfilesystem_uuid={FS_ID}\nencryption=none\nluks_uuid=none\ndevice_key_sha256={}\nbaseline_manifest_sha256={}\n", "a".repeat(64), "b".repeat(64)).as_bytes()).unwrap()
}

fn fixture() -> Vec<Block> {
    let mut blocks = vec![Block {
        device: DISK,
        parent: None,
        partition: None,
        tags: Tags::from([
            ("PTTYPE".into(), "gpt".into()),
            ("PTUUID".into(), DISK_GUID.into()),
        ]),
    }];
    for (index, (device, id, name, kind, filesystem)) in [
        (ESP, ESP_ID, "EFI", ESP_TYPE, "vfat"),
        (RECOVERY, RECOVERY_ID, "Recovery", LINUX_TYPE, "ext4"),
        (VOLUME, VOLUME_ID, "Linos", LINUX_TYPE, "ext4"),
    ]
    .into_iter()
    .enumerate()
    {
        blocks.push(Block {
            device,
            parent: Some(DISK),
            partition: Some(index as u32 + 1),
            tags: Tags::from([
                ("PART_ENTRY_UUID".into(), id.into()),
                ("PART_ENTRY_NUMBER".into(), (index + 1).to_string()),
                ("PART_ENTRY_DISK".into(), "8:0".into()),
                ("PART_ENTRY_SCHEME".into(), "gpt".into()),
                ("PART_ENTRY_NAME".into(), name.into()),
                ("PART_ENTRY_TYPE".into(), kind.into()),
                ("TYPE".into(), filesystem.into()),
            ]),
        });
    }
    blocks[3].tags.insert("UUID".into(), FS_ID.into());
    blocks
}

fn efivar(value: &str) -> Vec<u8> {
    let mut bytes = vec![6, 0, 0, 0];
    for unit in value.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[test]
fn validates_unique_roles_common_disk_filesystem_and_loader_esp() {
    let blocks = fixture();
    let (identity, disk) = validate(&layout(), &blocks, ESP_ID).unwrap();
    assert_eq!(identity.volume, VOLUME);
    assert_eq!(identity.esp, ESP);
    assert_eq!(disk, DISK);
    let mut uppercase = blocks.clone();
    for block in &mut uppercase {
        for key in ["PTUUID", "UUID", "PART_ENTRY_UUID", "PART_ENTRY_TYPE"] {
            if let Some(value) = block.tags.get_mut(key) {
                *value = value.to_ascii_uppercase();
            }
        }
    }
    validate(&layout(), &uppercase, ESP_ID).unwrap();
}

#[test]
fn refuses_duplicate_partition_disk_and_filesystem_identifiers() {
    for index in 0..4 {
        let mut blocks = fixture();
        let mut clone = blocks[index].clone();
        clone.device = Device {
            major: 9,
            minor: index as u32,
        };
        blocks.push(clone);
        assert!(
            validate(&layout(), &blocks, ESP_ID).is_err(),
            "clone {index}"
        );
    }
    let mut blocks = fixture();
    blocks[1].tags.insert("UUID".into(), FS_ID.into());
    assert!(validate(&layout(), &blocks, ESP_ID).is_err());
    let mut blocks = fixture();
    blocks.push(blocks[0].clone());
    assert!(validate(&layout(), &blocks, ESP_ID).is_err());
}

#[test]
fn refuses_missing_roles_wrong_native_values_and_extra_partitions() {
    for index in 0..4 {
        let mut blocks = fixture();
        blocks.remove(index);
        assert!(
            validate(&layout(), &blocks, ESP_ID).is_err(),
            "missing {index}"
        );
    }
    for index in 1..4 {
        for key in [
            "PART_ENTRY_NUMBER",
            "PART_ENTRY_DISK",
            "PART_ENTRY_SCHEME",
            "PART_ENTRY_NAME",
            "PART_ENTRY_TYPE",
            "TYPE",
        ] {
            let mut blocks = fixture();
            blocks[index].tags.insert(key.into(), "wrong".into());
            assert!(
                validate(&layout(), &blocks, ESP_ID).is_err(),
                "wrong {index}/{key}"
            );
        }
        let mut blocks = fixture();
        blocks[index].partition = Some(99);
        assert!(validate(&layout(), &blocks, ESP_ID).is_err());
    }
    let mut blocks = fixture();
    blocks.push(Block {
        device: Device { major: 8, minor: 4 },
        parent: Some(DISK),
        partition: Some(4),
        tags: Tags::new(),
    });
    assert!(validate(&layout(), &blocks, ESP_ID).is_err());
}

#[test]
fn refuses_cross_disk_ancestry_wrong_gpt_filesystem_and_firmware() {
    let mut blocks = fixture();
    blocks[3].parent = Some(Device { major: 9, minor: 0 });
    blocks[3]
        .tags
        .insert("PART_ENTRY_DISK".into(), "9:0".into());
    assert!(validate(&layout(), &blocks, ESP_ID).is_err());
    for (index, key, value) in [
        (0, "PTUUID", FS_ID),
        (0, "PTTYPE", "dos"),
        (3, "UUID", ESP_ID),
    ] {
        let mut blocks = fixture();
        blocks[index].tags.insert(key.into(), value.into());
        assert!(validate(&layout(), &blocks, ESP_ID).is_err());
    }
    assert!(validate(&layout(), &fixture(), RECOVERY_ID).is_err());
    let mut blocks = fixture();
    blocks[0].parent = Some(Device { major: 9, minor: 0 });
    assert!(validate(&layout(), &blocks, ESP_ID).is_err());
}

#[test]
fn decodes_loader_guid_and_refuses_invalid_efivar_bytes() {
    assert_eq!(loader_uuid(&efivar(ESP_ID)).unwrap(), ESP_ID);
    assert_eq!(
        loader_uuid(&efivar(&ESP_ID.to_ascii_uppercase())).unwrap(),
        ESP_ID
    );
    let good = efivar(ESP_ID);
    for bad in [
        vec![],
        good[..good.len() - 1].to_vec(),
        [good.clone(), vec![0, 0]].concat(),
        efivar("not-a-guid"),
        efivar("00000000-0000-0000-0000-000000000000"),
        efivar("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbb\0"),
    ] {
        assert!(loader_uuid(&bad).is_err());
    }
    let mut bad = good.clone();
    bad[4] = 0;
    bad[5] = 0;
    assert!(loader_uuid(&bad).is_err());
    let mut bad = good;
    let length = bad.len();
    bad[length - 2] = 1;
    assert!(loader_uuid(&bad).is_err());
}

#[test]
fn bounds_native_export_and_refuses_duplicate_or_malformed_fields() {
    let good = format!("DEVNAME=/proc/123/fd/7\nPTTYPE=gpt\nPTUUID={DISK_GUID}\n");
    assert_eq!(
        tags(good.as_bytes()).unwrap().get("PTUUID").unwrap(),
        DISK_GUID
    );
    for bad in [
        good.trim_end().to_owned(),
        format!("{good}PTTYPE=gpt\n"),
        "TYPE=\n".into(),
        "=ext4\n".into(),
        "TYPE ext4\n".into(),
        "type=ext4\n".into(),
        "TYPE=ext4\0\n".into(),
        format!("TYPE={}\n", "x".repeat(4097)),
    ] {
        assert!(tags(bad.as_bytes()).is_err());
    }
    assert!(tags(&vec![b'x'; MAX_PROBE + 1]).is_err());
    assert!(
        validate(
            &layout(),
            &vec![fixture()[0].clone(); MAX_DEVICES + 1],
            ESP_ID
        )
        .is_err()
    );
}

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/disk-identity-tests");
        fs::create_dir_all(&base).unwrap();
        let path = base.join(format!(
            "fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn digest(path: &Path) -> Vec<u8> {
    let output = Command::new("/usr/bin/sha256sum")
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    output.stdout
}

#[test]
fn native_gpt_probe_and_corruption_checks_use_only_a_disposable_regular_file() {
    let directory = Directory::new();
    let path = directory.0.join("disk.raw");
    File::create(&path)
        .unwrap()
        .set_len(16 * 1024 * 1024)
        .unwrap();
    let output = Command::new("/usr/sbin/sgdisk")
        .args([
            "--clear",
            &format!("--disk-guid={DISK_GUID}"),
            "--new=1:2048:+1M",
            "--typecode=1:ef00",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file = File::open(&path).unwrap();
    let probed = probe(&file).unwrap();
    assert_eq!(probed.get("PTUUID").unwrap(), DISK_GUID);
    assert_eq!(probed.get("PTTYPE").unwrap(), "gpt");
    let before = digest(&path);
    verify_gpt(&file).unwrap();
    assert_eq!(digest(&path), before);
    // Test each GPT copy independently. sgdisk may internally recover a copy,
    // but the inspector must refuse rather than accept a degraded table.
    let mut writer = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    for offset in [512, 16 * 1024 * 1024 - 512] {
        writer.seek(SeekFrom::Start(offset)).unwrap();
        let mut old = [0];
        writer.read_exact(&mut old).unwrap();
        writer.seek(SeekFrom::Start(offset)).unwrap();
        writer.write_all(b"X").unwrap();
        let before = digest(&path);
        assert!(verify_gpt(&file).is_err());
        assert_eq!(digest(&path), before);
        writer.seek(SeekFrom::Start(offset)).unwrap();
        writer.write_all(&old).unwrap();
    }
}

#[test]
fn native_filesystem_probe_matches_a_disposable_ext4_uuid() {
    let directory = Directory::new();
    let path = directory.0.join("filesystem.img");
    File::create(&path)
        .unwrap()
        .set_len(32 * 1024 * 1024)
        .unwrap();
    let status = Command::new("/usr/sbin/mke2fs")
        .args(["-t", "ext4", "-q", "-F", "-U", FS_ID])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    let before = digest(&path);
    let probed = probe(&File::open(&path).unwrap()).unwrap();
    assert_eq!(probed.get("TYPE").unwrap(), "ext4");
    assert_eq!(probed.get("UUID").unwrap(), FS_ID);
    assert_eq!(digest(&path), before);
    let file = File::open(&path).unwrap();
    let observed = Block {
        device: VOLUME,
        parent: Some(DISK),
        partition: Some(3),
        tags: probe(&file).unwrap(),
    };
    unchanged(&observed, &file).unwrap();
    let output = Command::new("/usr/sbin/tune2fs")
        .args(["-U", ESP_ID])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let changed = digest(&path);
    assert!(unchanged(&observed, &file).is_err());
    assert_eq!(digest(&path), changed);
}
