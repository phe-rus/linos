use std::{
    env,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use rustix::{
    fs::{self as rfs, AtFlags, Mode, OFlags},
    io::Errno,
};

#[derive(Debug, PartialEq, Eq)]
struct Device {
    hostname: String,
    id: String,
    state: String,
}

fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn random_uuid() -> io::Result<String> {
    let uuid = fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    let uuid = uuid.trim();
    if !valid_uuid(uuid) {
        return Err(io::Error::other("Unable to generate a device identifier"));
    }
    Ok(uuid.to_owned())
}

fn generate_hostname() -> io::Result<String> {
    let uuid = random_uuid()?;
    let suffix: String = uuid
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .take(10)
        .collect();
    Ok(format!("linos-{suffix}"))
}

fn filesystem_error(error: Errno) -> io::Error {
    if error == Errno::LOOP || error == Errno::NOTDIR {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "A directory or file in the target path is unsafe",
        )
    } else {
        error.into()
    }
}

fn open_directory(root: &File, components: &[&str], create: bool) -> io::Result<Option<File>> {
    let mut current = root.try_clone()?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;

    for name in components {
        let next = match rfs::openat(&current, *name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::NOENT) if !create => return Ok(None),
            Err(Errno::NOENT) => {
                match rfs::mkdirat(&current, *name, Mode::from(0o700)) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(error) => return Err(filesystem_error(error)),
                }
                rfs::openat(&current, *name, flags, Mode::empty()).map_err(filesystem_error)?
            }
            Err(error) => return Err(filesystem_error(error)),
        };
        current = File::from(next);
    }

    Ok(Some(current))
}

fn read_existing(parent: Option<&File>, name: &str) -> io::Result<Option<String>> {
    let Some(parent) = parent else {
        return Ok(None);
    };
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rfs::openat(parent, name, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(filesystem_error(error)),
    };
    let file = File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Existing identity entry is not a regular file",
        ));
    }

    let mut contents = String::new();
    file.take(129).read_to_string(&mut contents)?;
    if contents.len() > 128 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Existing identity entry is too large",
        ));
    }
    let value = contents.trim();
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Existing identity entry is empty",
        ));
    }
    Ok(Some(value.to_owned()))
}

// Work relative to a previously opened directory descriptor. The final name
// only appears after the temp file's contents are synced.
fn write_new(parent: &File, name: &str, value: &str, mode: u32) -> io::Result<()> {
    let temp = format!(".{name}.{}.tmp", random_uuid()?);
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;

    let fd =
        rfs::openat(parent, temp.as_str(), flags, Mode::from(mode)).map_err(filesystem_error)?;
    let result: io::Result<()> = (|| {
        let mut file = File::from(fd);
        file.write_all(format!("{value}\n").as_bytes())?;
        file.sync_all()?;
        rfs::linkat(parent, temp.as_str(), parent, name, AtFlags::empty())
            .map_err(filesystem_error)?;
        rfs::fsync(parent).map_err(filesystem_error)
    })();

    let cleanup = rfs::unlinkat(parent, temp.as_str(), AtFlags::empty()).map_err(filesystem_error);
    if result.is_ok() {
        cleanup?;
        rfs::fsync(parent).map_err(filesystem_error)?;
    }
    result
}

fn initialize(root: &Path) -> io::Result<Device> {
    initialize_for_root(root, false)
}

