use super::*;

const LOOP: Device = Device { major: 7, minor: 0 };
const VOLUME_DEV: Device = Device { major: 8, minor: 3 };
const ESP_DEV: Device = Device { major: 8, minor: 1 };

fn fixture() -> String {
    [
        "10 1 7:0 / / ro,relatime - ext4 /dev/linos-system ro,norecovery",
        "11 10 0:30 / /run rw,nosuid - tmpfs tmpfs rw",
        "12 11 8:3 / /run/linos/volume rw,relatime - ext4 /dev/vda3 rw",
        "13 10 8:1 / /efi rw,nosuid - vfat /dev/vda1 rw",
        "20 10 8:3 /State/config/17 /etc rw,relatime - ext4 /dev/vda3 rw",
        "21 10 8:3 /Users /home rw,relatime - ext4 /dev/vda3 rw",
        "22 10 8:3 /Users/root /root rw,relatime - ext4 /dev/vda3 rw",
        "23 10 8:3 /State/var /var rw,relatime - ext4 /dev/vda3 rw",
        "24 10 8:3 /State/local /usr/local rw,relatime - ext4 /dev/vda3 rw",
        "25 10 8:3 /State/srv /srv rw,relatime - ext4 /dev/vda3 rw",
        "26 10 8:3 /State/tmp/tmp /tmp rw,relatime - ext4 /dev/vda3 rw",
        "27 23 8:3 /State/tmp/var-tmp /var/tmp rw,relatime - ext4 /dev/vda3 rw",
        "28 23 7:0 /var/lib/dpkg /var/lib/dpkg ro,relatime - ext4 /dev/linos-system ro,norecovery",
        "29 23 7:0 /var/lib/apt/extended_states /var/lib/apt/extended_states ro,relatime - ext4 /dev/linos-system ro,norecovery",
    ].join("\n") + "\n"
}

fn loop_info() -> Loop {
    Loop {
        device: LOOP,
        backing_device: VOLUME_DEV,
        backing_inode: 1234,
        offset: 0,
        size_limit: 0,
        readonly: true,
        encrypt_type: 0,
    }
}

fn expected() -> Expected {
    Expected {
        image_device: VOLUME_DEV,
        image_inode: 1234,
        volume_device: VOLUME_DEV,
        esp_device: ESP_DEV,
        generation: 17,
    }
}

fn check(text: &str) -> io::Result<()> {
    validate(&Table::parse(text.as_bytes())?, loop_info(), expected())
}

#[test]
fn accepts_only_the_complete_plain_loop_and_bind_map() {
    check(&fixture()).unwrap();
    check(&fixture().replace("norecovery", "noload")).unwrap();
    check(&fixture().replace("10 1 7:0", "10 10 7:0")).unwrap();
    // Device identity comes from the kernel numbers, not the source display name.
    check(&fixture().replace("/dev/vda3", "untrusted-label")).unwrap();
    check(&fixture().replace("rw,relatime - ext4", "rw,relatime shared:3 master:2 - ext4"))
        .unwrap();
}

#[test]
fn rejects_every_missing_stacked_or_wrong_source_managed_mount() {
    let text = fixture();
    for line in text.lines().filter(|line| !line.contains(" /run ")) {
        assert!(
            check(&text.replace(&format!("{line}\n"), "")).is_err(),
            "missing {line}"
        );
        let mut words: Vec<_> = line.split(' ').map(str::to_owned).collect();
        words[0] = "90".into();
        assert!(
            check(&format!("{text}{}\n", words.join(" "))).is_err(),
            "stacked {line}"
        );
        words[0] = line.split(' ').next().unwrap().into();
        words[3] = "/wrong-source".into();
        assert!(
            check(&text.replace(line, &words.join(" "))).is_err(),
            "source {line}"
        );
        words[3] = line.split(' ').nth(3).unwrap().into();
        words[2] = "8:99".into();
        assert!(
            check(&text.replace(line, &words.join(" "))).is_err(),
            "device {line}"
        );
    }
}

#[test]
fn rejects_writable_system_readonly_state_and_journal_replay() {
    let text = fixture();
    for bad in [
        text.replace("/ / ro,relatime", "/ / rw,relatime"),
        text.replace("ro,norecovery", "rw,norecovery"),
        text.replace("ro,norecovery", "ro"),
        text.replace("/etc rw,relatime", "/etc ro,relatime"),
        text.replace("/dev/vda3 rw", "/dev/vda3 ro"),
        text.replace("/var/lib/dpkg ro,relatime", "/var/lib/dpkg rw,relatime"),
        text.replace("/ / ro,relatime", "/ / ro,rw"),
        text.replace("ro,norecovery", "ro,ro,norecovery"),
    ] {
        assert!(check(&bad).is_err());
    }
}

