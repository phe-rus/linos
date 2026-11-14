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
            let status = Command::new("/usr/sbin/locale-gen")
                .status()?;
            if !status.success() {
                return Err(io::Error::other("Locale generation failed"));
            }
        }
        fs::write("/etc/default/locale", format!("LANG={locale}\n"))?;
        fs::write(
            "/etc/default/keyboard",
            format!("XKBLAYOUT=\"{}\"\nXKBMODEL=\"pc105\"\n", self.keyboard),
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
        fs::write("/etc/timezone", format!("{}\n", self.timezone))?;
        fs::write("/etc/hostname", format!("{}\n", self.hostname))?;

        if self.network == Network::Dhcp {
            let config = Path::new("/etc/systemd/network/20-linos-dhcp.network");
            fs::create_dir_all(config.parent().ok_or_else(|| io::Error::other("Invalid network path"))?)?;
            let mut file = OpenOptions::new().write(true).create_new(true).open(config)?;
            file.write_all(b"[Match]\nName=en* eth*\n\n[Network]\nDHCP=yes\n")?;
            file.sync_all()?;
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
        && host.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames_are_strictly_validated() {
        for name in ["linos", "linos-server-1", "host7"] {
            assert!(valid_hostname(name));
        }
        for name in ["", "Bad", "-linos", "linos-", "has spaces", "a/b", "a".repeat(64).as_str()] {
            assert!(!valid_hostname(name));
        }
    }

    #[test]
    fn supported_settings_validate() {
        let valid = Settings::defaults("linos");
        valid.validate().unwrap();
        let invalid = Settings { timezone: "../etc/passwd".into(), ..valid };
        assert!(invalid.validate().is_err());
    }
}
