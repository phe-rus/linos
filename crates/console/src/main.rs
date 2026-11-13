//! Linos 13 Server command-line experience.
//! Deliberately changes no parent-shell state or terminal settings.
use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    process::{Command, ExitCode},
};

fn decorated() -> bool {
    io::stdout().is_terminal()
        && env::var_os("NO_COLOR").is_none()
        && env::var("TERM").is_ok_and(|term| term != "dumb")
}

fn heading(label: &str) {
    if decorated() {
        println!("\n  \x1b[1m{label}\x1b[0m");
    } else {
        println!("\n{label}");
    }
}

fn item(label: &str, value: &str) {
    if io::stdout().is_terminal() {
        println!("  {label:<15} {value}");
    } else {
        println!("{label}: {value}");
    }
}

fn value_in(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(name, value)| (name == key).then(|| value.trim_matches('"').to_owned()))
}

fn uptime() -> Option<String> {
    let raw = fs::read_to_string("/proc/uptime").ok()?;
    let seconds = raw.split_whitespace().next()?.parse::<f64>().ok()? as u64;
    Some(format!("{}d {}h {}m", seconds / 86400, (seconds / 3600) % 24, (seconds / 60) % 60))
}

fn memory() -> Option<String> {
    let raw = fs::read_to_string("/proc/meminfo").ok()?;
    let field = |key: &str| -> Option<u64> {
        raw.lines().find(|line| line.starts_with(key))?
            .split_whitespace().nth(1)?.parse().ok()
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    Some(format!("{} / {} MiB used", (total - available) / 1024, total / 1024))
}

fn release() -> String {
    fs::read_to_string("/usr/lib/linos/release")
        .ok()
        .and_then(|data| value_in(&data, "VERSION_ID"))
        .unwrap_or_else(|| "13".into())
}

fn status() {
    heading("Linos 13  /  Server");
    item("Version", &release());
    item("Hostname", &fs::read_to_string("/etc/hostname")
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|_| "unknown".into()));
    if let Some(value) = uptime() { item("Uptime", &value); }
    if let Some(value) = memory() { item("Memory", &value); }
    let provisioned = fs::metadata("/var/lib/linos/owner").is_ok();
    item("Ownership", if provisioned { "locally enrolled" } else { "not confirmed" });
    println!();
}

fn valid_unit(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._@-".contains(&b))
        && !name.contains("..")
}

fn command(program: &str, args: &[&str]) -> io::Result<ExitCode> {
    let status = Command::new(program).args(args).status()?;
    Ok(ExitCode::from(if status.success() { 0 } else { 1 }))
}

fn help() {
    println!("Linos 13 Server");
    println!("Usage: linos [status|network|services|service NAME|logs NAME|help]");
    println!();
    println!("  status        Local device overview");
    println!("  network       Network interfaces and addresses");
    println!("  services      Running system services");
    println!("  service NAME  Inspect a systemd service");
    println!("  logs NAME     Show recent journal entries for a service");
    println!();
    println!("Run 'linos' in an interactive terminal for the guided menu.");
    println!("Normal shell commands and third-party terminal applications are unchanged.");
}

fn interactive() -> io::Result<ExitCode> {
    loop {
        heading("Linos 13  /  Server");
        println!("  1  System overview");
        println!("  2  Network");
        println!("  3  Running services");
        println!("  4  Exit to shell");
        print!("\n  Choose an option: ");
        io::stdout().flush()?;
        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 { return Ok(ExitCode::SUCCESS); }
        match input.trim() {
            "1" => status(),
            "2" => { command("ip", &["-brief", "address"])?; },
            "3" => { command("systemctl", &["list-units", "--type=service", "--state=running", "--no-pager"])?; },
            "4" | "q" | "exit" => return Ok(ExitCode::SUCCESS),
            _ => println!("  Choose 1, 2, 3 or 4."),
        }
    }
}

fn run(args: &[String]) -> io::Result<ExitCode> {
    match args {
        [] if io::stdin().is_terminal() && io::stdout().is_terminal() => interactive(),
        [] => { help(); Ok(ExitCode::SUCCESS) },
        [arg] if arg == "help" || arg == "--help" || arg == "-h" => {
            help(); Ok(ExitCode::SUCCESS)
        }
        [arg] if arg == "status" => { status(); Ok(ExitCode::SUCCESS) },
        [arg] if arg == "network" => command("ip", &["-brief", "address"]),
        [arg] if arg == "services" => command("systemctl", &["list-units", "--type=service", "--state=running", "--no-pager"]),
        [arg, name] if arg == "service" && valid_unit(name) =>
            command("systemctl", &["status", "--no-pager", "--", name]),
        [arg, name] if arg == "logs" && valid_unit(name) =>
            command("journalctl", &["--no-pager", "-n", "60", "-u", name]),
        _ => {
            eprintln!("linos: invalid command or service name. Run 'linos help'.");
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
    fn service_names_are_safe() {
        assert!(valid_unit("ssh.service"));
        assert!(valid_unit("linos-first-boot.service"));
        assert!(valid_unit("getty@tty1.service"));
        assert!(!valid_unit("--help"));
        assert!(!valid_unit("../ssh"));
        assert!(!valid_unit("ssh;whoami"));
        assert!(!valid_unit(""));
    }
    #[test]
    fn release_fields_are_read_safely() {
        assert_eq!(value_in("VERSION_ID=\"13\"\n", "VERSION_ID").as_deref(), Some("13"));
        assert_eq!(value_in("OTHER=x\n", "VERSION_ID"), None);
    }
    #[test]
    fn unknown_commands_fail() {
        assert_eq!(run(&["install".into()]).unwrap(), ExitCode::from(2));
        assert_eq!(run(&["service".into(), "../bad".into()]).unwrap(), ExitCode::from(2));
    }
}