fn layout2(root: &Path) -> io::Result<bool> {
    match fs::read(root.join("usr/lib/linos/storage-layout")) {
        Ok(bytes) if bytes == b"layout=2\n" => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unknown Linos storage capability marker",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn initialize_for_root(root: &Path, allow_live_root: bool) -> io::Result<Device> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The target root cannot be a symbolic link",
        ));
    }
    let root = fs::canonicalize(root)?;
    if root == Path::new("/") && !allow_live_root {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Live system initialization is not enabled",
        ));
    }
    let root_fd = File::from(
        rfs::open(
            &root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(filesystem_error)?,
    );

    let etc = open_directory(&root_fd, &["etc"], false)?;
    let state_dir = open_directory(&root_fd, &["var", "lib", "linos"], false)?;

    // Validate existing state before creating or modifying any entries.
    let hostname = read_existing(etc.as_ref(), "hostname")?;
    let id = read_existing(state_dir.as_ref(), "device-id")?;
    let state = read_existing(state_dir.as_ref(), "provisioning-state")?;
    let managed = root.join("usr/lib/linos/update-layout").try_exists()? || layout2(&root)?;
    let saved_hostname = if managed {
        read_existing(state_dir.as_ref(), "hostname")?
    } else {
        None
    };

    if let Some(ref existing) = id
        && !valid_uuid(existing)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Existing device identifier is invalid",
        ));
    }
    if let Some(ref existing) = state
        && existing != "unclaimed"
        && existing != "provisioned"
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unsupported provisioning state",
        ));
    }

    let local_hostname = hostname.clone();
    let hostname = match saved_hostname.clone().or(hostname) {
        Some(existing) => existing,
        None => {
            let directory = open_directory(&root_fd, &["etc"], true)?
                .ok_or_else(|| io::Error::other("Unable to open etc"))?;
            let value = generate_hostname()?;
            write_new(&directory, "hostname", &value, 0o644)?;
            value
        }
    };

    if managed {
        let directory = open_directory(&root_fd, &["var", "lib", "linos"], true)?
            .ok_or_else(|| io::Error::other("Missing persistent identity directory"))?;
        if saved_hostname.is_none() {
            write_new(&directory, "hostname", &hostname, 0o600)?;
        }
        if local_hostname
            .as_ref()
            .is_some_and(|local| local != &hostname)
        {
            let etc = open_directory(&root_fd, &["etc"], false)?
                .ok_or_else(|| io::Error::other("Missing etc directory"))?;
            let temporary = format!(".hostname.{}.tmp", random_uuid()?);
            write_new(&etc, &temporary, &hostname, 0o644)?;
            let result =
                rfs::renameat(&etc, temporary.as_str(), &etc, "hostname").map_err(filesystem_error);
            let _ = rfs::unlinkat(&etc, temporary.as_str(), AtFlags::empty());
            result?;
            rfs::fsync(&etc).map_err(filesystem_error)?;
        } else if local_hostname.is_none() && saved_hostname.is_some() {
            let etc = open_directory(&root_fd, &["etc"], true)?
                .ok_or_else(|| io::Error::other("Missing etc directory"))?;
            write_new(&etc, "hostname", &hostname, 0o644)?;
        }
    }

    let id = match id {
        Some(existing) => existing,
        None => {
            let directory = open_directory(&root_fd, &["var", "lib", "linos"], true)?
                .ok_or_else(|| io::Error::other("Unable to open state directory"))?;
            let value = random_uuid()?;
            write_new(&directory, "device-id", &value, 0o600)?;
            value
        }
    };

    let state = match state {
        Some(existing) => existing,
        None => {
            let directory = open_directory(&root_fd, &["var", "lib", "linos"], true)?
                .ok_or_else(|| io::Error::other("Unable to open state directory"))?;
            write_new(&directory, "provisioning-state", "unclaimed", 0o600)?;
            "unclaimed".to_owned()
        }
    };
    Ok(Device {
        hostname,
        id,
        state,
    })
}

fn is_linos_release(contents: &str) -> bool {
    contents.lines().any(|line| line == "ID=linos")
}

fn initialize_live() -> io::Result<Device> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "First-boot initialization requires root privileges",
        ));
    }
    let release = fs::read_to_string("/usr/lib/linos/release")?;
    if !is_linos_release(&release) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Live initialization is restricted to an installed Linos system",
        ));
    }
    let is_layout2 = layout2(Path::new("/"))?;
    if is_layout2 {
        linos::recovery::boot::verify_running()?;
    }
    let device = initialize_for_root(Path::new("/"), true)?;
    if Path::new("/usr/lib/linos/update-layout").try_exists()? || is_layout2 {
        rustix::system::sethostname(device.hostname.as_bytes())?;
    }
    Ok(device)
}

