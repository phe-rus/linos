//! First-boot UI. Only presentation; passwd remains responsible for credentials.
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
        && env::var("TERM").map_or(true, |term| term != "dumb")
}

const INNER_WIDTH: usize = 42;

fn box_line(text: &str) -> String {
    let used = text.chars().count() + 2;
    format!(
        "  │  {}{}│\n",
        text,
        " ".repeat(INNER_WIDTH.saturating_sub(used))
    )
}

fn welcome() -> String {
    let title = "Linos 13 · Server setup";
    let mut value = format!(
        "\n  ╭─ {title} {}╮\n",
        "─".repeat(INNER_WIDTH.saturating_sub(title.chars().count() + 3))
    );
    value.push_str(&box_line(""));
    value.push_str(&box_line("Welcome to Linos."));
    value.push_str(&box_line("Let's make this server yours."));
    value.push_str(&box_line(""));
    value.push_str(&format!("  ╰{}╯\n\n", "─".repeat(INNER_WIDTH)));
    value
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
    let mut screen = format!("\x1b[2J\x1b[H{}", welcome());
    match stage {
        Stage::Username => {
            screen.push_str(&format!("  {cyan}◆{reset}  Choose your username\n  │  This is how you'll sign in.\n  │\n  └─ Username: "));
        }
        Stage::Password => {
            screen.push_str(&format!("  {green}◇{reset}  Username\n  │  {}\n  │\n  {cyan}◆{reset}  Secure your account\n  │  Choose a strong password.\n  └─ Set password\n\n", name.unwrap_or("")));
        }
        Stage::Complete => {
            screen.push_str(&format!("  {green}◇{reset}  Username\n  │  {}\n  │\n  {green}◇{reset}  Password\n  │  Saved securely\n  │\n  {green}●{reset}  Setup complete\n\n  Sign in as {} using the password you created.\n", name.unwrap_or(""), name.unwrap_or("")));
        }
    }
    screen
}

fn display(stage: Stage, name: Option<&str>) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(page(stage, name, visual()).as_bytes())?;
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
    fn box_borders_align() {
        let frame = welcome();
        let lines: Vec<_> = frame
            .lines()
            .filter(|line| {
                line.starts_with("  ╭") || line.starts_with("  │") || line.starts_with("  ╰")
            })
            .collect();
        assert_eq!(lines.len(), 6);
        for line in lines {
            assert_eq!(line.chars().count(), INNER_WIDTH + 4, "{line}");
        }
    }

    #[test]
    fn plain_mode_has_no_escape_sequences() {
        for stage in [Stage::Username, Stage::Password, Stage::Complete] {
            assert!(!page(stage, Some("linos"), false).contains('\x1b'));
        }
    }

    #[test]
    fn connected_setup_has_all_stages() {
        assert!(page(Stage::Username, None, true).contains("Username: "));
        assert!(page(Stage::Password, Some("linos"), true).contains("Set password"));
        assert!(page(Stage::Complete, Some("linos"), true).contains("Sign in as linos"));
    }
}