#[test]
fn rejects_wrong_loop_backing_identity_offset_limit_and_access() {
    let table = Table::parse(fixture().as_bytes()).unwrap();
    for bad in [
        Loop {
            backing_device: ESP_DEV,
            ..loop_info()
        },
        Loop {
            backing_inode: 1235,
            ..loop_info()
        },
        Loop {
            offset: 4096,
            ..loop_info()
        },
        Loop {
            size_limit: 4096,
            ..loop_info()
        },
        Loop {
            readonly: false,
            ..loop_info()
        },
        Loop {
            encrypt_type: 1,
            ..loop_info()
        },
        Loop {
            device: VOLUME_DEV,
            ..loop_info()
        },
    ] {
        assert!(validate(&table, bad, expected()).is_err());
    }
    for bad in [
        Expected {
            generation: 0,
            ..expected()
        },
        Expected {
            generation: 16,
            ..expected()
        },
        Expected {
            image_device: ESP_DEV,
            ..expected()
        },
        Expected {
            esp_device: VOLUME_DEV,
            ..expected()
        },
        Expected {
            esp_device: LOOP,
            ..expected()
        },
    ] {
        assert!(validate(&table, loop_info(), bad).is_err());
    }
}

#[test]
fn rejects_state_package_database_and_stale_configuration_substitution() {
    let text = fixture();
    let wrong_packages = text.replace("28 23 7:0 /var/lib/dpkg", "28 23 8:3 /State/var/lib/dpkg");
    assert!(check(&wrong_packages).is_err());
    assert!(check(&text.replace("/State/config/17", "/State/config/16")).is_err());
    assert!(check(&text.replace("/State/config/17", "/State/config/17/passwd")).is_err());
}

#[test]
fn rejects_hidden_mounts_ambiguous_parents_and_protected_descendants() {
    let text = fixture();
    assert!(check(&text.replace("28 23 ", "28 10 ")).is_err());
    assert!(check(&format!("{text}90 10 0:30 / /run rw - tmpfs tmpfs rw\n")).is_err());
    for (point, parent) in [
        ("/etc/shadow", 20),
        ("/var/lib/dpkg/status", 28),
        ("/var/lib/apt/extended_states/hidden", 29),
        ("/run/linos/volume/State", 12),
        ("/efi/EFI", 13),
        ("/usr", 10),
        ("/usr/bin", 10),
        ("/lib64", 10),
    ] {
        assert!(
            check(&format!(
                "{text}90 {parent} 0:30 / {point} rw - tmpfs tmpfs rw\n"
            ))
            .is_err(),
            "{point}"
        );
    }
    // Legitimate service and runtime mounts outside the protected sources are
    // left to service/schema policy rather than being called corrupt ancestry.
    check(&format!(
        "{text}90 23 0:31 / /var/lib/containers rw - tmpfs tmpfs rw\n"
    ))
    .unwrap();
    check(&format!(
        "{text}90 24 0:31 / /usr/local/owner-data rw - tmpfs tmpfs rw\n"
    ))
    .unwrap();
}

#[test]
fn decodes_kernel_path_escapes_and_rejects_malformed_tables() {
    assert_eq!(path(r"/a\040b\134c\011d\012e").unwrap(), "/a b\\c\td\ne");
    for value in [
        r"/bad\",
        r"/bad\04",
        r"/bad\000",
        r"/bad\999",
        "/a//b",
        "/a/../b",
        "/a/./b",
        "/a/",
        "relative",
    ] {
        assert!(path(value).is_err(), "{value}");
    }
    let text = fixture();
    for bad in [
        String::new(),
        text.trim_end().into(),
        text.replace("10 1 ", "0 1 "),
        text.replace("20 10 ", "20 20 "),
        text.replace("20 10 ", "10 10 "),
        text.replace("7:0", "4294967296:0"),
        text.replace("7:0", "7:+0"),
        text.replace(" - ", " "),
        text.replace(" ro,relatime ", "  ro,relatime "),
        text.replace("/etc", "/etc\0"),
    ] {
        assert!(Table::parse(bad.as_bytes()).is_err());
    }
    assert!(Table::parse(&vec![b'x'; MAX_MOUNTINFO + 1]).is_err());
    let excessive: String = (1..=MAX_MOUNTS + 1)
        .map(|id| format!("{id} {} 0:1 / /unrelated{id} rw - tmpfs tmpfs rw\n", id + 1))
        .collect();
    assert!(Table::parse(excessive.as_bytes()).is_err());
    // Unrelated names may contain escaped spaces without changing tokenization.
    check(&format!(
        "{text}90 11 0:31 / /run/with\\040space rw - tmpfs tmpfs rw\n"
    ))
    .unwrap();
}

#[test]
fn parses_live_mountinfo_and_refuses_regular_files_as_loop_devices() {
    Table::parse(&read_mountinfo().unwrap()).unwrap();
    let ordinary = File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(loop_status(&ordinary).is_err());
    assert_eq!(std::mem::size_of::<LoopInfo64>(), 232);
    assert_eq!(std::mem::offset_of!(LoopInfo64, flags), 52);
    assert_eq!(std::mem::offset_of!(LoopInfo64, init), 216);
}
