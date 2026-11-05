mod layout;
mod planner;

use std::{
    fs,
    io::{self, Write},
    path::Path,
};

const EDITION: &str = "server";

fn supported_edition(value: &str) -> bool {
    matches!(value, "server" | "desktop")
}

fn visible_block_device(name: &str) -> bool {
    (name.starts_with("sd")
        || name.starts_with("vd")
        || name.starts_with("nvme")
        || name.starts_with("mmcblk"))
        && !name.contains('/')
        && name.bytes().all(|c| c.is_ascii_alphanumeric())
}

fn disks(sys_block: &Path) -> io::Result<Vec<String>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(sys_block)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if visible_block_device(&name) {
            let size_path = entry.path().join("size");
            let sectors = fs::read_to_string(size_path)?
                .trim()
                .parse::<u64>()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let gib = (sectors as f64 * 512.0) / (1024.0 * 1024.0 * 1024.0);
            result.push(format!("{name}: {gib:.1} GiB"));
        }
    }
    result.sort();
    Ok(result)
}

fn show_status() {
    debug_assert!(supported_edition(EDITION));
    println!("Linos 13 Recovery");
    println!("Edition: {EDITION} (fixed by bootable image)");
    println!("Interface: CLI");
    println!("Installation: unavailable in this development build");
    println!("Disk operations: read-only");
}

fn show_disks(sys_block: &Path) -> io::Result<()> {
    println!("Detected storage (inspection only):");
    let found = disks(sys_block)?;
    if found.is_empty() {
        println!("  No supported block devices found.");
    } else {
        for item in found {
            println!("  {item}");
        }
    }
    println!("Nothing has been mounted, partitioned, or formatted.");
    Ok(())
}

fn interactive() -> io::Result<()> {
    loop {
        println!();
        println!("  Linos 13 Recovery");
        println!("  1. System information");
        println!("  2. Inspect disks (read-only)");
        println!("  3. Install Linos (not available yet)");
        println!("  4. Repair or restore (not available yet)");
        println!("  5. Exit");
        print!("Select an option: ");
        io::stdout().flush()?;
        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            return Ok(());
        }
        match input.trim() {
            "1" => show_status(),
            "2" => show_disks(Path::new("/sys/block"))?,
            "3" | "4" => println!("This operation is not implemented. No disks were changed."),
            "5" => return Ok(()),
            _ => println!("Choose a number from 1 to 5."),
        }
    }
}

const PROFILE_FILE: &str = "/usr/lib/linos/hardware-profile";

fn profile_field<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(found, value)| (found == key).then_some(value))
}

fn validate_profile(body: &str) -> bool {
    profile_field(body, "schema") == Some("1")
        && profile_field(body, "edition") == Some("server")
        && profile_field(body, "architecture") == Some("amd64")
        && profile_field(body, "boot") == Some("uefi")
        && profile_field(body, "recovery") == Some("cli")
        && profile_field(body, "hardware").is_some_and(|value| !value.is_empty())
}

fn show_profile() -> io::Result<()> {
    let body = fs::read_to_string(PROFILE_FILE)?;
    if !validate_profile(&body) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Recovery hardware profile is missing or incompatible",
        ));
    }
    println!("Linos recovery hardware profile:");
    for line in body.lines() {
        println!("  {line}");
    }
    println!("Validation indicates the builder target, not proof of physical hardware support.");
    Ok(())
}

fn run(args: &[String]) -> io::Result<()> {
    match args {
        [] => interactive(),
        [flag] if flag == "--help" || flag == "-h" => {
            println!("Usage: linos-recovery [status|disks|profile|plan /dev/DEVICE|--help]");
            println!("Linos Server recovery console. All commands are read-only.");
            Ok(())
        }
        [command] if command == "status" => {
            show_status();
            Ok(())
        }
        [command] if command == "disks" => show_disks(Path::new("/sys/block")),
        [command] if command == "profile" => show_profile(),
        [command, device] if command == "plan" => planner::inspect(device, Path::new("/sys/block")),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unknown recovery command. Use --help.",
        )),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(&args) {
        eprintln!("linos-recovery: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editions_are_explicit() {
        assert!(supported_edition("server"));
        assert!(supported_edition("desktop"));
        assert!(!supported_edition("mobile"));
        assert!(!supported_edition("Server"));
    }

    #[test]
    fn device_names_are_strict() {
        assert!(visible_block_device("vda"));
        assert!(visible_block_device("sda"));
        assert!(visible_block_device("nvme0n1"));
        assert!(visible_block_device("mmcblk0"));
        assert!(!visible_block_device("loop0"));
        assert!(!visible_block_device("../vda"));
        assert!(!visible_block_device("vda/partition"));
    }

    #[test]
    fn hardware_profiles_are_checked() {
        let valid = "schema=1\nedition=server\narchitecture=amd64\nboot=uefi\nrecovery=cli\nhardware=generic-amd64-uefi\n";
        assert!(validate_profile(valid));
        assert!(!validate_profile(
            &valid.replace("edition=server", "edition=desktop")
        ));
        assert!(!validate_profile(
            &valid.replace("boot=uefi", "boot=direct-kernel")
        ));
    }

    #[test]
    fn installation_targets_require_explicit_device_paths() {
        assert!(planner::inspect("/dev/../vda", Path::new("/sys/block")).is_err());
        assert!(planner::inspect("vda", Path::new("/sys/block")).is_err());
        assert!(planner::inspect("/dev/loop0", Path::new("/sys/block")).is_err());
    }

    #[test]
    fn unknown_commands_are_rejected() {
        assert!(run(&["install".into()]).is_err());
        assert!(run(&["format".into()]).is_err());
    }
}
