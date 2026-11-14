//! A composer-first Linos Server terminal experience.
//! Deliberately sparse: the workspace expands, command entry stays at the bottom.
use std::{
    env, fs, io,
    process::{Command, ExitCode},
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{Action, action_for, assets::Browser, geometry, host, memory, ownership, pty, uptime, version};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::Gray;
const COMMANDS: &[(Action, &str, &str)] = &[
    (Action::Home, "/home", "Return to the Linos Home workspace"),
    (
        Action::Status,
        "/status",
        "Show the health and identity of this server",
    ),
    (Action::Resources, "/resources", "CPU, memory, swap and storage"),
    (Action::Assets, "/assets", "Browse your home directory"),
    (Action::Settings, "/settings", "System settings and About"),
    (Action::Updates, "/updates", "Update policy and package status"),
    (
        Action::Network,
        "/network",
        "Inspect connected network interfaces",
    ),
    (
        Action::Services,
        "/services",
        "Inspect running system services",
    ),
    (Action::Logs, "/logs", "Read recent journal messages"),
    (Action::Help, "/help", "See keyboard shortcuts and commands"),
    (Action::Shell, "/shell", "Switch to the interactive Linux shell"),
    (Action::Logout, "/logout", "Sign out of this server"),
    (
        Action::Reboot,
        "/reboot",
        "Restart safely (asks for confirmation)",
    ),
    (
        Action::Shutdown,
        "/shutdown",
        "Power off safely (asks for confirmation)",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Home,
    Status,
    Network,
    Services,
    Logs,
    Help,
    Resources,
    Assets,
    Settings,
    Updates,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exit {
    Shell,
    Logout,
    Power,
}

struct App {
    input: String,
    highlighted: usize,
    view: View,
    content: String,
    notice: String,
    confirm: Option<Action>,
    scroll: u16,
    user: String,
    session: Option<pty::Session>,
    terminal_output: String,
    follow_tail: bool,
    assets: Browser,
    cwd: std::path::PathBuf,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            highlighted: 0,
            view: View::Home,
            content: String::new(),
            notice: "Ready".into(),
            confirm: None,
            scroll: 0,
            user: env::var("USER").unwrap_or_else(|_| "operator".into()),
            session: None,
            terminal_output: String::new(),
            follow_tail: true,
            assets: Browser::new(),
            cwd: env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")),
        }
    }

    fn suggestions(&self) -> Vec<(Action, &'static str, &'static str)> {
        if !self.input.starts_with('/') {
            return Vec::new();
        }
        let query = self.input.trim().to_lowercase();
        COMMANDS
            .iter()
            .filter(|(_, command, _)| command.starts_with(&query))
            .copied()
            .collect()
    }

    fn navigate(&mut self, view: View) {
        self.view = view;
        self.scroll = 0;
        self.content = match view {
            View::Home => String::new(),
            View::Status => format!(
                "Linos {} Server\n\nDevice      {}\nSession     {}\nUptime      {}\nMemory      {}\nSetup       {}",
                version(),
                host(),
                self.user,
                uptime().unwrap_or_else(|| "Unavailable".into()),
                memory().unwrap_or_else(|| "Unavailable".into()),
                ownership(),
            ),
            View::Resources => resources(),
            View::Assets => self.assets.display(),
            View::Settings => settings(),
            View::Updates => updates(),
            View::Terminal => self.terminal_output.clone(),
            View::Network => system_output("/usr/sbin/ip", &["-brief", "address"]),
            View::Services => system_output(
                "/usr/bin/systemctl",
                &[
                    "list-units",
                    "--type=service",
                    "--state=running",
                    "--no-pager",
                    "--plain",
                ],
            ),
            View::Logs => system_output(
                "/usr/bin/journalctl",
                &["--no-pager", "-n", "50", "--output=short"],
            ),
            View::Help => [
                "Linos Server commands",
                "",
                "/home         Return to the Home workspace",
                "clear          Clear PTY output without resetting shell state",
                "/status       Overview and device health",
                "/resources    CPU, memory, swap and filesystem capacity",
                "/assets       Browse your home directory (read-only)",
                "/settings     Device settings and About",
                "/updates      Update policy and installed package status",
                "/network      Network interfaces",
                "/services     Running services",
                "/logs         Recent system events",
                "/help         This guide",
                "/shell        Switch temporarily to the Linux shell",
                "/logout       Sign out",
                "/reboot       Restart (requires confirmation)",
                "/shutdown     Power off (requires confirmation)",
                "",
                "Type / to discover commands, or enter a Linux shell command.",
                "Linux commands run in a persistent PTY, including cd and pipelines.",
                "/home returns from terminal output to your Home workspace.",
                "Use Up/Down to select, Enter to run, Esc to clear.",
                "PageUp/PageDown scroll results. Ctrl+T switches to shell.",
                "Ctrl+T switches Home/Shell · Ctrl+R fits serial terminal size.",
                "",
                "Claude Code, Codex and other TUIs run normally in your shell.",
            ]
            .join("\n"),
        };
        self.notice = match view {
            View::Home => "Ready".into(),
            View::Status => "System overview".into(),
            View::Network => "Network interfaces".into(),
            View::Services => "Active system services".into(),
            View::Logs => "Recent system events".into(),
            View::Help => "Quick guide".into(),
            View::Resources => "System resources".into(),
            View::Assets => "Assets · home directory".into(),
            View::Settings => "System settings".into(),
            View::Updates => "Update information".into(),
            View::Terminal => "Linux command session · PTY".into(),
        };
    }

    fn apply(&mut self, action: Action) -> Option<Exit> {
        self.input.clear();
        self.highlighted = 0;
        match action {
            Action::Home => self.navigate(View::Home),
            Action::Status => self.navigate(View::Status),
            Action::Resources => self.navigate(View::Resources),
            Action::Assets => {
                self.assets.refresh().ok();
                self.navigate(View::Assets);
            },
            Action::Settings => self.navigate(View::Settings),
            Action::Updates => self.navigate(View::Updates),
            Action::Network => self.navigate(View::Network),
            Action::Services => self.navigate(View::Services),
            Action::Logs => self.navigate(View::Logs),
            Action::Help => self.navigate(View::Help),
            Action::Shell => return Some(Exit::Shell),
            Action::Logout => return Some(Exit::Logout),
            Action::Reboot | Action::Shutdown => {
                self.confirm = Some(action);
                self.notice = "Confirm power action: y to proceed, Enter to cancel".into();
            }
        }
        None
    }

    fn run_native_command(&mut self, command: String) -> io::Result<()> {
        if command.trim() == "clear" || command.trim() == "/clear" {
            self.terminal_output.clear();
            self.content.clear();
            self.scroll = 0;
            self.view = View::Terminal;
            self.notice = "Terminal output cleared".into();
            if command.trim() == "/clear" {
                return Ok(());
            }
        }
        if self.session.is_none() {
            let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
            self.session = Some(pty::Session::start(rows.saturating_sub(12), cols.saturating_sub(4))?);
        }
        if let Some(shell) = &mut self.session {
            shell.command(&command)?;
        }
        self.view = View::Terminal;
        self.content = self.terminal_output.clone();
        self.scroll = 0;
        self.follow_tail = true;
        self.notice = "Linux PTY session · /home returns to Home".into();
        Ok(())
    }

    fn key(&mut self, key: KeyEvent) -> io::Result<Option<Exit>> {
        if key.kind != KeyEventKind::Press {
            return Ok(None);
        }
        if let Some(action) = self.confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm = None;
                    let operation = if action == Action::Reboot {
                        "reboot"
                    } else {
                        "poweroff"
                    };
                    let outcome = Command::new("/usr/bin/systemctl").arg(operation).status()?;
                    if outcome.success() {
                        return Ok(Some(Exit::Power));
                    }
                    self.notice = "Permission denied. Power request was not accepted.".into();
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.confirm = None;
                    self.notice = "Cancelled. Your server is still running.".into();
                }
                _ => {}
            }
            return Ok(None);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('t') {
                return Ok(Some(Exit::Shell));
            }
            if key.code == KeyCode::Char('l') {
                self.terminal_output.clear();
                if self.view == View::Terminal { self.content.clear(); self.scroll = 0; }
                return Ok(None);
            }
            if key.code == KeyCode::Char('c') {
                if self.view == View::Terminal && let Some(shell) = &mut self.session {
                    shell.key(3)?;
                }
                return Ok(None);
            }
        }
        if self.view == View::Assets && self.input.is_empty() {
            match key.code {
                KeyCode::Up => {
                    self.assets.select_previous();
                    self.content = self.assets.display();
                    return Ok(None);
                }
                KeyCode::Down => {
                    self.assets.select_next();
                    self.content = self.assets.display();
                    return Ok(None);
                }
                KeyCode::Enter => {
                    match self.assets.activate() {
                        Ok(Some(path)) => self.notice = format!("File selected: {}", path.display()),
                        Ok(None) => self.notice = "Folder opened".into(),
                        Err(error) => self.notice = format!("Cannot open: {error}"),
                    }
                    self.content = self.assets.display();
                    return Ok(None);
                }
                KeyCode::Backspace => {
                    if let Err(error) = self.assets.parent() {
                        self.notice = format!("Cannot navigate: {error}");
                    }
                    self.content = self.assets.display();
                    return Ok(None);
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.input.chars().count() < 100 {
                    self.input.push(c);
                    self.highlighted = 0;
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.highlighted = 0;
            }
            KeyCode::Esc => {
                self.input.clear();
                self.highlighted = 0;
                self.notice = "Ready".into();
            }
            KeyCode::Up => {
                self.highlighted = self.highlighted.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Tab => {
                let total = self.suggestions().len();
                if total > 0 {
                    self.highlighted = (self.highlighted + 1).min(total - 1);
                }
            }
            KeyCode::Enter => {
                if self.input.is_empty() {
                    return Ok(None);
                }
                let waiting_for_secret = self.session.as_ref().is_some_and(|shell| !shell.echo_enabled());
                let action = if waiting_for_secret { None } else { self
                    .suggestions()
                    .get(self.highlighted)
                    .map(|item| item.0)
                    .or_else(|| action_for(&self.input)) };
                if let Some(path) = self.input.strip_prefix("/assets/") {
                    let path = std::path::PathBuf::from(format!("/{path}"));
                    let result = self.assets.open(&path);
                    self.input.clear();
                    match result {
                        Ok(()) => {
                            self.navigate(View::Assets);
                            self.notice = format!("Assets · {}", self.assets.path.display());
                        }
                        Err(error) => self.notice = format!("Cannot open Assets path: {error}"),
                    }
                    return Ok(None);
                }
                if let Some(action) = action {
                    return Ok(self.apply(action));
                }
                let command = std::mem::take(&mut self.input);
                if let Err(error) = self.run_native_command(command) {
                    self.notice = format!("Unable to start Linux command: {error}");
                }
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(7);
                self.follow_tail = false;
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(7);
                self.follow_tail = false;
            }
            _ => {}
        }
        Ok(None)
    }
}

fn system_output(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(output) => {
            let raw = if output.status.success() {
                &output.stdout
            } else {
                &output.stderr
            };
            let value = String::from_utf8_lossy(raw);
            value
                .chars()
                .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
                .take(8000)
                .collect()
        }
        Err(error) => format!("Unable to read system information: {error}"),
    }
}

