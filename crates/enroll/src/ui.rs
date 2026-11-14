//! First-boot presentation. Presentation only: account and password changes
//! remain in the independently validated enrollment and passwd paths.
use std::{
    env,
    io::{self, IsTerminal, Write},
};

#[derive(Clone, Copy)]
enum Stage {
    Username,
    Password,
    Complete,
}

fn visual() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && env::var_os("NO_COLOR").is_none()
        && env::var("TERM").is_err_or(|term| term != "dumb")
}

fn page(stage: Stage, name: Option<&str>, styled: bool) -> String {
    if !styled {
        return match stage {
            Stage::Username => {
                "Linos 13 Server setup\nWelcome to Linos. Let's set up your account.\n".to_owned()
            }
            Stage::Password => format!(
                "Username: {}\nSet a password to protect your account.\n",
                name.unwrap_or("")
            ),
            Stage::Complete => format!(
                "Linos setup complete.\nSign in as {} with the password you created.\n",
                name.unwrap_or("")
            ),
        };
    }

    let cyan = "\x1b[36m";
    let green = "\x1b[32m";
    let reset = "\x1b[0m";
    let mut screen = String::from(
        "\x1b[2J\x1b[H\n  \x1b[1m╭─ Linos 13 · Server setup ───────────────╮\x1b[0m\n  │                                        │\n  │  Welcome to Linos.                     │\n  │  Let's make this server yours.         │\n  ╰────────────────────────────────────────╯\n\n",
    );

    match stage {
        Stage::Username => {
            screen.push_str(&format!(
                "  {cyan}◆{reset}  Choose your username\n  │  This is how you'll sign in.\n  │\n  └  Username: "
            ));
        }
        Stage::Password => {
            screen.push_str(&format!(
                "  {green}◇{reset}  Username\n  │  {}\n  │\n  {cyan}◆{reset}  Secure your account\n  │  Choose a strong password.\n  └  Password setup\n\n",
                name.unwrap_or("")
            ));
        }
        Stage::Complete => {
            screen.push_str(&format!(
                "  {green}◇{reset}  Username\n  │  {}\n  │\n  {green}◇{reset}  Password\n  │  Saved securely\n  │\n  {green}●{reset}  Setup complete\n\n  Sign in as {} using the password you created.\n",
                name.unwrap_or(""),
                name.unwrap_or("")
            ));
        }
    }
    screen
}

fn display(stage: Stage, name: Option<&str>) -> io::Result<()> {
    let styled = visual();
    let mut stdout = io::stdout().lock();
    stdout.write_all(page(stage, name, styled).as_bytes())?;
    stdout.flush()
}

pub fn start() -> io::Result<()> {
    display(Stage::Username, None)
}

pub fn username_prompt() -> io::Result<()> {
    if !visual() {
        print!("Username: ");
        io::stdout().flush()?;
    }
    Ok(())
}

pub fn password(username: &str) -> io::Result<()> {
    display(Stage::Password, Some(username))
}

pub fn completed(username: &str) -> io::Result<()> {
    display(Stage::Complete, Some(username))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_mode_never_emits_control_sequences() {
        for stage in [Stage::Username, Stage::Password, Stage::Complete] {
            let output = page(stage, Some("operator"), false);
            assert!(!output.contains('\x1b'));
        }
    }

    #[test]
    fn guided_setup_is_a_connected_sequence() {
        let first = page(Stage::Username, None, true);
        let second = page(Stage::Password, Some("operator"), true);
        let completed = page(Stage::Complete, Some("operator"), true);
        assert!(first.contains("Username: "));
        assert!(second.contains("operator"));
        assert!(second.contains("Password setup"));
        assert!(completed.contains("Setup complete"));
        assert!(completed.contains("Sign in as operator"));
    }
}
