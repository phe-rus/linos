//! Validated regional configuration for a newly enrolled Linos Server.
//! Avoids arbitrary paths and shell interpolation in privileged operations.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::symlink,
    path::Path,
    process::Command,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    French,
    Chinese,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::French => "Français",
            Self::Chinese => "中文",
        }
    }

    fn locale(self) -> &'static str {
        match self {
            Self::English => "C.UTF-8",
            Self::French => "fr_FR.UTF-8",
            Self::Chinese => "zh_CN.UTF-8",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Offline,
    Dhcp,
}

impl Network {
    pub fn label(self) -> &'static str {
        match self {
            Self::Offline => "Configure later",
            Self::Dhcp => "Automatic DHCP",
        }
    }
}

pub struct Settings {
    pub language: Language,
    pub keyboard: String,
    pub timezone: String,
    pub hostname: String,
    pub network: Network,
}

pub fn hostname_from_username(username: &str) -> String {
    let base: String = username
        .bytes()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        .map(char::from)
        .take(58)
        .collect();
    let base = base.trim_matches('-');
    if base.is_empty() {
        "linos-server".to_owned()
    } else {
        format!("{base}-server")
    }
}

fn hosts_with_hostname(previous: &str, hostname: &str) -> String {
    let mut found = false;
    let mut lines: Vec<String> = previous
        .lines()
        .map(|line| {
            let (mapping, comment) = line.split_once('#').unwrap_or((line, ""));
            let fields: Vec<_> = mapping.split_whitespace().collect();
            if fields.first() != Some(&"127.0.1.1") {
                return line.to_owned();
            }
            found = true;
            let mut updated = format!("127.0.1.1\t{hostname}");
            for alias in fields.iter().skip(2).filter(|alias| **alias != hostname) {
                updated.push(' ');
                updated.push_str(alias);
            }
            if !comment.is_empty() {
                updated.push_str(" #");
                updated.push_str(comment);
            }
            updated
        })
        .collect();
    if !found {
        lines.push(format!("127.0.1.1\t{hostname}"));
    }
    format!("{}\n", lines.join("\n"))
}

fn write_configuration(path: &str, contents: &str) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    let parent = Path::new(path)
        .parent()
        .ok_or_else(|| io::Error::other("Configuration path has no parent"))?;
    fs::File::open(parent)?.sync_all()
}

impl Settings {
    pub fn defaults(hostname: &str) -> Self {
        Self {
            language: Language::English,
            keyboard: "us".into(),
            timezone: "Etc/UTC".into(),
            hostname: hostname.to_owned(),
            network: Network::Offline,
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if !valid_hostname(&self.hostname)
            || !["us", "gb", "fr", "de"].contains(&self.keyboard.as_str())
            || ![
                "Etc/UTC",
                "Africa/Kampala",
                "Europe/London",
                "Europe/Paris",
                "America/New_York",
                "Asia/Shanghai",
                "Asia/Kolkata",
            ]
            .contains(&self.timezone.as_str())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid first-boot settings",
            ));
        }
        Ok(())
    }

    pub fn apply(&self) -> io::Result<()> {
        self.validate()?;

        // The selected locale must be provisioned in the image, not merely
        // advertised in the interface.
        let locale = self.language.locale();
        if locale != "C.UTF-8" {
            let entry = format!("{locale} UTF-8");
            let previous = fs::read_to_string("/etc/locale.gen")?;
            let updated = previous
                .lines()
                .map(|line| {
                    if line.trim_start().trim_start_matches('#').trim() == entry {
                        entry.as_str()
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            write_configuration("/etc/locale.gen", &format!("{updated}\n"))?;
            let status = Command::new("/usr/sbin/locale-gen").status()?;
            if !status.success() {
                return Err(io::Error::other("Locale generation failed"));
            }
        }
        write_configuration("/etc/default/locale", &format!("LANG={locale}\n"))?;
        write_configuration(
            "/etc/default/keyboard",
            &format!("XKBLAYOUT=\"{}\"\nXKBMODEL=\"pc105\"\n", self.keyboard),
        )?;
        let zone = Path::new("/usr/share/zoneinfo").join(&self.timezone);
        if !zone.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Selected timezone is not installed",
            ));
        }
        let localtime = Path::new("/etc/localtime");
        if localtime.exists() || localtime.is_symlink() {
            fs::remove_file(localtime)?;
        }
        symlink(zone, localtime)?;
        write_configuration("/etc/timezone", &format!("{}\n", self.timezone))?;
        write_configuration("/etc/hostname", &format!("{}\n", self.hostname))?;
        let hosts = match fs::read_to_string("/etc/hosts") {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        write_configuration("/etc/hosts", &hosts_with_hostname(&hosts, &self.hostname))?;
        let hostname_status = Command::new("/usr/bin/hostnamectl")
            .args(["set-hostname", &self.hostname])
            .status()?;
        if !hostname_status.success() {
            return Err(io::Error::other("Unable to activate server hostname"));
        }

        if self.network == Network::Dhcp {
            let config = Path::new("/etc/systemd/network/20-linos-dhcp.network");
            fs::create_dir_all(
                config
                    .parent()
                    .ok_or_else(|| io::Error::other("Invalid network path"))?,
            )?;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(config)?;
            file.write_all(b"[Match]\nName=en* eth*\n\n[Network]\nDHCP=yes\n")?;
            file.sync_all()?;
            fs::File::open(config.parent().unwrap())?.sync_all()?;
            let enabled = Command::new("/usr/bin/systemctl")
                .args(["enable", "systemd-networkd.service"])
                .status()?;
            if !enabled.success() {
                return Err(io::Error::other("Could not enable networkd"));
            }
        }
        Ok(())
    }
}

pub fn valid_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 63
        && !host.starts_with('-')
        && !host.ends_with('-')
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames_are_strictly_validated() {
        for name in ["linos", "linos-server-1", "host7"] {
            assert!(valid_hostname(name));
        }
        for name in [
            "",
            "Bad",
            "-linos",
            "linos-",
            "has spaces",
            "a/b",
            "a".repeat(64).as_str(),
        ] {
            assert!(!valid_hostname(name));
        }
    }

    #[test]
    fn hostname_defaults_to_username() {
        assert_eq!(hostname_from_username("linos"), "linos-server");
        assert_eq!(hostname_from_username("my_user"), "myuser-server");
        assert!(valid_hostname(&hostname_from_username("a")));
    }

    #[test]
    fn supported_settings_validate() {
        let valid = Settings::defaults("linos");
        valid.validate().unwrap();
        let invalid = Settings {
            timezone: "../etc/passwd".into(),
            ..valid
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn hostname_resolves_locally_without_losing_other_hosts() {
        let previous = "127.0.0.1 localhost\n::1 localhost ip6-localhost\n192.0.2.7 backup\n";
        let updated = hosts_with_hostname(previous, "linos-server");
        assert!(updated.starts_with(previous));
        assert!(updated.ends_with("127.0.1.1\tlinos-server\n"));
        assert_eq!(hosts_with_hostname(&updated, "linos-server"), updated);
    }

    #[test]
    fn hostname_change_preserves_local_aliases_and_comments() {
        let previous = "127.0.0.1 localhost\n127.0.1.1 old-host local-alias # local machine\n";
        let updated = hosts_with_hostname(previous, "linos-server");
        assert_eq!(
            updated,
            "127.0.0.1 localhost\n127.0.1.1\tlinos-server local-alias # local machine\n"
        );
    }
}