fn read_mem_field(key: &str) -> Option<u64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    text.lines()
        .find(|line| line.starts_with(key))?
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
}

fn usage_bar(used: u64, total: u64, width: usize) -> String {
    if total == 0 { return "Not configured".into(); }
    let filled = ((used.min(total) as u128 * width as u128) / total as u128) as usize;
    let percentage = ((used.min(total) as u128 * 100) / total as u128) as u64;
    format!("[{}{}] {percentage:>3}%", "█".repeat(filled), "░".repeat(width - filled))
}

fn resources() -> String {
    let total = read_mem_field("MemTotal:").unwrap_or(0) / 1024;
    let available = read_mem_field("MemAvailable:").unwrap_or(0) / 1024;
    let swap_total = read_mem_field("SwapTotal:").unwrap_or(0) / 1024;
    let swap_free = read_mem_field("SwapFree:").unwrap_or(0) / 1024;
    let cpu = fs::read_to_string("/proc/stat")
        .ok()
        .map(|s| s.lines().filter(|l| l.starts_with("cpu") && l.chars().nth(3).is_some_and(|c| c.is_ascii_digit())).count())
        .unwrap_or(0);
    let load = fs::read_to_string("/proc/loadavg")
        .ok()
        .map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join("  "))
        .unwrap_or_else(|| "Unavailable".into());
    let memory_used = total.saturating_sub(available);
    let swap_used = swap_total.saturating_sub(swap_free);
    let storage = system_output("/usr/bin/df", &["-k", "-P", "/"]);
    let disk = storage.lines().nth(1)
        .and_then(|line| {
            let values: Vec<_> = line.split_whitespace().collect();
            let capacity = values.get(1)?.parse::<u64>().ok()? / 1024;
            let used = values.get(2)?.parse::<u64>().ok()? / 1024;
            Some((used, capacity))
        });
    let mut output = format!(
        "SYSTEM RESOURCES\n\nCPU  ·  {cpu} logical processors\nLoad average (1 / 5 / 15 min)\n{load}\n\nMEMORY\n{}\n{memory_used} / {total} MiB used    {available} MiB available\n\nSWAP\n{}\n{swap_used} / {swap_total} MiB used\n\n",
        usage_bar(memory_used, total, 28),
        usage_bar(swap_used, swap_total, 28)
    );
    if let Some((used, capacity)) = disk {
        output.push_str(&format!(
            "STORAGE  ·  Root filesystem\n{}\n{used} / {capacity} MiB used\n",
            usage_bar(used, capacity, 28),
        ));
    } else {
        output.push_str("STORAGE\nUsage unavailable\n");
    }
    output.push_str("\nLive memory and disk usage refresh every two seconds. CPU load is not CPU utilization.");
    output
}

