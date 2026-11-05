use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::Command,
};

const STATE_DIR: &str = "/var/lib/linos";
const RELEASE_FILE: &str = "/usr/lib/linos/release";

fn validate_username(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_' || *byte == b'-'
        })
        && name != "root"
}

fn validate_device_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn parse_owner_record<'a>(contents: &'a str, device_id: &str) -> io::Result<&'a str> {
    let mut lines = contents.lines();
    let username = lines.next().and_then(|line| line.strip_prefix("username="));
    let recorded_id = lines.next().and_then(|line| line.strip_prefix("device-id="));
    match (username, recorded_id, lines.next()) {
        (Some(name), Some(id), None) if validate_username(name) && id == device_id => Ok(name),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Owner record is invalid",
        )),
    }
}

fn verify_linos_host() -> io::Result<()> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Root privileges required",
        ));
    }
    let release = fs::read_to_string(RELEASE_FILE)?;
    if !release.lines().any(|line| line == "ID=linos") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Not an installed Linos system",
        ));
    }
    let meta = fs::symlink_metadata(STATE_DIR)?;
    if !meta.file_type().is_dir() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe Linos state directory",
        ));
    }
    Ok(())
}

fn read_state(name: &str) -> io::Result<String> {
    let path = Path::new(STATE_DIR).join(name);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.file_type().is_file() || meta.uid() != 0 || meta.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe state file",
        ));
    }
    let value = fs::read_to_string(&path)?;
    if value.len() > 256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Oversized state file",
        ));
    }
    Ok(value.trim().to_owned())
}

fn create_state(name: &str, value: &str) -> io::Result<()> {
    let path = Path::new(STATE_DIR).join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    writeln!(file, "{value}")?;
    file.sync_all()?;
    fs::File::open(STATE_DIR)?.sync_all()
}

fn commit_provisioned_state() -> io::Result<()> {
    let path = Path::new(STATE_DIR).join("provisioning-state");
    let state = read_state("provisioning-state")?;
    if state == "provisioned" {
        return Ok(());
    }
    if state != "unclaimed" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unexpected provisioning state",
        ));
    }
    let temporary =
        Path::new(STATE_DIR).join(format!(".provisioning-state.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(b"provisioned\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    fs::File::open(STATE_DIR)?.sync_all()
}

fn run(command: &str, args: &[&str]) -> io::Result<()> {
    let status = Command::new(command).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{command} failed: {status}")))
    }
}

fn account_exists(username: &str) -> io::Result<bool> {
    let result = Command::new("/usr/bin/id")
        .arg("-u")
        .arg(username)
        .output()?;
    Ok(result.status.success())
}

fn prompt_username() -> io::Result<String> {
    print!("Administrator username: ");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let username = value.trim().to_owned();
    if !validate_username(&username) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid administrator username",
        ));
    }
    Ok(username)
}

fn enroll() -> io::Result<()> {
    verify_linos_host()?;
    let device_id = read_state("device-id")?;
    if !validate_device_id(&device_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Device identity missing",
        ));
    }

    let owner_path = Path::new(STATE_DIR).join("owner");
    if owner_path.exists() {
        let owner = read_state("owner")?;
        let _username = parse_owner_record(&owner, &device_id)?;
        commit_provisioned_state()?;
        let pending = Path::new(STATE_DIR).join("pending-owner");
        if pending.exists() {
            fs::remove_file(pending)?;
        }
        println!("This device is already enrolled.");
        return Ok(());
    }

    let state = read_state("provisioning-state")?;
    if state != "unclaimed" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Device is not unclaimed",
        ));
    }

    println!("Linos 13 | Local administrator enrollment");
    println!("Use this system's trusted physical or virtual console.");
    println!("No default administrator password will be created.");
    let username = prompt_username()?;
    let pending = Path::new(STATE_DIR).join("pending-owner");

    if pending.exists() {
        if read_state("pending-owner")? != username {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "A different enrollment is pending",
            ));
        }
        if !account_exists(&username)? {
            run(
                "/usr/sbin/useradd",
                &["-m", "-U", "-s", "/bin/bash", "-G", "sudo", &username],
            )?;
        }
    } else {
        if account_exists(&username)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Account already exists",
            ));
        }
        create_state("pending-owner", &username)?;
        run(
            "/usr/sbin/useradd",
            &["-m", "-U", "-s", "/bin/bash", "-G", "sudo", &username],
        )?;
    }

    println!("Choose a strong password for {username}.");
    run("/usr/bin/passwd", &[&username])?;

    create_state(
        "owner",
        &format!("username={username}\ndevice-id={device_id}"),
    )?;
    commit_provisioned_state()?;
    fs::remove_file(pending)?;
    println!("Linos administrator enrollment complete. Please reboot.");
    Ok(())
}

fn main() -> io::Result<()> {
    if std::env::args().len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Usage: linos-enroll",
        ));
    }
    enroll()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_administrator_usernames() {
        for value in ["niina", "admin1", "la_niina", "operator-1"] {
            assert!(validate_username(value));
        }
        for value in ["", "root", "Root", "1admin", "a b", "a/b"] {
            assert!(!validate_username(value));
        }
        assert!(!validate_username(&"a".repeat(33)));
    }

    #[test]
    fn rejects_invalid_device_identifiers() {
        assert!(validate_device_id("4b879a42-536a-4d36-a815-4dd87c21fd01"));
        assert!(!validate_device_id("invalid-device-identifier"));
        assert!(!validate_device_id("4b879a42_536a-4d36-a815-4dd87c21fd01"));
    }

    #[test]
    fn validates_exact_owner_record_and_device_binding() {
        let id = "4b879a42-536a-4d36-a815-4dd87c21fd01";
        let record = format!("username=niina\ndevice-id={id}");
        assert_eq!(parse_owner_record(&record, id).unwrap(), "niina");
        for invalid in [
            format!("username=root\ndevice-id={id}"),
            format!("username=niina\ndevice-id=wrong-{id}"),
            format!("username=niina\ndevice-id={id}\nextra=true"),
            format!("prefix=username=niina\ndevice-id={id}"),
            format!("username=niina\nwrong=device-id={id}"),
        ] {
            assert!(parse_owner_record(&invalid, id).is_err());
        }
    }
}
