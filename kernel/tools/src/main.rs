// SPDX-License-Identifier: MIT
//! Linos-owned kernel build preparation and policy validation.
//! This tool never installs a kernel or modifies the verified upstream checkout.
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn overlay(source: &Path, overlays: &Path) -> Result<()> {
    for relative in ["Kconfig", "kernel/Makefile"] {
        if !source.join(relative).is_file() {
            return Err(format!("Not an upstream Linux source tree: {relative}"));
        }
    }
    for filename in ["linos_boot_state.c", "Kconfig.linos"] {
        let src = overlays.join(filename);
        let dest = source.join("kernel").join(filename);
        let contents = fs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        if dest.exists() {
            let existing = fs::read(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
            if existing != contents {
                return Err(format!(
                    "Refusing conflicting Linos overlay: {}",
                    dest.display()
                ));
            }
        } else {
            write(&dest, &contents)?;
        }
    }
    for (relative, line) in [
        ("Kconfig", r#"source "kernel/Kconfig.linos""#),
        (
            "kernel/Makefile",
            "obj-$(CONFIG_LINOS_BOOT_STATE) += linos_boot_state.o",
        ),
    ] {
        let path = source.join(relative);
        let old = read(&path)?;
        if !old.lines().any(|existing| existing == line) {
            let mut updated = old.trim_end_matches('\n').to_owned();
            updated.push('\n');
            updated.push_str(line);
            updated.push('\n');
            write(&path, updated.as_bytes())?;
        }
    }
    Ok(())
}

fn policy_file(path: &Path) -> Result<BTreeMap<String, String>> {
    let mut entries = BTreeMap::new();
    for (index, line) in read(path)?.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, value) = line.split_once('=').ok_or_else(|| {
            format!(
                "{}:{} invalid Kconfig assignment",
                path.display(),
                index + 1
            )
        })?;
        let valid = name.starts_with("CONFIG_")
            && name[7..]
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && !name[7..].is_empty()
            && matches!(value, "y" | "m" | "n");
        if !valid {
            return Err(format!(
                "{}:{} invalid policy: {line}",
                path.display(),
                index + 1
            ));
        }
        if entries.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err(format!(
                "{}:{} duplicate option: {name}",
                path.display(),
                index + 1
            ));
        }
    }
    Ok(entries)
}

fn require(map: &BTreeMap<String, String>, name: &str, expected: &str) -> Result<()> {
    if map.get(name).map(String::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(format!("{name} must be {expected} in Linos kernel policy"))
    }
}

fn valid_version(source: &str, key: &str) -> Result<()> {
    let prefix = format!("{key}=");
    let value = source
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .ok_or_else(|| format!("Missing {key}"))?;
    let sections: Vec<_> = value.split('.').collect();
    if sections.len() != 3
        || sections
            .iter()
            .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(format!("Invalid {key}: {value}"));
    }
    Ok(())
}

fn policy(root: &Path) -> Result<()> {
    let config = root.join("kernel/config");
    let common = policy_file(&config.join("common.config"))?;
    for option in [
        "MODULES",
        "BLK_DEV_INITRD",
        "SECURITY_LANDLOCK",
        "SECURITY_YAMA",
        "HARDENED_USERCOPY",
        "SECCOMP_FILTER",
        "LINOS_BOOT_STATE",
        "PSI",
        "MEMCG",
        "CGROUP_PIDS",
        "HIGH_RES_TIMERS",
        "PREEMPT_DYNAMIC",
        "PSTORE",
        "DM_VERITY",
        "OVERLAY_FS",
        "DEBUG_INFO_NONE",
    ] {
        require(&common, &format!("CONFIG_{option}"), "y")?;
    }
    for option in ["LOCALVERSION_AUTO", "DEBUG_INFO_BTF"] {
        require(&common, &format!("CONFIG_{option}"), "n")?;
    }
    for arch in ["amd64", "arm64"] {
        let selected = policy_file(&config.join(format!("{arch}.config")))?;
        for option in [
            "DRM_VIRTIO_GPU",
            "FRAMEBUFFER_CONSOLE",
            "INPUT_EVDEV",
            "VT_CONSOLE",
        ] {
            require(&selected, &format!("CONFIG_{option}"), "y")?;
        }
        for key in selected.keys() {
            if common.contains_key(key) {
                return Err(format!("{arch}: duplicate common policy option: {key}"));
            }
        }
    }
    let versions = read(&root.join("kernel/source.conf"))?;
    valid_version(&versions, "LINOS_KERNEL_VERSION")?;
    valid_version(&versions, "LINOS_KERNEL_LTS")?;
    let code = read(&root.join("kernel/overlays/linos_boot_state.c"))?;
    for phrase in [
        "CAP_SYS_BOOT",
        r#"sysfs_streq(buf, "ready")"#,
        "atomic_cmpxchg",
    ] {
        if !code.contains(phrase) {
            return Err(format!("Missing boot-state contract: {phrase}"));
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let result = match args.get(1).and_then(|arg| arg.to_str()) {
        Some("apply") if args.len() == 4 => {
            overlay(&PathBuf::from(&args[2]), &PathBuf::from(&args[3]))
        }
        Some("check") if args.len() == 3 => policy(&PathBuf::from(&args[2])),
        _ => Err("Usage: linos-kernel-tool apply <disposable-linux-source> <overlay-dir> | check <linos-root>".to_owned()),
    };
    if let Err(error) = result {
        eprintln!("linos-kernel-tool: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn sandbox() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            env::temp_dir().join(format!("linos-kernel-tool-{}-{unique}", std::process::id()));
        fs::create_dir_all(path.join("kernel")).unwrap();
        path
    }
    fn fixtures(path: &Path) {
        write(&path.join("Kconfig"), b"menu \"Kernel\"\nendmenu\n").unwrap();
        write(&path.join("kernel/Makefile"), b"obj-y += fork.o\n").unwrap();
    }
    #[test]
    fn overlay_idempotence_and_conflict() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = sandbox();
        fixtures(&path);
        let overlays = root.join("kernel/overlays");
        overlay(&path, &overlays).unwrap();
        overlay(&path, &overlays).unwrap();
        assert_eq!(
            read(&path.join("Kconfig"))
                .unwrap()
                .matches(r#"source "kernel/Kconfig.linos""#)
                .count(),
            1
        );
        assert_eq!(
            read(&path.join("kernel/Makefile"))
                .unwrap()
                .matches("linos_boot_state.o")
                .count(),
            1
        );
        write(&path.join("kernel/linos_boot_state.c"), b"conflict").unwrap();
        assert!(
            overlay(&path, &overlays)
                .unwrap_err()
                .contains("conflicting")
        );
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn source_layout_is_required() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = sandbox();
        assert!(overlay(&path, &root.join("kernel/overlays")).is_err());
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn repository_policy_passes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        policy(&root).unwrap();
    }
    #[test]
    fn duplicate_policy_rejected() {
        let path = sandbox();
        let cfg = path.join("sample.config");
        write(&cfg, b"CONFIG_ABC=y\nCONFIG_ABC=n\n").unwrap();
        assert!(policy_file(&cfg).unwrap_err().contains("duplicate"));
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn invalid_version_rejected() {
        assert!(valid_version("LINOS_KERNEL_VERSION=1.0.0", "LINOS_KERNEL_VERSION").is_ok());
        assert!(valid_version("LINOS_KERNEL_VERSION=1.0.rc", "LINOS_KERNEL_VERSION").is_err());
    }
}