fn assets() -> String {
    let home = env::var("HOME").unwrap_or_else(|_| "/".into());
    let root = std::path::Path::new(&home);
    let mut output = format!("HOME  {home}\n\n");
    let Ok(entries) = fs::read_dir(root) else {
        return format!("{output}Unable to read this directory.");
    };
    let mut names: Vec<(bool, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let kind = entry.file_type().ok()?;
            let name: String = entry.file_name().to_string_lossy().chars().filter(|c| !c.is_control()).collect();
            Some((kind.is_dir(), name))
        })
        .collect();
    names.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    for (directory, name) in names.iter().take(120) {
        output.push_str(if *directory { "▸  " } else { "   " });
        output.push_str(name);
        output.push('\n');
    }
    if names.len() > 120 {
        output.push_str("\nMore entries available through /shell.");
    }
    output.push_str("\nRead-only preview. File operations will require explicit confirmation.");
    output
}

fn settings() -> String {
    let timezone = fs::read_to_string("/etc/timezone")
        .unwrap_or_else(|_| "System default".into());
    format!(
        "ABOUT\nLinos {} Server\nMarauder · Sequence 9\nHostname               {}\n\nPREFERENCES\nTime zone              {}\nLanguage               {}\n\nSECURITY\nAuthentication         Local PAM\nCloud account          Optional\n\nSettings are currently read-only. Administrative changes will require explicit confirmation.",
        version(), host(), timezone.trim(), env::var("LANG").unwrap_or_else(|_| "Unset".into()),
    )
}