// The userspace/kernel readiness contract is deliberately separate from
// "the kernel booted". Never acknowledge a device that is unclaimed or whose
// owner record does not match the generated device identifier.
fn acknowledge_ready(root: &Path, kernel_state: &Path) -> io::Result<()> {
    let release = fs::read_to_string(root.join("usr/lib/linos/release"))?;
    if !is_linos_release(&release) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Readiness acknowledgement requires Linos",
        ));
    }
    let root_fd = File::from(
        rfs::open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(filesystem_error)?,
    );
    let state_dir = open_directory(&root_fd, &["var", "lib", "linos"], false)?
        .ok_or_else(|| io::Error::other("Missing Linos identity directory"))?;
    let value = |name: &str| {
        read_existing(Some(&state_dir), name)?
            .ok_or_else(|| io::Error::other(format!("Missing Linos identity: {name}")))
    };
    let device_id = value("device-id")?;
    if !valid_uuid(&device_id) || value("provisioning-state")? != "provisioned" {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The Linos device has not completed enrollment",
        ));
    }
    let setup_dir = open_directory(&root_fd, &["etc", "linos"], false)?;
    if read_existing(setup_dir.as_ref(), "setup-complete")?.as_deref() != Some("ready") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Linos setup completion has not been recorded",
        ));
    }
    let owner = value("owner")?;
    let mut lines = owner.lines();
    let username = lines.next().and_then(|line| line.strip_prefix("username="));
    let bound_device = lines
        .next()
        .and_then(|line| line.strip_prefix("device-id="));
    if username.is_none_or(|name| name.is_empty())
        || bound_device != Some(device_id.as_str())
        || lines.next().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The Linos owner record does not match this device",
        ));
    }

    match fs::read_to_string(kernel_state)?.trim() {
        "starting" => {
            fs::write(kernel_state, b"ready\n")?;
            if fs::read_to_string(kernel_state)?.trim() != "ready" {
                return Err(io::Error::other("The kernel did not acknowledge readiness"));
            }
            Ok(())
        }
        "ready" => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unexpected Linos kernel readiness state",
        )),
    }
}

fn acknowledge_ready_live() -> io::Result<()> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Kernel readiness acknowledgement requires root",
        ));
    }
    acknowledge_ready(Path::new("/"), Path::new("/sys/kernel/linos/boot_state"))
}

