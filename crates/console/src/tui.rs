//! Linos Server dashboard, built on Ratatui.
//! Runs only on an interactive TTY and returns terminal ownership on exit.
use std::{
    env, io,
    process::{Command, ExitCode},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use super::{Action, action_for, host, memory, overview, uptime, version};

const SKY: Color = Color::Cyan;
const MUTED: Color = Color::Gray;
const ACTIONS: &[(Action, &str, &str)] = &[
    (Action::Status, "Overview", "/status"),
    (Action::Network, "Network", "/network"),
    (Action::Services, "Services", "/services"),
    (Action::Logs, "System logs", "/logs"),
    (Action::Help, "Shortcuts", "/help"),
    (Action::Shell, "Return to shell", "/shell"),
    (Action::Logout, "Sign out", "/logout"),
    (Action::Reboot, "Restart", "/reboot"),
    (Action::Shutdown, "Shut down", "/shutdown"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Overview,
    Network,
    Services,
    Logs,
    Help,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
    Shell,
    Logout,
    Power,
}

struct App {
    screen: Screen,
    selected: usize,
    palette: Option<String>,
    confirm: Option<Action>,
    info: String,
    scroll: u16,
    user: String,
}

impl App {
    fn new() -> Self {
        Self {
            screen: Screen::Overview,
            selected: 0,
            palette: None,
            confirm: None,
            info: String::from("Ready  ·  Press / for commands  ·  Tab to navigate"),
            scroll: 0,
            user: env::var("USER").unwrap_or_else(|_| "operator".to_owned()),
        }
    }

    fn view(&mut self, screen: Screen) {
        self.screen = screen;
        self.scroll = 0;
        self.info = match screen {
            Screen::Overview => "System overview".into(),
            Screen::Network => "Read-only network interfaces".into(),
            Screen::Services => "Running systemd services".into(),
            Screen::Logs => "Recent system journal entries".into(),
            Screen::Help => "Keyboard shortcuts and slash commands".into(),
        };
    }

    fn action(&mut self, action: Action) -> io::Result<Option<Exit>> {
        self.palette = None;
        match action {
            Action::Status => self.view(Screen::Overview),
            Action::Network => self.view(Screen::Network),
            Action::Services => self.view(Screen::Services),
            Action::Logs => self.view(Screen::Logs),
            Action::Help => self.view(Screen::Help),
            Action::Shell => return Ok(Some(Exit::Shell)),
            Action::Logout => return Ok(Some(Exit::Logout)),
            Action::Reboot | Action::Shutdown => self.confirm = Some(action),
        }
        Ok(None)
    }

    fn key(&mut self, key: KeyEvent) -> io::Result<Option<Exit>> {
        if key.kind != KeyEventKind::Press {
            return Ok(None);
        }

        if let Some(action) = self.confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.confirm = None;
                    let operation = if action == Action::Reboot { "reboot" } else { "poweroff" };
                    let result = Command::new("/usr/bin/systemctl").arg(operation).status()?;
                    if result.success() {
                        return Ok(Some(Exit::Power));
                    }
                    self.info = format!("Unable to {operation}. Check authorization.");
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.confirm = None;
                    self.info = "Power operation cancelled".into();
                }
                _ => {}
            }
            return Ok(None);
        }

        if let Some(input) = self.palette.as_mut() {
            match key.code {
                KeyCode::Esc => self.palette = None,
                KeyCode::Backspace => { input.pop(); }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if input.len() < 64 { input.push(c); }
                }
                KeyCode::Enter => {
                    let entered = self.palette.take().unwrap_or_default();
                    if let Some(action) = action_for(&entered) {
                        return self.action(action);
                    }
                    self.info = format!("Unknown command: {entered}. Press / to try again.");
                }
                _ => {}
            }
            return Ok(None);
        }

        match key.code {
            KeyCode::Char('/') => self.palette = Some("/".into()),
            KeyCode::Up | KeyCode::BackTab => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1).min(ACTIONS.len() - 1),
            KeyCode::Enter => return self.action(ACTIONS[self.selected].0),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(8),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(8),
            KeyCode::Esc => return Ok(Some(Exit::Shell)),
            KeyCode::Char('q') if key.modifiers.is_empty() => return Ok(Some(Exit::Shell)),
            KeyCode::Char('1') => self.view(Screen::Overview),
            KeyCode::Char('2') => self.view(Screen::Network),
            KeyCode::Char('3') => self.view(Screen::Services),
            KeyCode::Char('4') => self.view(Screen::Logs),
            KeyCode::Char('5') => self.view(Screen::Help),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Exit::Shell));
            }
            _ => {}
        }
        Ok(None)
    }
}

fn clean(raw: &[u8]) -> String {
    let output = String::from_utf8_lossy(raw);
    output.chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .take(9000)
        .collect()
}

fn read_command(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => clean(&output.stdout),
        Ok(output) => format!("Unable to read system information\n{}", clean(&output.stderr)),
        Err(error) => format!("System command unavailable: {error}"),
    }
}

