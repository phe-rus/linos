//! Trusted local console username presentation.
//! Does NOT authenticate users: replaces itself with login(1), which uses PAM.
//! This program is invoked by agetty as root, and must not shell-expand input.
use std::{
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Write},
    os::unix::fs::OpenOptionsExt,
    os::unix::process::CommandExt,
    process::Command,
};

fn allowed_username(username: &str) -> bool {
    !username.is_empty()
        && username.len() <= 32
        && !username.starts_with('-')
        && username
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'_' || ch == b'-')
}

fn suggested_username() -> String {
    // Optional, root-managed, public username hint; never a password.
    fs::read_to_string("/etc/linos/login-user")
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|name| allowed_username(name))
        .unwrap_or_default()
}

fn prompt() -> io::Result<String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("Not a trusted interactive console"));
    }
    // Boot progress remains visible until local sign-in takes ownership.
    // Switching off console progress does not change journal logging.
    let _ = Command::new("/usr/bin/kill")
        .args(["-s", "RTMIN+21", "1"])
        .status();
    print!("\x1b[3J\x1b[2J\x1b[H");
    io::stdout().flush()?;

    cliclack::intro(" Linos 13 · Sign in ")?;
    cliclack::log::info("Welcome back. Sign in to manage your server.")?;
    let hint = suggested_username();
    let mut question = cliclack::input("Username").placeholder("Your account name");
    if !hint.is_empty() {
        question = question.default_input(&hint);
    }
    let username: String = question
        .validate(|text: &String| {
            if allowed_username(text) {
                Ok(())
            } else {
                Err("Enter a valid local account name.")
            }
        })
        .interact()?;
    cliclack::log::step(format!("Continue as {username}"))?;
    // We do not ask for or store passwords. login(1) handles the PAM flow.
    Ok(username)
}

// Invoked only by the PAM login session hook after successful authentication.
// The remembered value is a public username hint, never an authentication token.
fn record_authenticated_session() -> io::Result<()> {
    if std::env::var("PAM_SERVICE").as_deref() != Ok("login")
        || std::env::var("PAM_TYPE").as_deref() != Ok("open_session")
    {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Not a PAM login session"));
    }
    let tty = std::env::var("PAM_TTY").unwrap_or_default();
    let tty = tty.strip_prefix("/dev/").unwrap_or(&tty);
    if tty != "tty1" && !tty.starts_with("ttyS") && !tty.starts_with("ttyAMA") {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Not a supported local console"));
    }
    let user = std::env::var("PAM_USER").unwrap_or_default();
    if !allowed_username(&user) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid local username"));
    }
    fs::create_dir_all("/etc/linos")?;
    // The containing directory is root-owned. Create and atomically rename a
    // private temporary file rather than following an existing destination.
    let temp = format!("/etc/linos/.login-user-{}", std::process::id());
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
        writeln!(file, "{user}")?;
        file.sync_all()?;
        fs::rename(&temp, "/etc/linos/login-user")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--record-session") {
        if let Err(error) = record_authenticated_session() {
            eprintln!("Unable to remember login username: {error}");
            std::process::exit(1);
        }
        return;
    }
    // agetty's login-program runs as root. Treat all parameters as untrusted.
    // Only an absolute program path is used; a user value follows "--".
    let mut cmd = Command::new("/bin/login");
    match prompt() {
        Ok(user) => {
            cmd.arg("--").arg(user);
        }
        Err(error) => {
            eprintln!("Linos sign-in unavailable ({error}); using standard login.");
            // No username is supplied, so login(1) asks normally.
        }
    }
    let error = cmd.exec();
    eprintln!("Unable to start trusted system login: {error}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_option_injection_and_control_characters() {
        for value in ["-f", "--help", "root admin", "abc\n", "", "../etc/shadow"] {
            assert!(!allowed_username(value));
        }
        for value in ["linos", "linos_admin", "my-server1"] {
            assert!(allowed_username(value));
        }
    }
}