fn main() -> io::Result<()> {
    let mut arguments = env::args().skip(1);
    let Some(option) = arguments.next() else {
        eprintln!("Usage: linos-init --root <directory> | --first-boot | --acknowledge-ready");
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing initialization mode",
        ));
    };

    if option == "--acknowledge-ready" {
        if arguments.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unexpected readiness arguments",
            ));
        }
        return acknowledge_ready_live();
    }

    let device = match option.as_str() {
        "--first-boot" => {
            if arguments.next().is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unexpected first-boot arguments",
                ));
            }
            initialize_live()?
        }
        "--root" => {
            let root = arguments.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Missing root directory")
            })?;
            if arguments.next().is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unexpected arguments",
                ));
            }
            initialize(&PathBuf::from(root))?
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected --root, --first-boot or --acknowledge-ready",
            ));
        }
    };

    println!("  [ OK ] Linos device identity ready");
    println!("  [ OK ] Hostname: {}", device.hostname);
    println!("Provisioning state: {}", device.state);
    eprintln!("Device identifier: {}", device.id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        let name = random_uuid().expect("random UUID");
        let root = env::temp_dir().join(format!("linos-init-test-{name}"));
        fs::create_dir(&root).expect("create test directory");
        root
    }

    fn cleanup(root: PathBuf) {
        fs::remove_dir_all(root).expect("clean test directory");
    }

    #[test]
    fn detects_linos_release_identity() {
        assert!(is_linos_release(
            "NAME=\"linos\"\nID=linos\nVERSION_ID=\"13\"\n"
        ));
        assert!(!is_linos_release("ID=debian\n"));
        assert!(!is_linos_release("ID=linos-extra\n"));
    }

    #[test]
    fn managed_hostname_survives_missing_and_stale_slot_files() {
        let root = test_root();
        fs::create_dir_all(root.join("usr/lib/linos")).unwrap();
        fs::write(root.join("usr/lib/linos/update-layout"), "managed image\n").unwrap();
        let first = initialize(&root).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("var/lib/linos/hostname"))
                .unwrap()
                .trim(),
            first.hostname
        );
        fs::remove_file(root.join("etc/hostname")).unwrap();
        assert_eq!(initialize(&root).unwrap(), first);
        fs::write(root.join("etc/hostname"), "stale-slot-name\n").unwrap();
        assert_eq!(initialize(&root).unwrap(), first);
        assert_eq!(
            fs::read_to_string(root.join("etc/hostname"))
                .unwrap()
                .trim(),
            first.hostname
        );
        cleanup(root);
    }

    #[test]
    fn managed_hostname_rejects_symlinked_persistent_state() {
        let root = test_root();
        fs::create_dir_all(root.join("usr/lib/linos")).unwrap();
        fs::write(root.join("usr/lib/linos/update-layout"), "managed image\n").unwrap();
        initialize(&root).unwrap();
        fs::remove_file(root.join("var/lib/linos/hostname")).unwrap();
        std::os::unix::fs::symlink(
            root.join("etc/hostname"),
            root.join("var/lib/linos/hostname"),
        )
        .unwrap();
        assert!(initialize(&root).is_err());
        cleanup(root);
    }

    #[test]
    fn creates_identity_and_hostname() {
        let root = test_root();
        let device = initialize(&root).expect("initialize");

        assert!(device.hostname.starts_with("linos-"));
        assert!(valid_uuid(&device.id));
        assert_eq!(device.state, "unclaimed");
        assert_eq!(
            fs::read_to_string(root.join("etc/hostname"))
                .expect("hostname")
                .trim(),
            device.hostname
        );
        assert_eq!(
            fs::read_to_string(root.join("var/lib/linos/device-id"))
                .expect("device id")
                .trim(),
            device.id
        );
        assert_eq!(
            fs::read_to_string(root.join("var/lib/linos/provisioning-state"))
                .expect("provisioning state")
                .trim(),
            "unclaimed"
        );
        cleanup(root);
    }

    #[test]
    fn preserves_identity_on_repeat() {
        let root = test_root();
        let first = initialize(&root).expect("first initialization");
        let second = initialize(&root).expect("second initialization");
        assert_eq!(first, second);
        cleanup(root);
    }

    #[test]
    fn preserves_existing_hostname() {
        let root = test_root();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/hostname"), "custom-host\n").unwrap();

        let device = initialize(&root).expect("initialize");
        assert_eq!(device.hostname, "custom-host");
        assert!(valid_uuid(&device.id));
        cleanup(root);
    }

    #[test]
    fn rejects_live_root() {
        let error = initialize(Path::new("/")).expect_err("live root must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn rejects_empty_existing_hostname() {
        let root = test_root();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/hostname"), "\n").unwrap();
        let error = initialize(&root).expect_err("empty hostname must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!root.join("var/lib/linos/device-id").exists());
        cleanup(root);
    }

    #[test]
    fn rejects_invalid_existing_device_id() {
        let root = test_root();
        fs::create_dir_all(root.join("var/lib/linos")).unwrap();
        fs::write(root.join("var/lib/linos/device-id"), "not-a-uuid\n").unwrap();
        let error = initialize(&root).expect_err("invalid identity must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!root.join("etc/hostname").exists());
        cleanup(root);
    }

    #[test]
    fn creates_private_state_files() {
        use std::os::unix::fs::PermissionsExt;

        let root = test_root();
        initialize(&root).expect("initialize");
        for name in ["device-id", "provisioning-state"] {
            let metadata = fs::metadata(root.join("var/lib/linos").join(name)).unwrap();
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }
        cleanup(root);
    }

    #[test]
    fn rejects_symlinked_etc_directory() {
        use std::os::unix::fs::symlink;

        let root = test_root();
        let external = test_root();
        symlink(&external, root.join("etc")).unwrap();
        let error = initialize(&root).expect_err("must not follow etc symlink");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!external.join("hostname").exists());
        cleanup(root);
        cleanup(external);
    }

    #[test]
    fn readiness_requires_completed_enrollment_and_matching_owner() {
        let root = test_root();
        let device = initialize(&root).unwrap();
        let state = root.join("kernel-state");
        fs::write(&state, "starting\n").unwrap();
        fs::create_dir_all(root.join("usr/lib/linos")).unwrap();
        fs::write(root.join("usr/lib/linos/release"), "ID=linos\n").unwrap();
        assert!(acknowledge_ready(&root, &state).is_err());
        let data = root.join("var/lib/linos");
        fs::write(data.join("provisioning-state"), "provisioned\n").unwrap();
        fs::create_dir_all(root.join("etc/linos")).unwrap();
        fs::write(root.join("etc/linos/setup-complete"), "ready\n").unwrap();
        fs::write(
            data.join("owner"),
            format!("username=linosadmin\ndevice-id={}\n", device.id),
        )
        .unwrap();
        acknowledge_ready(&root, &state).unwrap();
        assert_eq!(fs::read_to_string(&state).unwrap(), "ready\n");
        // Repeating the acknowledgement is safe.
        acknowledge_ready(&root, &state).unwrap();
        fs::write(&state, "starting\n").unwrap();
        fs::write(data.join("owner"), "username=wrong\ndevice-id=invalid\n").unwrap();
        assert!(acknowledge_ready(&root, &state).is_err());
        assert_eq!(fs::read_to_string(&state).unwrap(), "starting\n");
        cleanup(root);
    }

    #[test]
    fn readiness_rejects_bad_kernel_state() {
        let root = test_root();
        let device = initialize(&root).unwrap();
        let state = root.join("kernel-state");
        fs::create_dir_all(root.join("usr/lib/linos")).unwrap();
        fs::write(root.join("usr/lib/linos/release"), "ID=linos\n").unwrap();
        let data = root.join("var/lib/linos");
        fs::write(data.join("provisioning-state"), "provisioned\n").unwrap();
        fs::create_dir_all(root.join("etc/linos")).unwrap();
        fs::write(root.join("etc/linos/setup-complete"), "ready\n").unwrap();
        fs::write(
            data.join("owner"),
            format!("username=admin\ndevice-id={}\n", device.id),
        )
        .unwrap();
        fs::write(&state, "invalid\n").unwrap();
        assert!(acknowledge_ready(&root, &state).is_err());
        cleanup(root);
    }

    #[test]
    fn validates_uuid_shape() {
        assert!(valid_uuid("4b879a42-536a-4d36-a815-4dd87c21fd01"));
        assert!(!valid_uuid(""));
        assert!(!valid_uuid("not-a-uuid"));
        assert!(!valid_uuid("4b879a42_536a-4d36-a815-4dd87c21fd01"));
    }
    #[test]
    fn recovers_from_interrupted_initialization() {
        let root = test_root();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/hostname"), "kept-name\n").unwrap();
        let device = initialize(&root).expect("resume interrupted initialization");
        assert_eq!(device.hostname, "kept-name");
        assert!(valid_uuid(&device.id));
        assert_eq!(device.state, "unclaimed");
        cleanup(root);
    }

    #[test]
    fn preserves_provisioned_state() {
        let root = test_root();
        fs::create_dir_all(root.join("var/lib/linos")).unwrap();
        fs::write(
            root.join("var/lib/linos/provisioning-state"),
            "provisioned\n",
        )
        .unwrap();
        let device = initialize(&root).expect("provisioned identity is valid");
        assert_eq!(device.state, "provisioned");
        cleanup(root);
    }

    #[test]
    fn rejects_invalid_provisioning_state() {
        let root = test_root();
        fs::create_dir_all(root.join("var/lib/linos")).unwrap();
        fs::write(root.join("var/lib/linos/provisioning-state"), "ready\n").unwrap();
        let error = initialize(&root).expect_err("unsupported state");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!root.join("etc/hostname").exists());
        cleanup(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_state_directory() {
        use std::os::unix::fs::symlink;
        let root = test_root();
        let external = test_root();
        fs::create_dir_all(root.join("var/lib")).unwrap();
        symlink(&external, root.join("var/lib/linos")).unwrap();
        let error = initialize(&root).expect_err("must reject symlink");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!external.join("device-id").exists());
        cleanup(root);
        cleanup(external);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_identity_file() {
        use std::os::unix::fs::symlink;
        let root = test_root();
        fs::create_dir_all(root.join("var/lib/linos")).unwrap();
        symlink("/etc/hostname", root.join("var/lib/linos/device-id")).unwrap();
        let error = initialize(&root).expect_err("must reject symlink");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        cleanup(root);
    }
}
