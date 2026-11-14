//! Consistent, responsive first-boot console presentation.
//! This module never handles credentials; the system's passwd utility does.
use std::{
    env,
    io::{self, IsTerminal, Write},
    process::Command,
    thread,
    time::Duration,
};

#[derive(Clone, Copy)]
enum Stage {
    Username,
    Password,
    Complete,
}

const DEFAULT_WIDTH: usize = 50;
const MIN_WIDTH: usize = 38;
const MAX_WIDTH: usize = 60;
const INDENT: &str = "  ";
const TITLE: &str = "Linos 13 · Server setup";

fn visual() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && env::var_os("NO_COLOR").is_none()
        && env::var("TERM").map_or(true, |term| term != "dumb")
}

fn terminal_width() -> usize {
    let from_stty = Command::new("/usr/bin/stty")
        .arg("size")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|size| size.split_whitespace().nth(1)?.parse::<usize>().ok());
    let from_env = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok());
    let columns = from_stty.filter(|width| *width > 0).or(from_env);
    columns.unwrap_or(DEFAULT_WIDTH + 2).saturating_sub(4)
}

fn frame_width(available: usize) -> Option<usize> {
    (available >= MIN_WIDTH).then(|| available.min(MAX_WIDTH))
}

fn frame_line(content: &str, width: usize) -> String {
    let text_width = content.chars().count();
    let gap = width.saturating_sub(text_width + 4);
    format!("{INDENT}│  {content}{}│\n", " ".repeat(gap))
}

fn frame(width: usize) -> String {
    let title_width = TITLE.chars().count();
    let top_gap = width.saturating_sub(title_width + 5);
    let mut out = format!("{INDENT}╭─ {TITLE} {}╮\n", "─".repeat(top_gap));
    out.push_str(&frame_line("", width));
    out.push_str(&frame_line("Welcome to Linos.", width));
    out.push_str(&frame_line("Let's make this server yours.", width));
    out.push_str(&frame_line("", width));
    out.push_str(&format!("{INDENT}╰{}╯\n", "─".repeat(width - 2)));
    out
}

fn plain_page(stage: Stage, name: Option<&str>) -> String {
    match stage {
        Stage::Username => "Linos 13 Server setup\nWelcome to Linos.\nUsername: ".to_owned(),
        Stage::Password => format!("Username: {}\nSet your password.\n", name.unwrap_or("")),
        Stage::Complete => format!(
            "Setup complete.\nSign in as {} with your password.\n",
            name.unwrap_or("")
        ),
    }
}

fn page(stage: Stage, name: Option<&str>, width: usize, styled: bool) -> String {
    if !styled {
        return plain_page(stage, name);
    }
    let Some(width) = frame_width(width) else {
        return plain_page(stage, name);
    };
    let cyan = "\x1b[36m";
    let green = "\x1b[32m";
    let reset = "\x1b[0m";
    let mut out = format!("\x1b[2J\x1b[H\n{}\n", frame(width));
    match stage {
        Stage::Username => {
            out.push_str(&format!(
                "{INDENT}{cyan}◆{reset}  Your account\n{INDENT}│  Choose a name to sign in with.\n{INDENT}│\n{INDENT}└─ Username: "
            ));
        }
        Stage::Password => {
            out.push_str(&format!(
                "{INDENT}{green}◇{reset}  Username\n{INDENT}│  {}\n{INDENT}│\n{INDENT}{cyan}◆{reset}  Secure your account\n{INDENT}│  Choose a strong password.\n{INDENT}└─ Set password\n\n",
                name.unwrap_or("")
            ));
        }
        Stage::Complete => {
            out.push_str(&format!(
                "{INDENT}{green}◇{reset}  Username\n{INDENT}│  {}\n{INDENT}│\n{INDENT}{green}◇{reset}  Password\n{INDENT}│  Secured\n{INDENT}│\n{INDENT}{green}●{reset}  You're all set\n\n{INDENT}Sign in as {} with your password.\n",
                name.unwrap_or(""),
                name.unwrap_or("")
            ));
        }
    }
    out
}

fn styled() -> bool {
    visual() && frame_width(terminal_width()).is_some()
}

fn display(stage: Stage, name: Option<&str>) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(page(stage, name, terminal_width(), styled()).as_bytes())?;
    stdout.flush()
}

fn end_boot_status_output() -> io::Result<()> {
    let status = Command::new("/usr/bin/kill")
        .args(["-s", "RTMIN+21", "1"])
        .status()?;
    if !status.success() {
        return Err(io::Error::other("Could not finish boot console handoff"));
    }
    thread::sleep(Duration::from_millis(150));
    Ok(())
}

pub fn start() -> io::Result<()> {
    if visual() {
        end_boot_status_output()?;
    }
    display(Stage::Username, None)
}

pub fn username_prompt() -> io::Result<()> {
    // The form itself renders the prompt in interactive mode.
    // Plain output also ends in "Username: " for serial automation.
    io::stdout().flush()
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
    fn all_frame_edges_have_the_same_width() {
        for width in [38, 42, 50, 60] {
            let panel = frame(width);
            let lines: Vec<_> = panel.lines().collect();
            assert_eq!(lines.len(), 6);
            for line in lines {
                assert_eq!(line.chars().count(), width + 2, "{line}");
            }
        }
    }

    #[test]
    fn responsive_width_clamps_or_falls_back() {
        assert_eq!(frame_width(20), None);
        assert_eq!(frame_width(38), Some(38));
        assert_eq!(frame_width(120), Some(60));
    }

    #[test]
    fn plain_mode_does_not_clear_terminal() {
        for stage in [Stage::Username, Stage::Password, Stage::Complete] {
            assert!(!page(stage, Some("linos"), 50, false).contains('\x1b'));
            assert!(!page(stage, Some("linos"), 20, true).contains('\x1b'));
        }
    }

    #[test]
    fn flow_is_consistent_across_stages() {
        assert!(page(Stage::Username, None, 50, true).contains("Username: "));
        assert!(page(Stage::Password, Some("linos"), 50, true).contains("Set password"));
        assert!(page(Stage::Complete, Some("linos"), 50, true).contains("Sign in as linos"));
    }
}
