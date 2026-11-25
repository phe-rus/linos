//! Test only ephemeral keys. No private material is persisted or logged.
use rustix::fs::{MemfdFlags, memfd_create};
use std::{fs::File, io::Write, os::fd::AsRawFd, process::Command};

fn output(args: &[&str]) -> Vec<u8> {
    let output = Command::new("/usr/bin/openssl")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Native test signature operation failed"
    );
    output.stdout
}
fn snapshot(bytes: &[u8]) -> File {
    let mut file =
        File::from(memfd_create("linos-signature-fixture", MemfdFlags::CLOEXEC).unwrap());
    file.write_all(bytes).unwrap();
    file
}
fn path(file: &File) -> String {
    format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd())
}

pub(super) struct Signer {
    private: File,
    pub public: Vec<u8>,
}
impl Signer {
    pub fn new() -> Self {
        let private = snapshot(&output(&["genpkey", "-algorithm", "ED25519"]));
        let public = output(&["pkey", "-pubout", "-in", &path(&private)]);
        Self { private, public }
    }
    pub fn sign(&self, domain: &[u8], bytes: &[u8]) -> Vec<u8> {
        let input = snapshot(&[domain, bytes].concat());
        output(&[
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            &path(&self.private),
            "-in",
            &path(&input),
        ])
    }
}
