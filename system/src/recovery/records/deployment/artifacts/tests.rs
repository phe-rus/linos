use super::super::tests::{Signer, field, manifest, sums};
use super::*;
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture {
    root: PathBuf,
    payload: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Sparse filesystem images stay on disk backed workspace storage.
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/deployment-artifact-tests");
        fs::create_dir_all(&base).unwrap();
        let root = base.join(format!(
            "fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let payload = root.join("deployment");
        fs::create_dir(&payload).unwrap();
        fs::set_permissions(&payload, fs::Permissions::from_mode(0o700)).unwrap();
        let fixture = Self { root, payload };
        File::create(fixture.image())
            .unwrap()
            .set_len(32 * 1024 * 1024)
            .unwrap();
        fixture.native(
            "/usr/sbin/mke2fs",
            &[
                "-t",
                "ext4",
                "-F",
                "-q",
                "-E",
                "lazy_itable_init=0,lazy_journal_init=0",
            ],
        );
        for name in &ARTIFACT_NAMES[1..] {
            fs::write(fixture.payload.join(name), format!("synthetic {name}\n")).unwrap();
        }
        fixture
    }

    fn image(&self) -> PathBuf {
        self.payload.join("system.img")
    }

    fn native(&self, program: &str, args: &[&str]) {
        let output = Command::new(program)
            .args(args)
            .arg(self.image())
            .env("LC_ALL", "C")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn authenticated(&self) -> OfficialManifest {
        let mut text = manifest();
        for (name, prefix) in ARTIFACT_NAMES.into_iter().zip([
            "system",
            "kernel",
            "initramfs",
            "inventory",
            "applications",
            "services",
            "accounts",
            "extensions",
        ]) {
            let file = File::open(self.payload.join(name)).unwrap();
            text = field(
                &text,
                &format!("{prefix}_bytes"),
                &file.metadata().unwrap().len().to_string(),
            );
            text = field(
                &text,
                &format!("{prefix}_sha256"),
                &sha256_file(&file).unwrap(),
            );
        }
        let signer = Signer::ed25519();
        signer.verify(&text, &sums(&text)).unwrap()
    }

    fn directory(&self) -> File {
        File::open(&self.payload).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn validates_all_signed_files_and_clean_ext4_without_changing_bytes() {
    let fixture = Fixture::new();
    let manifest = fixture.authenticated();
    let before = sha256_file(&File::open(fixture.image()).unwrap()).unwrap();
    let checked = manifest.verify_artifacts(&fixture.directory()).unwrap();
    assert_eq!(checked.deployment_id(), manifest.id());
    assert_eq!(sha256_file(checked.system_image()).unwrap(), before);
    for name in ARTIFACT_NAMES {
        assert!(checked.file(name).is_some());
    }
    assert!(checked.file("../kernel").is_none());
    assert!(checked.file("manifest").is_none());
    checked.revalidate().unwrap();
    assert_eq!(
        sha256_file(&File::open(fixture.image()).unwrap()).unwrap(),
        before
    );
}

#[test]
fn refuses_changed_lengths_and_same_length_tampering_for_each_artifact() {
    let fixture = Fixture::new();
    let manifest = fixture.authenticated();
    for name in ARTIFACT_NAMES {
        let path = fixture.payload.join(name);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let length = file.metadata().unwrap().len();
        file.set_len(length + 1).unwrap();
        assert!(
            manifest.verify_artifacts(&fixture.directory()).is_err(),
            "length {name}"
        );
        file.set_len(length).unwrap();
        let mut original = [0];
        file.read_exact(&mut original).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&[original[0] ^ 1]).unwrap();
        assert!(
            manifest.verify_artifacts(&fixture.directory()).is_err(),
            "digest {name}"
        );
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&original).unwrap();
    }
    // A sparse overlimit file is refused before hashing any of its contents.
    let image = fs::OpenOptions::new()
        .write(true)
        .open(fixture.image())
        .unwrap();
    image.set_len(4 * 1024 * 1024 * 1024 + 1).unwrap();
    assert!(sha256_file(&File::open(fixture.image()).unwrap()).is_err());
}

#[test]
fn refuses_links_fifos_missing_files_and_other_writers() {
    let fixture = Fixture::new();
    let manifest = fixture.authenticated();
    let kernel = fixture.payload.join("kernel");
    let saved = fixture.root.join("saved-kernel");
    fs::rename(&kernel, &saved).unwrap();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    symlink(&saved, &kernel).unwrap();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    fs::remove_file(&kernel).unwrap();
    fs::hard_link(&saved, &kernel).unwrap();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    fs::remove_file(&kernel).unwrap();
    let status = Command::new("mkfifo").arg(&kernel).status().unwrap();
    assert!(status.success());
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    fs::remove_file(&kernel).unwrap();
    fs::create_dir(&kernel).unwrap();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    fs::remove_dir(&kernel).unwrap();
    fs::rename(&saved, &kernel).unwrap();
    fs::set_permissions(&kernel, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    fs::set_permissions(&kernel, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&fixture.payload, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
}

#[test]
fn retained_descriptors_do_not_switch_to_replaced_pathnames() {
    let fixture = Fixture::new();
    let manifest = fixture.authenticated();
    let checked = manifest.verify_artifacts(&fixture.directory()).unwrap();
    let kernel = fixture.payload.join("kernel");
    fs::rename(&kernel, fixture.root.join("original-kernel")).unwrap();
    fs::write(&kernel, "untrusted replacement\n").unwrap();
    let mut original = checked.file("kernel").unwrap().try_clone().unwrap();
    let mut text = String::new();
    original.read_to_string(&mut text).unwrap();
    assert_eq!(text, "synthetic kernel\n");
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    // Revalidation follows retained identities, never the replacement names.
    checked.revalidate().unwrap();
    let mut owner_writer = fs::OpenOptions::new()
        .write(true)
        .open(fixture.root.join("original-kernel"))
        .unwrap();
    owner_writer.write_all(b"modified").unwrap();
    assert!(checked.revalidate().is_err());
}

#[test]
fn refuses_authentic_unclean_and_recovery_required_images_without_repair() {
    for operation in ["set_super_value state 2", "feature needs_recovery"] {
        let fixture = Fixture::new();
        fixture.native("/usr/sbin/debugfs", &["-w", "-R", operation]);
        let manifest = fixture.authenticated();
        let before = sha256_file(&File::open(fixture.image()).unwrap()).unwrap();
        assert!(
            manifest.verify_artifacts(&fixture.directory()).is_err(),
            "{operation}"
        );
        assert_eq!(
            sha256_file(&File::open(fixture.image()).unwrap()).unwrap(),
            before
        );
    }
}

#[test]
fn refuses_authentic_structural_corruption_even_with_a_clean_header() {
    let fixture = Fixture::new();
    // Mark an otherwise free block as allocated without allocating it to a file.
    fixture.native("/usr/sbin/debugfs", &["-w", "-R", "setb 30000"]);
    let header = Command::new("/usr/sbin/dumpe2fs")
        .args(["-h"])
        .arg(fixture.image())
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    clean_header(std::str::from_utf8(&header.stdout).unwrap()).unwrap();
    let manifest = fixture.authenticated();
    let before = sha256_file(&File::open(fixture.image()).unwrap()).unwrap();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
    assert_eq!(
        sha256_file(&File::open(fixture.image()).unwrap()).unwrap(),
        before
    );
}

#[test]
fn refuses_authentic_non_ext4_images() {
    let fixture = Fixture::new();
    fixture.native("/usr/sbin/mke2fs", &["-t", "ext2", "-F", "-q"]);
    let manifest = fixture.authenticated();
    assert!(manifest.verify_artifacts(&fixture.directory()).is_err());
}

#[test]
fn filesystem_header_requires_clean_state_and_no_external_journal() {
    let header =
        "Filesystem state: clean\nFilesystem features: has_journal extent\nJournal inode: 8\n";
    clean_header(header).unwrap();
    for bad in [
        header.replace("clean", "not clean"),
        header.replace("has_journal", "has_journal needs_recovery"),
        header.replace("has_journal", "journal_dev"),
        header.replace("Journal inode: 8\n", ""),
        header.replace("Journal inode: 8", "Journal inode: 0"),
        format!("{header}Journal UUID: 11111111-1111-1111-1111-111111111111\n"),
        format!("{header}Journal device: 0x1234\n"),
        format!("{header}Filesystem state: clean\n"),
        "Filesystem features: extent\n".to_owned(),
    ] {
        assert!(clean_header(&bad).is_err(), "{bad}");
    }
}
