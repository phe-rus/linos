//! Clack-style first-boot experience for the native Linos Server console.
//! The UI only collects choices. System configuration lives in setup.rs.
use std::{
    env,
    io::{self, IsTerminal, Write},
    process::Command,
    thread,
    time::Duration,
};

use crate::setup::{Language, Network, Settings};

fn interactive() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && env::var("TERM").is_ok_and(|value| value != "dumb")
        && env::var_os("NO_COLOR").is_none()
}

fn end_boot_status_output() -> io::Result<()> {
    let status = Command::new("/usr/bin/kill")
        .args(["-s", "RTMIN+21", "1"])
        .status()?;
    if !status.success() {
        return Err(io::Error::other("Unable to reserve the setup terminal"));
    }
    thread::sleep(Duration::from_millis(150));
    Ok(())
}

pub fn start() -> io::Result<()> {
    if interactive() {
        end_boot_status_output()?;
        // Clear the visible terminal AND supported scrollback, not just scroll up.
        print!("\x1b[3J\x1b[2J\x1b[H");
        io::stdout().flush()?;
        cliclack::clear_screen()?;
        cliclack::intro(" Linos 13 · Server setup ")?;
        cliclack::log::info("Welcome to Linos. Let's make this server yours.")?;
    } else {
        println!("Linos 13 Server setup");
        println!("Welcome to Linos.");
    }
    Ok(())
}

pub fn username() -> io::Result<String> {
    if interactive() {
        cliclack::input("Username")
            .placeholder("Your administrator login")
            .validate(|name: &String| {
                if crate::validate_username(name) {
                    Ok(())
                } else {
                    Err("Use 1-32 lowercase letters, numbers, hyphens or underscores.")
                }
            })
            .interact()
    } else {
        print!("Username: ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(line.trim().to_owned())
    }
}

pub fn password(username: &str) -> io::Result<()> {
    if interactive() {
        cliclack::log::step(format!("Securing administrator account {username}"))?;
        cliclack::log::info("Enter and confirm your password using the system password prompt.")?;
    } else {
        println!("Set password for {username}");
    }
    Ok(())
}

pub fn configure(username: &str, _current_hostname: &str) -> io::Result<Settings> {
    if !interactive() {
        println!("Using system defaults for region and networking.");
        return Ok(Settings::defaults(&crate::setup::hostname_from_username(
            username,
        )));
    }

    cliclack::log::step("Choose your preferences. Arrow keys scroll options; Enter selects.")?;
    let language = cliclack::select("Language")
        .item(Language::English, "English", "Default")
        .item(Language::French, "Français", "")
        .item(Language::Chinese, "中文", "")
        .interact()?;

    let keyboard: &str = cliclack::select("Keyboard layout")
        .item("us", "English (US)", "Default")
        .item("gb", "English (UK)", "")
        .item("fr", "Français (AZERTY)", "")
        .item("de", "Deutsch (QWERTZ)", "")
        .interact()?;

    let timezone: &str = cliclack::select("Time zone / location")
        .item("Etc/UTC", "UTC", "Coordinated Universal Time")
        .item("Africa/Kampala", "Africa / Kampala", "")
        .item("Europe/London", "Europe / London", "")
        .item("Europe/Paris", "Europe / Paris", "")
        .item("America/New_York", "America / New York", "")
        .item("Asia/Shanghai", "Asia / Shanghai", "")
        .item("Asia/Kolkata", "Asia / Kolkata", "")
        .interact()?;

    let hostname: String = cliclack::input("Server hostname")
        .default_input(&crate::setup::hostname_from_username(username))
        .validate(|value: &String| {
            if crate::setup::valid_hostname(value) {
                Ok(())
            } else {
                Err("Use 1-63 lowercase letters, digits or hyphens.")
            }
        })
        .interact()?;

    let network = cliclack::select("Network configuration")
        .item(
            Network::Offline,
            "Configure later",
            "Recommended for offline testing",
        )
        .item(
            Network::Dhcp,
            "Automatic DHCP",
            "Enable networkd after setup",
        )
        .interact()?;

    let summary = format!(
        "Language: {}\nKeyboard: {keyboard}\nTime zone: {timezone}\nHostname: {hostname}\nNetworking: {}",
        language.label(),
        network.label()
    );
    cliclack::note("Review server settings", summary)?;
    let accepted = cliclack::confirm("Apply these settings?")
        .initial_value(true)
        .interact()?;
    if !accepted {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Setup was not confirmed; sign-in settings were not committed.",
        ));
    }
    Ok(Settings {
        language,
        keyboard: keyboard.to_owned(),
        timezone: timezone.to_owned(),
        hostname,
        network,
    })
}

pub fn completed(username: &str) -> io::Result<()> {
    if interactive() {
        cliclack::outro(format!(
            "Setup complete. Sign in as {username} with your password."
        ))?;
    } else {
        println!("Setup complete. Sign in as {username}.");
    }
    Ok(())
}
