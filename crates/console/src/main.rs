//! Linos 13 Server home and scripting CLI.
//! Interactive controls never alter the caller's shell or other TUIs.
use std::{
    env, fs,
    io::{self, IsTerminal},
    path::Path,
    process::{Command, ExitCode},
};

const PUBLIC_READY: &str = "/etc/linos/setup-complete";

fn interactive_terminal() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && env::var_os("NO_COLOR").is_none()
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
    println!("Run 'linos' in an interactive terminal for guided management.");
    println!("A normal shell remains available after selecting 'Return to shell'.");
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
}

fn action_for(value: &str) -> Option<Action> {
    match value.trim() {
        "/status" | "status" => Some(Action::Status),
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
        Action::Status => {
            status();
            Ok(ExitCode::SUCCESS)
        }
        Action::Network => command("ip", &["-brief", "address"]),
        Action::Services => command(
            "systemctl",
            &["list-units", "--type=service", "--state=running", "--no-pager"],
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
        },
    }
}

fn choose_action() -> io::Result<Action> {
    cliclack::select("What would you like to do?")
        .item(Action::Status, "System overview", "/status")
        .item(Action::Network, "Network", "/network")
        .item(Action::Services, "Services", "/services")
        .item(Action::Logs, "Recent logs", "/logs")
        .item(Action::Help, "Commands", "/help")
        .item(Action::Shell, "Return to shell", "/shell")
        .item(Action::Logout, "Sign out", "/logout")
        .item(Action::Reboot, "Restart server", "/reboot")
        .item(Action::Shutdown, "Shut down server", "/shutdown")
        .interact()
}

fn home() -> io::Result<ExitCode> {
    // The app controls the screen only while active. It does not modify PS1,
    // login files, terminal modes or any third-party process.
    print!("\x1b[3J\x1b[2J\x1b[H");
    cliclack::intro(" Linos 13 · Server ")?;
    let user = env::var("USER").unwrap_or_else(|_| "operator".into());
    cliclack::log::info(format!("Welcome, {user}.  Host: {}", host()))?;
    cliclack::note("Your server", overview())?;
    loop {
        let action = choose_action()?;
        if action == Action::Shell {
            cliclack::outro("Returning to your shell")?;
            return Ok(ExitCode::SUCCESS);
        }
        if action == Action::Logout {
            cliclack::outro("Signing out. See you next time.")?;
            return Ok(ExitCode::from(20));
        }
        if matches!(action, Action::Reboot | Action::Shutdown) {
            let is_reboot = action == Action::Reboot;
            let verb = if is_reboot { "Restart" } else { "Shut down" };
            let confirmed = cliclack::confirm(format!(
                "{verb} {}? Active connections and services will be interrupted.",
                host()
            ))
            .initial_value(false)
            .interact()?;
            if !confirmed {
                cliclack::log::info("Cancelled. Your server is still running.")?;
                continue;
            }
            cliclack::log::step(if is_reboot {
                "Restarting Linos safely"
            } else {
                "Shutting down Linos safely"
            })?;
            let operation = if is_reboot { "reboot" } else { "poweroff" };
            let result = command("/usr/bin/systemctl", &[operation])?;
            if result != ExitCode::SUCCESS {
                cliclack::log::error("The system rejected the request. Check your permissions.")?;
            } else {
                cliclack::outro("The system is closing your session.")?;
                return Ok(ExitCode::SUCCESS);
            }
            continue;
        }
        if action == Action::Help {
            cliclack::note(
                "Quick commands",
                "linos status · linos network · linos services\n\
                 linos service ssh.service · linos logs ssh.service\n\
                 All commands can be used non-interactively.",
            )?;
            continue;
        }
        if action == Action::Status {
            cliclack::note("System overview", overview())?;
            continue;
        }
        cliclack::log::step("Opening system output")?;
        let _ = execute(action)?;
        cliclack::log::info("Press the arrow keys to select another action.")?;
    }
}

fn run(args: &[String]) -> io::Result<ExitCode> {
    match args {
        [] if interactive_terminal() => home(),
        [] => {
            help();
            Ok(ExitCode::SUCCESS)
        }
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
