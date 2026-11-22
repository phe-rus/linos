use linos::recovery::update::{Event, Phase, Slot, image::ImageStore, journal::Journal};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const A: &str = "11111111-1111-4111-8111-111111111111";
const B: &str = "22222222-2222-4222-8222-222222222222";
struct Fixture {
    path: PathBuf,
    root: PathBuf,
    key: PathBuf,
}
fn command(program: &str, args: &[&str], dir: &Path) {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "linos-image-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        command(
            "/usr/bin/openssl",
            &["genpkey", "-algorithm", "ED25519", "-out", "private.pem"],
            &path,
        );
        command(
            "/usr/bin/openssl",
            &[
                "pkey",
                "-in",
                "private.pem",
                "-pubout",
                "-out",
                "public.pem",
            ],
            &path,
        );
        let root = path.join("store");
        fs::create_dir(&root).unwrap();
        let key = path.join("public.pem");
        Self { path, root, key }
    }
    fn payload(&self, name: &str, release: u64) -> PathBuf {
        let path = self.path.join(name);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("system.img"), format!("system image {release}")).unwrap();
        fs::write(path.join("kernel"), format!("kernel {release}")).unwrap();
        fs::write(path.join("initramfs"), format!("initramfs {release}")).unwrap();
        let size = |name| fs::metadata(path.join(name)).unwrap().len();
        fs::write(path.join("manifest"), format!("format=2\nedition=server\narchitecture=amd64\nhardware_profile=qemu-amd64-uefi\nrelease={release}\nbuild=test-{release}\nsystem_bytes={}\nkernel_bytes={}\ninitramfs_bytes={}\n", size("system.img"), size("kernel"), size("initramfs"))).unwrap();
        self.sign(&path);
        path
    }
    fn sign(&self, path: &Path) {
        let output = Command::new("/usr/bin/sha256sum")
            .args(["system.img", "kernel", "initramfs", "manifest"])
            .current_dir(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        fs::write(path.join("SHA256SUMS"), output.stdout).unwrap();
        let output = Command::new("/usr/bin/openssl")
            .args(["pkeyutl", "-sign", "-rawin", "-inkey"])
            .arg(self.path.join("private.pem"))
            .args(["-in", "SHA256SUMS", "-out", "SHA256SUMS.sig"])
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn initialize(&self) -> ImageStore {
        ImageStore::initialize(&self.root, &self.payload("baseline", 1), &self.key, A, B).unwrap()
    }
    fn entry(&self, name: &str) -> PathBuf {
        self.root.join("efi/loader/entries").join(name)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}

#[test]
fn stages_only_inactive_image_and_activation_retry_preserves_boot_counts() {
    let f = Fixture::new();
    let store = f.initialize();
    let healthy = store.snapshot().unwrap();
    let active = fs::read(f.root.join("slots/a/system.img")).unwrap();
    fs::write(f.path.join("data-sentinel"), b"preserve me").unwrap();
    let staged = store
        .stage(healthy, &f.payload("candidate", 2), &f.key)
        .unwrap();
    assert_eq!(
        staged.phase,
        Phase::Staged {
            active: Slot::A,
            candidate: Slot::B
        }
    );
    assert_eq!(fs::read(f.root.join("slots/a/system.img")).unwrap(), active);
    assert!(!f.entry("linos-2-b+2.conf").exists());
    let trial = store.activate(staged, &f.key).unwrap();
    assert!(store.activate(staged, &f.key).is_err());
    assert!(
        fs::read_to_string(f.entry("linos-2-b+2.conf"))
            .unwrap()
            .contains(&format!("root=PARTUUID={B}"))
    );
    assert_eq!(store.activate(trial, &f.key).unwrap(), trial);
    fs::rename(f.entry("linos-2-b+2.conf"), f.entry("linos-2-b+1-1.conf")).unwrap();
    assert_eq!(store.activate(trial, &f.key).unwrap(), trial);
    assert!(!f.entry("linos-2-b+2.conf").exists());
    fs::rename(f.entry("linos-2-b+1-1.conf"), f.entry("linos-2-b+0-2.conf")).unwrap();
    assert_eq!(store.activate(trial, &f.key).unwrap(), trial);
    assert!(f.entry("linos-1-a.conf").exists());
    fs::remove_file(f.entry("linos-2-b+0-2.conf")).unwrap();
    assert!(store.activate(trial, &f.key).is_err());
    assert!(!f.entry("linos-2-b+2.conf").exists());
    assert_eq!(
        fs::read(f.path.join("data-sentinel")).unwrap(),
        b"preserve me"
    );
    drop(store);
    assert_eq!(
        ImageStore::open(&f.root).unwrap().snapshot().unwrap(),
        trial
    );
}

#[test]
fn refuses_untrusted_tampered_wrong_target_and_downgrade_payloads() {
    let f = Fixture::new();
    let store = f.initialize();
    let healthy = store.snapshot().unwrap();
    let candidate = f.payload("candidate", 2);
    fs::write(candidate.join("SHA256SUMS.sig"), [0u8; 64]).unwrap();
    assert!(store.stage(healthy, &candidate, &f.key).is_err());
    f.sign(&candidate);
    let size = fs::metadata(candidate.join("system.img")).unwrap().len() as usize;
    fs::write(candidate.join("system.img"), vec![b'X'; size]).unwrap();
    assert!(store.stage(healthy, &candidate, &f.key).is_err());
    let wrong = f.payload("wrong", 2);
    let manifest = fs::read_to_string(wrong.join("manifest"))
        .unwrap()
        .replace("architecture=amd64", "architecture=arm64");
    fs::write(wrong.join("manifest"), manifest).unwrap();
    f.sign(&wrong);
    assert!(store.stage(healthy, &wrong, &f.key).is_err());
    assert!(store.stage(healthy, &f.payload("old", 1), &f.key).is_err());
    assert_eq!(store.snapshot().unwrap(), healthy);
    assert!(!f.root.join("slots/b").exists());
}

#[test]
fn refuses_payload_keys_symlinks_and_changed_candidate_before_activation() {
    let f = Fixture::new();
    let store = f.initialize();
    let healthy = store.snapshot().unwrap();
    let candidate = f.payload("candidate", 2);
    fs::copy(&f.key, candidate.join("public.pem")).unwrap();
    assert!(
        store
            .stage(healthy, &candidate, &candidate.join("public.pem"))
            .is_err()
    );
    fs::remove_file(candidate.join("system.img")).unwrap();
    symlink("/dev/zero", candidate.join("system.img")).unwrap();
    assert!(store.stage(healthy, &candidate, &f.key).is_err());
    let staged = store.stage(healthy, &f.payload("good", 2), &f.key).unwrap();
    fs::write(f.root.join("slots/b/system.img"), b"changed after staging").unwrap();
    assert!(store.activate(staged, &f.key).is_err());
    assert_eq!(store.snapshot().unwrap(), staged);
    assert!(!f.entry("linos-2-b+2.conf").exists());
}

#[test]
fn resumes_interruption_after_trial_journal_before_entry_publication() {
    let f = Fixture::new();
    let store = f.initialize();
    let staged = store
        .stage(
            store.snapshot().unwrap(),
            &f.payload("candidate", 2),
            &f.key,
        )
        .unwrap();
    drop(store);
    let journal = Journal::open(&f.root).unwrap();
    let trial = journal.apply(staged, Event::BeginTrial).unwrap();
    drop(journal);
    let store = ImageStore::open(&f.root).unwrap();
    assert_eq!(store.activate(trial, &f.key).unwrap(), trial);
    assert_eq!(store.activate(trial, &f.key).unwrap(), trial);
    assert!(f.entry("linos-2-b+2.conf").exists());
}

#[test]
fn rejects_overlapping_roots_unsafe_destination_and_nonempty_initialization() {
    let f = Fixture::new();
    let baseline = f.payload("baseline", 1);
    assert!(ImageStore::initialize(&f.root, &baseline, &f.key, A, A).is_err());
    let store = ImageStore::initialize(&f.root, &baseline, &f.key, A, B).unwrap();
    let healthy = store.snapshot().unwrap();
    symlink(f.root.join("slots/a"), f.root.join("slots/b")).unwrap();
    assert!(
        store
            .stage(healthy, &f.payload("candidate", 2), &f.key)
            .is_err()
    );
    assert_eq!(
        fs::read(f.root.join("slots/a/system.img")).unwrap(),
        b"system image 1"
    );
    drop(store);
    assert!(ImageStore::initialize(&f.root, &baseline, &f.key, A, B).is_err());
    assert!(ImageStore::open(Path::new("/dev/null")).is_err());
}

#[test]
fn refuses_unknown_boot_entries_and_changed_fallback_policy() {
    let f = Fixture::new();
    let store = f.initialize();
    let healthy = store.snapshot().unwrap();
    let candidate = f.payload("candidate", 2);
    fs::write(f.entry("linos-999-foreign.conf"), b"title unverified\n").unwrap();
    assert!(store.stage(healthy, &candidate, &f.key).is_err());
    fs::remove_file(f.entry("linos-999-foreign.conf")).unwrap();
    let staged = store.stage(healthy, &candidate, &f.key).unwrap();
    fs::write(f.entry("linos-999-foreign.conf"), b"title unverified\n").unwrap();
    assert!(store.activate(staged, &f.key).is_err());
    assert_eq!(store.snapshot().unwrap(), staged);
    fs::remove_file(f.entry("linos-999-foreign.conf")).unwrap();
    fs::write(
        f.root.join("efi/loader/loader.conf"),
        b"default linos-999-foreign.conf\n",
    )
    .unwrap();
    drop(store);
    assert!(ImageStore::open(&f.root).is_err());
}