fn page_content(app: &App) -> String {
    match app.screen {
        Screen::Overview => format!(
            "Linos {} Server\n\nDevice       {}\nSession      {}\nUptime       {}\nMemory       {}\n\n{}\n\nSystem data is read-only on this page.",
            version(),
            host(),
            app.user,
            uptime().unwrap_or_else(|| "Unavailable".into()),
            memory().unwrap_or_else(|| "Unavailable".into()),
            overview()
        ),
        Screen::Network => read_command("/usr/sbin/ip", &["-brief", "address"]),
        Screen::Services => read_command("/usr/bin/systemctl", &[
            "list-units", "--type=service", "--state=running", "--no-pager", "--plain"
        ]),
        Screen::Logs => read_command("/usr/bin/journalctl", &["--no-pager", "-n", "60", "--output=short"]),
        Screen::Help => [
            "Navigation",
            "",
            "  ↑ / ↓       Move between actions",
            "  Tab         Next action",
            "  Enter       Open selected action",
            "  1 - 5       Overview, Network, Services, Logs, Help",
            "  /           Open command palette",
            "  PageUp/Down Scroll longer output",
            "  q / Esc     Return to shell",
            "",
            "Slash commands",
            "",
            "  /status     System overview",
            "  /network    Network interfaces",
            "  /services   System services",
            "  /logs       Recent journal messages",
            "  /help       This view",
            "  /shell      Return to Bash",
            "  /logout     Sign out",
            "  /reboot     Restart server (confirmation required)",
            "  /shutdown   Shut down (confirmation required)",
            "",
            "Regular system commands and third-party TUIs run",
            "normally after returning to the shell.",
        ].join("\n"),
    }
}

fn panel<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(6), Constraint::Length(3)])
        .split(area);

    let heading = Paragraph::new(Line::from(vec![
        Span::styled(" ◆  Linos 13", Style::default().fg(SKY).add_modifier(Modifier::BOLD)),
        Span::styled("  /  Server", Style::default().fg(Color::White)),
        Span::styled(format!("      {}@{}", app.user, host()), Style::default().fg(MUTED)),
    ]))
    .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(heading, vertical[0]);

    let wide = area.width >= 76;
    let middle = if wide {
        Layout::default().direction(Direction::Horizontal())
            .constraints([Constraint::Length(23), Constraint::Min(20)]).split(vertical[1])
    } else {
        Layout::default().direction(Direction::Vertical())
            .constraints([Constraint::Length(10), Constraint::Min(3)]).split(vertical[1])
    };

    let entries: Vec<ListItem<'_>> = ACTIONS.iter().map(|(_, title, _)| {
        ListItem::new(format!(" {title}"))
    }).collect();
    let list = List::new(entries)
        .block(panel("Navigate"))
        .highlight_style(Style::default().fg(SKY).add_modifier(Modifier::BOLD))
        .highlight_symbol(" ›");
    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, middle[0], &mut state);

    let title = match app.screen {
        Screen::Overview => "Overview",
        Screen::Network => "Network",
        Screen::Services => "Services",
        Screen::Logs => "Journal",
        Screen::Help => "Help",
    };
    let content = Paragraph::new(page_content(app))
        .block(panel(title))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    frame.render_widget(content, middle[1]);

    let footer = Paragraph::new(format!(
        "  {}    ↑↓ Navigate  ·  Enter Open  ·  / Commands  ·  q Shell",
        app.info
    )).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray)))
      .style(Style::default().fg(MUTED));
    frame.render_widget(footer, vertical[2]);

    if let Some(input) = &app.palette {
        let width = area.width.saturating_sub(4).min(64);
        let popup = Rect::new(area.x + (area.width.saturating_sub(width) / 2), area.y + area.height / 3, width, 5);
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new(format!("  {input}▏\n\n  Enter to run · Esc to cancel"))
            .block(panel("Command palette"))
            .style(Style::default().fg(SKY)), popup);
    }

    if let Some(action) = app.confirm {
        let message = if action == Action::Reboot { "Restart" } else { "Shut down" };
        let width = area.width.saturating_sub(4).min(58);
        let popup = Rect::new(area.x + (area.width.saturating_sub(width) / 2), area.y + area.height / 3, width, 7);
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new(format!(
            "\n  {message} {}?\n\n  This disconnects active sessions.\n  Enter/Y Confirm    N/Esc Cancel", host()
        )).block(panel("Confirm power action"))
        .style(Style::default().fg(Color::Yellow)), popup);
    }
}

pub fn run() -> io::Result<ExitCode> {
    let result = ratatui::run(|mut terminal| {
        let mut app = App::new();
        loop {
            terminal.draw(|frame| draw(frame, &app))?;
            if event::poll(Duration::from_millis(120))? {
                if let Event::Key(key) = event::read()? {
                    if let Some(exit) = app.key(key)? {
                        break Ok(exit);
                    }
                }
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

    #[test]
    fn view_renders_on_small_and_wide_terminals() {
        for (width, height) in [(48, 18), (80, 24), (120, 30)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let app = App::new();
            terminal.draw(|frame| draw(frame, &app)).unwrap();
        }
    }

    #[test]
    fn actions_do_not_execute_power_without_confirmation() {
        let mut app = App::new();
        assert!(app.action(Action::Shutdown).unwrap().is_none());
        assert_eq!(app.confirm, Some(Action::Shutdown));
        assert!(app.action(Action::Shell).unwrap().is_some());
    }

    #[test]
    fn sanitizes_output_from_system_commands() {
        assert_eq!(clean(b"hello\x1b[31m world\n"), "hello[31m world\n");
    }
}