fn updates() -> String {
    let version = system_output("/usr/bin/dpkg-query", &["-W", "-f=${Version}", "linos"]);
    format!(
        "LINOS UPDATES\nInstalled package      linos {}\n\nUpdate channel         Development\nAutomatic OS updates   Disabled (not configured)\nRollback               Not implemented\n\nSystem packages currently use Debian apt. No update was started.\nA verified update mechanism and rollback policy are required before automatic Linos updates can be enabled.",
        version.trim(),
    )
}

fn paragraph_at(frame: &mut Frame, rect: Rect, text: Vec<Line<'_>>) {
    frame.render_widget(Paragraph::new(text), rect);
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 36 || area.height < 14 {
        frame.render_widget(
            Paragraph::new("Linos 13 Server\n\nResize the terminal for the interactive workspace.\nCtrl+C: shell"),
            area,
        );
        return;
    }

    // Keep the main workspace centred on wide terminals, like a modern CLI.
    let width = area.width.saturating_sub(if area.width > 100 { 8 } else { 2 });
    let x = area.x + (area.width - width) / 2;
    let top = area.y + 1;
    let composer_y = area.bottom().saturating_sub(5);
    let left = x + 2;
    let inner_width = width.saturating_sub(4);

    paragraph_at(frame, Rect::new(x, top, width, 1), vec![
        Line::from(vec![
            Span::styled("◆  LINOS", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled("  13  /  SERVER", Style::default().fg(DIM)),
            Span::styled(format!("    •    {}@{}", app.user, host()), Style::default().fg(DIM)),
        ]),
    ]);

    if app.view == View::Home {
        let hero_y = top + 3;
        paragraph_at(frame, Rect::new(left, hero_y, inner_width, 2), vec![
            Line::styled(format!("Welcome back, {}.", app.user), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Line::styled("Your server is ready. What would you like to do?", Style::default().fg(DIM)),
        ]);

        if composer_y > hero_y + 11 {
            let status_y = hero_y + 4;
            frame.render_widget(
                Block::default().title("  SERVER OVERVIEW  ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
                Rect::new(x, status_y, width, 6),
            );
            paragraph_at(frame, Rect::new(left, status_y + 1, inner_width, 4), vec![
                Line::from(vec![
                    Span::styled("Device       ", Style::default().fg(DIM)),
                    Span::raw(host()),
                ]),
                Line::from(vec![
                    Span::styled("Uptime       ", Style::default().fg(DIM)),
                    Span::raw(uptime().unwrap_or_else(|| "Unavailable".into())),
                ]),
                Line::from(vec![
                    Span::styled("Memory       ", Style::default().fg(DIM)),
                    Span::raw(memory().unwrap_or_else(|| "Unavailable".into())),
                ]),
                Line::from(vec![
                    Span::styled("Setup        ", Style::default().fg(DIM)),
                    Span::raw(ownership()),
                ]),
            ]);

            if composer_y > status_y + 9 {
                paragraph_at(frame, Rect::new(left, status_y + 7, inner_width, 2), vec![
                    Line::styled("EXPLORE YOUR SERVER", Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
                    Line::from(vec![
                        Span::styled("/status  ", Style::default().fg(ACCENT)),
                        Span::styled("Overview    ", Style::default().fg(DIM)),
                        Span::styled("/network  ", Style::default().fg(ACCENT)),
                        Span::styled("Network    ", Style::default().fg(DIM)),
                        Span::styled("/services  ", Style::default().fg(ACCENT)),
                        Span::styled("Services", Style::default().fg(DIM)),
                    ]),
                ]);
            }
        }
    } else {
        let title = match app.view {
            View::Home => "Home",
            View::Status => "System overview",
            View::Network => "Network interfaces",
            View::Services => "Running services",
            View::Logs => "System logs",
            View::Help => "Help and shortcuts",
            View::Resources => "System resources",
            View::Assets => "Assets",
            View::Settings => "Settings",
            View::Updates => "Updates",
            View::Terminal => "Terminal",
        };
        paragraph_at(frame, Rect::new(left, top + 3, inner_width, 1), vec![
            Line::styled(title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]);
    }

    let waiting_for_secret = app.session.as_ref().is_some_and(|shell| !shell.echo_enabled());
    let suggestions = if waiting_for_secret { Vec::new() } else { app.suggestions() };
    let available_suggestions = composer_y.saturating_sub(top + 8) as usize;
    let count = suggestions.len().min(5).min(available_suggestions);
    let suggestion_y = composer_y.saturating_sub(count as u16 + 1);
    if app.view != View::Home && suggestion_y > top + 6 {
        frame.render_widget(
            Paragraph::new(app.content.as_str())
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0))
                .style(Style::default().fg(Color::White)),
            Rect::new(left, top + 6, inner_width, suggestion_y - (top + 6)),
        );
    }

    for (index, (_, command, description)) in suggestions.iter().take(count).enumerate() {
        let active = index == app.highlighted.min(count - 1);
        paragraph_at(frame, Rect::new(left, suggestion_y + index as u16, inner_width, 1), vec![
            Line::from(vec![
                Span::styled(
                    format!("{} {command:<12}", if active { "❯" } else { " " }),
                    Style::default().fg(if active { ACCENT } else { DIM }),
                ),
                Span::styled(*description, Style::default().fg(if active { Color::White } else { DIM })),
            ]),
        ]);
    }

    paragraph_at(
        frame,
        Rect::new(left, composer_y.saturating_sub(1), inner_width, 1),
        vec![Line::styled(
            format!("  {}  {}", if app.view == View::Assets { "ASSETS" } else { "DIRECTORY" },
                if app.view == View::Assets { app.assets.path.display().to_string() } else { app.cwd.display().to_string() }),
            Style::default().fg(DIM),
        )],
    );
    frame.render_widget(
        Block::default().title("  COMMAND  ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if app.input.is_empty() { Color::DarkGray } else { ACCENT })),
        Rect::new(x, composer_y, width, 3),
    );
    let display_input = if waiting_for_secret {
        "•".repeat(app.input.chars().count())
    } else {
        app.input.clone()
    };
    paragraph_at(frame, Rect::new(left, composer_y + 1, inner_width, 1), vec![
        Line::from(vec![
            Span::styled("❯ ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(
                if app.input.is_empty() { if waiting_for_secret { "Password (hidden)" } else { "Type / for commands or enter a Linux command" } } else { &display_input },
                Style::default().fg(if app.input.is_empty() { DIM } else { Color::White }),
            ),
        ]),
    ]);
    // The cursor follows the user input rather than showing a static cursor glyph.
    let cursor_x = left.saturating_add(2).saturating_add(app.input.chars().count() as u16)
        .min(x + width.saturating_sub(2));
    frame.set_cursor_position((cursor_x, composer_y + 1));

    let footer = if let Some(action) = app.confirm {
        if action == Action::Reboot {
            "Restart server?  Y Confirm · Enter Cancel"
        } else {
            "Shut down server?  Y Confirm · Enter Cancel"
        }
    } else {
        &app.notice
    };
    paragraph_at(frame, Rect::new(x, area.bottom().saturating_sub(1), width, 1), vec![
        Line::from(vec![
            Span::styled(format!("◆  {footer}"), Style::default().fg(DIM)),
            Span::styled("    / Commands · Ctrl+T Shell · Ctrl+R Fit", Style::default().fg(DIM)),
        ]),
    ]);
}

fn interactive_shell() -> io::Result<()> {
    // The Ratatui terminal has already been restored before this child starts.
    // Run the user's genuine interactive shell with normal shell behaviour:
    // cd, sudo, jobs, pipes, SSH and nested TUIs all work normally.
    let shell = env::var("SHELL")
        .ok()
        .filter(|path| path.starts_with('/') && std::path::Path::new(path).is_file())
        .unwrap_or_else(|| "/bin/sh".into());
    let bash = shell.ends_with("/bash")
        && std::path::Path::new("/usr/lib/linos/linos-shell.bashrc").is_file();
    let mut child = Command::new(shell);
    if bash {
        child.args(["--rcfile", "/usr/lib/linos/linos-shell.bashrc"]);
    }
    let status = child.arg("-i").env("LINOS_NO_WELCOME", "1").status()?;
    if !status.success() {
        eprintln!("Shell exited with status {status}");
    }
    Ok(())
}

pub fn run() -> io::Result<ExitCode> {
    let mut app = App::new();
    loop {
        // Ratatui owns the terminal only while Linos Home is active. Leaving
        // this scope restores the real terminal before entering a child shell.
        let result = ratatui::run(|terminal| {
            let mut last_refresh = Instant::now();
            let mut last_geometry = Instant::now();
            let serial_console = geometry::serial_console();
            let mut dirty = true;
            loop {
                if serial_console
                    && last_geometry.elapsed() >= Duration::from_secs(4)
                    && app.input.is_empty()
                {
                    if geometry::negotiate().unwrap_or(false) {
                        terminal.autoresize()?;
                        dirty = true;
                    }
                    last_geometry = Instant::now();
                }
                if app.view == View::Resources && last_refresh.elapsed() >= Duration::from_secs(2) {
                    app.content = resources();
                    last_refresh = Instant::now();
                    dirty = true;
                }
                if let Some(shell) = &mut app.session {
                    if let Some(cwd) = shell.current_dir() {
                        if app.cwd != cwd {
                            app.cwd = cwd;
                            dirty = true;
                        }
                    }
                    let previous = app.terminal_output.len();
                    shell.drain(&mut app.terminal_output);
                    if previous != app.terminal_output.len() && app.view == View::Terminal {
                        app.content.clone_from(&app.terminal_output);
                        if app.follow_tail {
                            let height = crossterm::terminal::size().map(|(_, h)| h.saturating_sub(14)).unwrap_or(10);
                            app.scroll = app.content.lines().count().saturating_sub(usize::from(height)).min(usize::from(u16::MAX)) as u16;
                        }
                        dirty = true;
                    }
                }
                if dirty {
                    terminal.draw(|frame| draw(frame, &app))?;
                    dirty = false;
                }
                if event::poll(Duration::from_millis(100))? {
                    match event::read()? {
                        Event::Key(key) => {
                            if key.kind == KeyEventKind::Press
                                && key.code == KeyCode::Char('r')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                app.notice = if geometry::negotiate()? {
                                    "Terminal geometry refreshed".into()
                                } else {
                                    "Terminal does not support size reporting; use stty rows/cols".into()
                                };
                                terminal.autoresize()?;
                                dirty = true;
                                continue;
                            }
                            if let Some(exit) = app.key(key)? {
                                break Ok::<Exit, io::Error>(exit);
                            }
                            dirty = true;
                        }
                        Event::Resize(width, height) => {
                            if let Some(shell) = &mut app.session {
                                shell.resize(height.saturating_sub(12), width.saturating_sub(4))?;
                            }
                            dirty = true;
                        }
                        _ => {}
                    }
                }
            }
        })?;
        match result {
            Exit::Shell => {
                // Exiting the nested shell (exit/Ctrl+D) returns here rather
                // than terminating the Linos Home session.
                interactive_shell()?;
                app.navigate(View::Home);
            }
            Exit::Power => return Ok(ExitCode::SUCCESS),
            Exit::Logout => return Ok(ExitCode::from(20)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn is_responsive_at_common_terminal_sizes() {
        for (width, height) in [(30, 10), (48, 18), (80, 24), (120, 32), (160, 42)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &App::new())).unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol().to_owned())
                .collect();
            assert!(screen.contains("LINOS") || screen.contains("Linos 13"));
            assert!(screen.contains("Type / for commands") || width < 36 || height < 14);
        }
    }

    #[test]
    fn new_management_views_are_reachable() {
        let mut app = App::new();
        for (action, expected) in [
            (Action::Resources, View::Resources),
            (Action::Assets, View::Assets),
            (Action::Settings, View::Settings),
            (Action::Updates, View::Updates),
        ] {
            assert!(app.apply(action).is_none());
            assert_eq!(app.view, expected);
        }
    }

    #[test]
    fn palette_filters_and_executes_commands() {
        let mut app = App::new();
        app.key(press(KeyCode::Char('/'))).unwrap();
        app.key(press(KeyCode::Char('s'))).unwrap();
        assert_eq!(app.suggestions().len(), 5); // status, services, settings, shell, shutdown
        app.key(press(KeyCode::Char('t'))).unwrap();
        app.key(press(KeyCode::Enter)).unwrap();
        assert_eq!(app.view, View::Status);
    }

    #[test]
    fn password_and_shell_are_not_captured_by_the_dashboard() {
        let mut app = App::new();
        for c in "/shell".chars() {
            app.key(press(KeyCode::Char(c))).unwrap();
        }
        assert_eq!(app.key(press(KeyCode::Enter)).unwrap(), Some(Exit::Shell));
    }

    #[test]
    fn shell_switch_is_a_temporary_session_mode() {
        let mut app = App::new();
        assert_eq!(app.apply(Action::Shell), Some(Exit::Shell));
        assert_eq!(app.view, View::Home);
        let shortcut = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL);
        assert_eq!(app.key(shortcut).unwrap(), Some(Exit::Shell));
    }

    #[test]
    fn resource_bars_handle_zero_and_percentages() {
        assert!(usage_bar(0, 0, 12).contains("Not configured"));
        assert!(usage_bar(5, 10, 10).contains("50%"));
        assert!(usage_bar(15, 10, 10).contains("100%"));
    }

    #[test]
    fn ctrl_l_clears_output_without_exiting_home() {
        let mut app = App::new();
        app.view = View::Terminal;
        app.terminal_output = "previous command output".into();
        app.content = app.terminal_output.clone();
        let key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        assert_eq!(app.key(key).unwrap(), None);
        assert!(app.content.is_empty());
        assert!(app.terminal_output.is_empty());
    }

    #[test]
    fn power_requests_are_cancelled_by_default() {
        let mut app = App::new();
        app.apply(Action::Shutdown);
        assert_eq!(app.confirm, Some(Action::Shutdown));
        app.key(press(KeyCode::Enter)).unwrap();
        assert!(app.confirm.is_none());
    }
}
