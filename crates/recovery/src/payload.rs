use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
    process::Command,
};

const PAYLOAD_DIR: &str = "/usr/share/linos/install";

fn value<'a>(manifest: &'a str, key: &str) -> Option<&'a str> {
    manifest
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(found, value)| (found == key).then_some(value))
}

fn valid_manifest(manifest: &str) -> bool {
    value(manifest, "format") == Some("1")
        && value(manifest, "edition") == Some("server")
        && value(manifest, "architecture") == Some("amd64")
        && value(manifest, "build").is_some_and(|build| {
            !build.is_empty()
                && build
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

fn expected_checksums(contents: &str) -> bool {
    const EXPECTED: [&str; 4] = ["rootfs.tar.gz", "kernel", "initramfs", "manifest"];
    let lines: Vec<_> = contents.lines().collect();
    if lines.len() != EXPECTED.len() {
        return false;
    }
    lines.iter().zip(EXPECTED).all(|(line, expected)| {
        line.split_once("  ").is_some_and(|(hash, name)| {
            hash.len() == 64
                && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                && name == expected
        })
    })
}

pub fn verify() -> io::Result<()> {
    let path = Path::new(PAYLOAD_DIR);
    let manifest = fs::read_to_string(path.join("manifest"))?;
    if !valid_manifest(&manifest) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Wrong edition, architecture, or payload format",
        ));
    }

    let sums = fs::read_to_string(path.join("SHA256SUMS"))?;
    if !expected_checksums(&sums) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Offline payload checksum list has unexpected entries",
        ));
    }

    let status = Command::new("/usr/bin/sha256sum")
        .arg("--check")
        .arg("--status")
        .arg("SHA256SUMS")
        .current_dir(path)
        .status()?;
    if !status.success() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Offline payload checksum verification failed",
        ));
    }

    println!("Linos Server offline payload");
    println!("  Edition: server");
    println!("  Architecture: amd64");
    println!("  Build: {}", value(&manifest, "build").unwrap_or("unknown"));
    println!("  Checksums: valid");
    println!("  Authenticity: NOT VERIFIED (development payload is unsigned)");
    println!("  Installation: DISABLED");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_image_edition_and_architecture() {
        let valid = "format=1\nedition=server\narchitecture=amd64\nbuild=20261105.2\n";
        assert!(valid_manifest(valid));
        assert!(!valid_manifest(&valid.replace("server", "desktop")));
        assert!(!valid_manifest(&valid.replace("amd64", "arm64")));
    }

    #[test]
    fn checksum_list_does_not_allow_extra_paths() {
        let hash = "a".repeat(64);
        let checksums = format!(
            "{hash}  rootfs.tar.gz\n{hash}  kernel\n{hash}  initramfs\n{hash}  manifest\n"
        );
        assert!(expected_checksums(&checksums));
        assert!(!expected_checksums(&checksums.replace("kernel", "../secret")));
    }

    #[test]
    fn rejects_invalid_build_identifiers() {
        let invalid = "format=1\nedition=server\narchitecture=amd64\nbuild=../unsafe\n";
        assert!(!valid_manifest(invalid));
    }
}
