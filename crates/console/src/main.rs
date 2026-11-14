//! Linos 13 Server home and scripting CLI.
//! Interactive controls never alter the caller's shell or other TUIs.
use std::{
    env, fs,
    io::{self, IsTerminal},
    path::Path,
    process::{Command, ExitCode},
};

mod pty;
mod tui;

const PUBLIC_READY: &str = "/etc/linos/setup-complete";

fn interactive_terminal() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && env::var("TERM").is_ok_and(|term| term != "dumb")
}

fn value_in(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(name, value)| (name == key).then(|| value.trim_matches('"').to_owned()))
}

fn version() -> String {
    fs::read_to_string("/usr/lib/linos/release")
        .ok()
        .and_then(|data| value_in(&data, "VERSION_ID"))
        .unwrap_or_else(|| "13".to_owned())
}

fn host() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|text| text.trim().to_owned())
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn uptime() -> Option<String> {
    let raw = fs::read_to_string("/proc/uptime").ok()?;
    let seconds = raw.split_whitespace().next()?.parse::<f64>().ok()? as u64;
    Some(format!(
        "{}d {}h {}m",
        seconds / 86400,
        (seconds / 3600) % 24,
        (seconds / 60) % 60,
    ))
}

fn memory() -> Option<String> {
    let raw = fs::read_to_string("/proc/meminfo").ok()?;
    let field = |key: &str| -> Option<u64> {
        raw.lines()
            .find(|line| line.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    Some(format!(
        "{} / {} MiB used",
        (total - available) / 1024,
        total / 1024
    ))
}

fn ownership() -> &'static str {
    if Path::new(PUBLIC_READY).is_file() {
        "Configured"
    } else {
        // Owner records are intentionally private and inaccessible to normal
        // users. Missing a public marker does NOT prove a device is unowned.
        "Unavailable (restricted)"
    }
}

fn overview() -> String {
    format!(
        "Linos {} Server\nHost        {}\nUptime      {}\nMemory      {}\nSetup       {}",
        version(),
        host(),
        uptime().unwrap_or_else(|| "Unavailable".into()),
        memory().unwrap_or_else(|| "Unavailable".into()),
        ownership(),
    )
}

fn status() {
    println!("{}", overview());
}

fn valid_unit(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._@-".contains(&b))
}

fn command(program: &str, args: &[&str]) -> io::Result<ExitCode> {
    let status = Command::new(program).args(args).status()?;
    Ok(ExitCode::from(if status.success() { 0 } else { 1 }))
}

fn help() {
    println!("Linos 13 Server");
    println!("Usage: linos [status|network|services|service NAME|logs NAME|help]");
    println!("       linos /status | /network | /services | /logs | /shell");
    println!("       linos /logout | /reboot | /shutdown");
    println!("       linos default home|shell");
    println!("Linos Home is the default interactive Server environment.");
    println!("Use /shell or Ctrl+T to enter your shell, then exit to return to Home.");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Status,
    Network,
    Services,
    Logs,
    Help,
    Shell,
    Logout,
    Reboot,
    Shutdown,
    Resources,
    Assets,
    Settings,
    Updates,
}

fn action_for(value: &str) -> Option<Action> {
    match value.trim() {
        "/status" | "status" => Some(Action::Status),
        "/resources" | "resources" => Some(Action::Resources),
        "/assets" | "assets" => Some(Action::Assets),
        "/settings" | "settings" => Some(Action::Settings),
        "/updates" | "updates" => Some(Action::Updates),
        "/network" | "network" => Some(Action::Network),
        "/services" | "services" => Some(Action::Services),
        "/logs" | "logs" => Some(Action::Logs),
        "/help" | "help" => Some(Action::Help),
        "/shell" | "shell" | "exit" | "/exit" => Some(Action::Shell),
        "/logout" | "logout" => Some(Action::Logout),
        "/reboot" | "reboot" => Some(Action::Reboot),
        "/shutdown" | "shutdown" | "/poweroff" => Some(Action::Shutdown),
        _ => None,
    }
}

fn execute(action: Action) -> io::Result<ExitCode> {
    match action {
        Action::Resources | Action::Assets | Action::Settings | Action::Updates => {
            eprintln!("Open the interactive Linos Server home for this view.");
            Ok(ExitCode::from(2))
        }
        Action::Status => {
            status();
            Ok(ExitCode::SUCCESS)
        }
        Action::Network => command("ip", &["-brief", "address"]),
        Action::Services => command(
            "systemctl",
            &[
                "list-units",
                "--type=service",
                "--state=running",
                "--no-pager",
            ],
        ),
        Action::Logs => command("journalctl", &["--no-pager", "-n", "30"]),
        Action::Help => {
            help();
            Ok(ExitCode::SUCCESS)
        }
        Action::Shell => Ok(ExitCode::SUCCESS),
        Action::Logout => Ok(ExitCode::from(20)),
        Action::Reboot | Action::Shutdown => {
            // Only the interactive home may offer a confirmation dialog.
            // Non-interactive power commands are intentionally rejected.
            eprintln!("Use the interactive Linos home to confirm power actions.");
            Ok(ExitCode::from(2))
        }
    }
}

fn set_default_mode(mode: &str) -> io::Result<ExitCode> {
    if mode != "home" && mode != "shell" {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Choose home or shell"));
    }
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    let config_dir = Path::new(&home).join(".config/linos");
    fs::create_dir_all(&config_dir)?;
    // A per-user preference, not a privileged system authentication setting.
    fs::write(config_dir.join("default-mode"), format!("{mode}\n"))?;
    println!("Linos default mode: {mode}. Effective at your next sign-in.");
    Ok(ExitCode::SUCCESS)
}

fn home() -> io::Result<ExitCode> {
    tui::run()
}

fn run(args: &[String]) -> io::Result<ExitCode> {
    match args {
        [] if interactive_terminal() => home(),
        [] => {
            help();
            Ok(ExitCode::SUCCESS)
        }
        [command, mode] if command == "default" => set_default_mode(mode),
        [arg] if arg == "--help" || arg == "-h" => {
            help();
            Ok(ExitCode::SUCCESS)
        }
        [arg] if let Some(action) = action_for(arg) => execute(action),
        [arg, unit] if arg == "service" && valid_unit(unit) => {
            command("systemctl", &["status", "--no-pager", "--", unit])
        }
        [arg, unit] if arg == "logs" && valid_unit(unit) => {
            command("journalctl", &["--no-pager", "-n", "60", "-u", unit])
        }
        _ => {
            eprintln!("linos: unknown command. Try 'linos help'.");
            Ok(ExitCode::from(2))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("linos: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_service_names() {
        assert!(valid_unit("ssh.service"));
        assert!(valid_unit("linos-first-boot.service"));
        assert!(!valid_unit("--help"));
        assert!(!valid_unit("../ssh"));
        assert!(!valid_unit("ssh;whoami"));
    }

    #[test]
    fn exact_commands_only() {
        assert!(action_for("/status") == Some(Action::Status));
        assert!(action_for("/shell") == Some(Action::Shell));
        assert!(action_for("/logout") == Some(Action::Logout));
        assert!(action_for("/shutdown") == Some(Action::Shutdown));
        assert!(action_for("/status;id").is_none());
        assert!(action_for("not-a-command").is_none());
    }

    #[test]
    fn private_owner_record_is_not_a_negative_status() {
        assert_ne!(ownership(), "not confirmed");
    }

    #[test]
    fn release_fields() {
        assert_eq!(
            value_in("VERSION_ID=\"13\"\n", "VERSION_ID").as_deref(),
            Some("13")
        );
    }

    #[test]
    fn invalid_commands() {
        assert_eq!(run(&["install".into()]).unwrap(), ExitCode::from(2));
    }
}
