//! Real per-user preferences shared with the login-shell startup contract.
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Preferences {
    pub mode: String,
    pub color: String,
    pub refresh: u64,
    directory: PathBuf,
}

impl Preferences {
    pub fn load() -> Self {
        let directory = env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| PathBuf::from(home).join(".config/linos"))
            .unwrap_or_default();
        Self::at(directory)
    }

    fn at(directory: PathBuf) -> Self {
        let read = |name: &str| {
            if directory.as_os_str().is_empty() {
                return String::new();
            }
            fs::read_to_string(directory.join(name))
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let mode = read("default-mode");
        let color = read("color");
        let refresh = read("refresh-seconds").parse().unwrap_or(2);
        Self {
            mode: if mode == "shell" { mode } else { "home".into() },
            color: if matches!(color.as_str(), "always" | "never") {
                color
            } else {
                "auto".into()
            },
            refresh: if (1..=10).contains(&refresh) {
                refresh
            } else {
                2
            },
            directory,
        }
    }

    pub fn set(&mut self, name: &str, value: &str) -> io::Result<()> {
        if self.directory.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "HOME is not set; preferences were not saved",
            ));
        }
        let file = match (name, value) {
            ("default", "home" | "shell") => "default-mode",
            ("color", "auto" | "always" | "never") => "color",
            ("refresh", value) if value.parse::<u64>().is_ok_and(|n| (1..=10).contains(&n)) => {
                "refresh-seconds"
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Use /settings default home|shell, color auto|always|never, or refresh 1..10",
                ));
            }
        };
        fs::create_dir_all(&self.directory)?;
        // Publish the complete value at once so a concurrent sign-in never
        // observes an empty or partially written preference.
        let temp = self
            .directory
            .join(format!(".{file}.{}", std::process::id()));
        let result = (|| {
            use std::io::Write;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            writeln!(output, "{value}")?;
            output.sync_all()?;
            fs::rename(&temp, self.directory.join(file))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        match name {
            "default" => self.mode = value.into(),
            "color" => self.color = value.into(),
            "refresh" => self.refresh = value.parse().unwrap(),
            _ => unreachable!(),
        }
        Ok(())
    }

    pub fn colors(&self) -> bool {
        match self.color.as_str() {
            "always" => true,
            "never" => false,
            _ => env::var_os("NO_COLOR").is_none() && env::var("TERM").is_ok_and(|s| s != "dumb"),
        }
    }
}

/// Values passed as separate arguments to system tools, never shell fragments.
pub fn system_change(name: &str, value: &str) -> io::Result<(&'static str, &'static str)> {
    match name {
        "hostname"
            if !value.is_empty()
                && value.len() <= 63
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !value.starts_with('-')
                && !value.ends_with('-') =>
        {
            Ok(("hostnamectl", "set-hostname"))
        }
        "timezone"
            if !value.starts_with('/')
                && !value.contains("..")
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b))
                && Path::new("/usr/share/zoneinfo").join(value).is_file() =>
        {
            Ok(("timedatectl", "set-timezone"))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a valid hostname or installed timezone, for example Africa/Nairobi",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_persist_and_reject_invalid_values() {
        let directory = env::current_dir()
            .unwrap()
            .join(format!("target/preferences-test-{}", std::process::id()));
        let mut p = Preferences::at(directory.clone());
        p.set("default", "shell").unwrap();
        p.set("color", "never").unwrap();
        p.set("refresh", "1").unwrap();
        let loaded = Preferences::at(directory.clone());
        assert_eq!(
            (loaded.mode.as_str(), loaded.color.as_str(), loaded.refresh),
            ("shell", "never", 1)
        );
        assert!(!loaded.colors());
        assert!(p.set("refresh", "0").is_err());
        assert!(p.set("default", "shell;id").is_err());
        assert_eq!(Preferences::at(directory.clone()).mode, "shell");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn system_changes_are_validated_before_confirmation() {
        assert!(system_change("hostname", "my-server").is_ok());
        for value in ["", "-x", "x-", "a;id", "a b", "$(id)"] {
            assert!(system_change("hostname", value).is_err());
        }
        assert!(system_change("timezone", "../etc/passwd").is_err());
        assert!(system_change("timezone", "/etc/passwd").is_err());
        assert!(system_change("timezone", "Africa/Nairobi").is_ok());
    }
}
