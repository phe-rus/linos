//! A composer-first Linos Server terminal experience.
//! Deliberately sparse: the workspace expands, command entry stays at the bottom.
use std::{
    env, io,
    process::{Command, ExitCode},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{Action, action_for, host, memory, ownership, uptime, version};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::Gray;
const COMMANDS: &[(Action, &str, &str)] = &[
    (
        Action::Status,
        "/status",
        "Show the health and identity of this server",
    ),
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
    (Action::Shell, "/shell", "Open a normal Linux shell"),
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
                "/status       Overview and device health",
                "/network      Network interfaces",
                "/services     Running services",
                "/logs         Recent system events",
                "/help         This guide",
                "/shell        Return to your shell",
                "/logout       Sign out",
                "/reboot       Restart (requires confirmation)",
                "/shutdown     Power off (requires confirmation)",
                "",
                "Type / to discover commands.",
                "Use Up/Down to select, Enter to run, Esc to clear.",
                "PageUp/PageDown scroll results. Ctrl+C returns to shell.",
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
        };
    }

    fn apply(&mut self, action: Action) -> Option<Exit> {
        self.input.clear();
        self.highlighted = 0;
        match action {
            Action::Status => self.navigate(View::Status),
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
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(Some(Exit::Shell));
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
                let action = self
                    .suggestions()
                    .get(self.highlighted)
                    .map(|item| item.0)
                    .or_else(|| action_for(&self.input));
                if let Some(action) = action {
                    return Ok(self.apply(action));
                }
                self.notice = "Unknown command. Type / to see available actions.".into();
                self.input.clear();
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(7),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(7),
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
    let width = area.width.saturating_sub(4).min(94);
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
        };
        paragraph_at(frame, Rect::new(left, top + 3, inner_width, 1), vec![
            Line::styled(title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]);
    }

    let suggestions = app.suggestions();
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

    frame.render_widget(
        Block::default().title("  COMMAND  ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if app.input.is_empty() { Color::DarkGray } else { ACCENT })),
        Rect::new(x, composer_y, width, 3),
    );
    paragraph_at(frame, Rect::new(left, composer_y + 1, inner_width, 1), vec![
        Line::from(vec![
            Span::styled("❯ ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(
                if app.input.is_empty() { "Type / for commands" } else { &app.input },
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
            Span::styled("    / Commands · ↑↓ Select · Ctrl+C Shell", Style::default().fg(DIM)),
        ]),
    ]);
}

pub fn run() -> io::Result<ExitCode> {
    let result = ratatui::run(|terminal| {
        let mut app = App::new();
        loop {
            terminal.draw(|frame| draw(frame, &app))?;
            if event::poll(Duration::from_millis(140))?
                && let Event::Key(key) = event::read()?
                && let Some(exit) = app.key(key)?
            {
                break Ok::<Exit, io::Error>(exit);
            }
        }
    })?;
    Ok(match result {
        Exit::Shell | Exit::Power => ExitCode::SUCCESS,
        Exit::Logout => ExitCode::from(20),
    })
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
        for (width, height) in [(30, 10), (48, 18), (80, 24), (120, 32)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &App::new())).unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol().to_owned())
                .collect();
            assert!(screen.contains("Linos 13") || width < 24);
            assert!(screen.contains("Type / for commands") || width < 24);
        }
    }

    #[test]
    fn palette_filters_and_executes_commands() {
        let mut app = App::new();
        app.key(press(KeyCode::Char('/'))).unwrap();
        app.key(press(KeyCode::Char('s'))).unwrap();
        assert_eq!(app.suggestions().len(), 4); // status, services, shell, shutdown
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
    fn power_requests_are_cancelled_by_default() {
        let mut app = App::new();
        app.apply(Action::Shutdown);
        assert_eq!(app.confirm, Some(Action::Shutdown));
        app.key(press(KeyCode::Enter)).unwrap();
        assert!(app.confirm.is_none());
    }
}
