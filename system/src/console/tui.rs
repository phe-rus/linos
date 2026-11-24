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
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::{
    Action, action_for, assets::Browser, geometry, hardware, health, host, memory, monitor,
    ownership, preferences, pty, query, store, uptime, version,
};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::Gray;
const COMMANDS: &[(Action, &str, &str)] = &[
    (Action::Home, "/home", "Return to the Linos Home workspace"),
    (
        Action::Status,
        "/status",
        "Show the health and identity of this server",
    ),
    (
        Action::Resources,
        "/resources",
        "CPU, memory, swap and storage",
    ),
    (Action::Assets, "/assets", "Browse your home directory"),
    (Action::Tasks, "/tasks", "Live processes and task controls"),
    (Action::Settings, "/settings", "System settings and About"),
    (
        Action::Updates,
        "/updates",
        "Update policy and package status",
    ),
    (
        Action::Hardware,
        "/hardware",
        "Inspect detected hardware and capabilities",
    ),
    (
        Action::Store,
        "/store",
        "Browse the Linus store package inventory",
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
    (
        Action::Shell,
        "/shell",
        "Switch to the interactive Linux shell",
    ),
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
    Hardware,
    Store,
    Tasks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exit {
    Shell,
    Logout,
    Power,
}

fn confirms_action(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && matches!(key.code, KeyCode::Char('y' | 'Y'))
        && !key.modifiers.intersects(
            KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SUPER
                | KeyModifiers::HYPER
                | KeyModifiers::META,
        )
}

struct ViewJob {
    receiver: std::sync::mpsc::Receiver<String>,
    cancel: query::Cancellation,
}
impl Drop for ViewJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct App {
    input: String,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    history_draft: String,
    preferences: preferences::Preferences,
    monitor: monitor::Monitor,
    task_selected: usize,
    task_confirmation: Option<(monitor::Task, String)>,
    setting_confirmation: Option<(String, String)>,
    native_command: Option<String>,
    view_job: Option<ViewJob>,
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
    command_pending: bool,
    assets: Browser,
    cwd: std::path::PathBuf,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            preferences: preferences::Preferences::load(),
            monitor: monitor::Monitor::new(),
            task_selected: 0,
            task_confirmation: None,
            setting_confirmation: None,
            native_command: None,
            view_job: None,
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
            command_pending: false,
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
        self.view_job = None;
        self.scroll = 0;
        self.content = match view {
            View::Home => String::new(),
            View::Status => self.load_content(|cancel| {
                format!("{}\n\n{}", super::overview(), health::report(cancel))
            }),
            View::Resources => {
                if self.monitor.tasks.is_empty() {
                    self.monitor.sample();
                }
                self.monitor.resources()
            }
            View::Tasks => {
                self.monitor.sample();
                self.monitor.display(self.task_selected, self.task_rows())
            }
            View::Assets => self.assets.display(),
            View::Settings => self.settings(),
            View::Updates => self.load_content(updates),
            View::Hardware => self.load_content(|_| hardware::report()),
            View::Store => self.load_content(store::overview),
            View::Terminal => self.terminal_output.clone(),
            View::Network => self.load_content(|cancel| {
                system_output("/usr/sbin/ip", &["-brief", "address"], cancel)
            }),
            View::Services => self.load_content(|cancel| {
                system_output(
                    "/usr/bin/systemctl",
                    &[
                        "list-units",
                        "--type=service",
                        "--state=running",
                        "--no-pager",
                        "--plain",
                    ],
                    cancel,
                )
            }),
            View::Logs => self.load_content(|cancel| {
                system_output(
                    "/usr/bin/journalctl",
                    &["--no-pager", "-n", "50", "--output=short"],
                    cancel,
                )
            }),
            View::Help => [
                "Linos Server commands",
                "",
                "/home         Return to the Home workspace",
                "clear          Clear PTY output without resetting shell state",
                "/status       Overview and device health",
                "/resources    CPU, memory, swap and filesystem capacity",
                "/tasks        Live tasks; /tasks pause|resume|terminate|kill PID",
                "/assets       Browse your home directory (read-only)",
                "/settings     Device settings and About",
                "/updates      Update policy and installed package status",
                "/hardware     Detected hardware and CPU details",
                "/store        Linus store package inventory and previews",
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
                "Left/Right, Home/End, Delete and Backspace edit commands.",
                "Up/Down recall command history. Ctrl+A/E/U/K/W edit input.",
                "Ctrl+C cancels input or interrupts a command; Ctrl+D sends EOF.",
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
            View::Tasks => "Live tasks · Up/Down select · /tasks for controls".into(),
            View::Assets => "Assets · home directory".into(),
            View::Settings => "System settings".into(),
            View::Updates => "Update information".into(),
            View::Hardware => "Hardware details".into(),
            View::Store => "Linus store".into(),
            View::Terminal => "Linux command session · PTY".into(),
        };
    }

    fn apply(&mut self, action: Action) -> Option<Exit> {
        self.input.clear();
        self.cursor = 0;
        self.highlighted = 0;
        match action {
            Action::Home => self.navigate(View::Home),
            Action::Status => self.navigate(View::Status),
            Action::Resources => self.navigate(View::Resources),
            Action::Tasks => self.navigate(View::Tasks),
            Action::Assets => {
                self.assets.refresh().ok();
                self.navigate(View::Assets);
            }
            Action::Settings => self.navigate(View::Settings),
            Action::Updates => self.navigate(View::Updates),
            Action::Hardware => self.navigate(View::Hardware),
            Action::Store => self.navigate(View::Store),
            Action::Network => self.navigate(View::Network),
            Action::Services => self.navigate(View::Services),
            Action::Logs => self.navigate(View::Logs),
            Action::Help => self.navigate(View::Help),
            Action::Shell => return Some(Exit::Shell),
            Action::Logout => return Some(Exit::Logout),
            Action::Reboot | Action::Shutdown => {
                if self.view == View::Terminal {
                    self.navigate(View::Home);
                }
                self.confirm = Some(action);
                self.notice = "Confirm power action: y to proceed, Enter to cancel".into();
            }
        }
        None
    }

    fn password_input_active(&self) -> bool {
        // An idle Bash prompt can disable ECHO while readline is active.
        // A submitted command with echo disabled owns secret input. Do not
        // depend on English prompt text, which may be localized or cleared.
        self.command_pending
            && self
                .session
                .as_ref()
                .is_some_and(|shell| !shell.is_ready() && !shell.echo_enabled())
    }

    fn task_rows(&self) -> usize {
        crossterm::terminal::size()
            .map(|(_, h)| usize::from(h.saturating_sub(24)).max(1))
            .unwrap_or(6)
    }

    fn load_content(
        &mut self,
        read: impl FnOnce(&query::Cancellation) -> String + Send + 'static,
    ) -> String {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let cancel = query::Cancellation::default();
        let worker_cancel = cancel.clone();
        std::thread::spawn(move || {
            let _ = sender.send(read(&worker_cancel));
        });
        // Each navigation owns its receiver. Late results from an older view
        // cannot overwrite the current workspace.
        self.view_job = Some(ViewJob { receiver, cancel });
        "Reading system information… You can keep typing or change views.".into()
    }

    fn poll_content(&mut self) -> bool {
        let result = self.view_job.as_ref().map(|job| job.receiver.try_recv());
        match result {
            Some(Ok(content)) => {
                self.content = content;
                self.view_job = None;
                true
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.content = "System information could not be read. Try this view again.".into();
                self.view_job = None;
                true
            }
            _ => false,
        }
    }

    fn settings(&self) -> String {
        let timezone = fs::canonicalize("/etc/localtime")
            .ok()
            .and_then(|path| {
                path.strip_prefix("/usr/share/zoneinfo")
                    .ok()
                    .map(|zone| zone.display().to_string())
            })
            .or_else(|| fs::read_to_string("/etc/timezone").ok())
            .unwrap_or_else(|| "Unavailable".into());
        format!(
            "About\nLinos {} Server, {}\nTime zone: {}\nLanguage: {}\n\nYour preferences (saved immediately)\nDefault at sign-in: {}\nColor: {}\nLive refresh: {} seconds\n\n/settings default home|shell\n/settings color auto|always|never\n/settings refresh 1..10\n\nSystem settings (confirmation and administrator authorization)\n/settings hostname NAME\n/settings timezone AREA/CITY\n\nSign-in stays protected by local PAM. No password is stored by Home.\nLanguage and keyboard configuration remain in system setup.",
            version(),
            host(),
            timezone.trim(),
            env::var("LANG").unwrap_or_else(|_| "Unset".into()),
            self.preferences.mode,
            self.preferences.color,
            self.preferences.refresh
        )
    }

    fn refresh_monitor(&mut self) {
        let selected = self
            .monitor
            .tasks
            .get(self.task_selected)
            .map(|t| (t.pid, t.start));
        self.monitor.sample();
        self.task_selected = selected
            .and_then(|identity| {
                self.monitor
                    .tasks
                    .iter()
                    .position(|t| (t.pid, t.start) == identity)
            })
            .unwrap_or(
                self.task_selected
                    .min(self.monitor.tasks.len().saturating_sub(1)),
            );
        self.content = if self.view == View::Tasks {
            self.monitor.display(self.task_selected, self.task_rows())
        } else {
            self.monitor.resources()
        };
    }

    fn paste(&mut self, text: &str) {
        if self.confirm.is_some()
            || self.task_confirmation.is_some()
            || self.setting_confirmation.is_some()
        {
            return;
        }
        self.cursor = self.cursor.min(self.input.len());
        while !self.input.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
        for c in text.chars() {
            let c = if matches!(c, '\r' | '\n' | '\t') {
                ' '
            } else {
                c
            };
            if !c.is_control() && self.input.len() + c.len_utf8() <= 4096 {
                self.input.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
        }
        self.highlighted = 0;
        self.notice = "Pasted into command field. Enter submits.".into();
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
            self.session = Some(pty::Session::start(
                rows.saturating_sub(12),
                cols.saturating_sub(4),
            )?);
        }
        if let Some(shell) = &mut self.session {
            shell.command(&command)?;
            self.command_pending = true;
        }
        self.view = View::Terminal;
        self.content = self.terminal_output.clone();
        self.scroll = 0;
        self.follow_tail = true;
        self.notice = "Linux PTY session · /home returns to Home".into();
        Ok(())
    }

    fn key(&mut self, key: KeyEvent) -> io::Result<Option<Exit>> {
        if key.kind == KeyEventKind::Release {
            return Ok(None);
        }
        // Keep cursor positions at UTF-8 character boundaries, including
        // after command submission, history recall and external test input.
        self.cursor = self.cursor.min(self.input.len());
        while !self.input.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
        // Repeated editing is useful; repeated Enter must never submit twice.
        if key.kind == KeyEventKind::Repeat && matches!(key.code, KeyCode::Enter | KeyCode::Tab) {
            return Ok(None);
        }
        if self.task_confirmation.is_some() || self.setting_confirmation.is_some() {
            match key.code {
                KeyCode::Char('y' | 'Y') if confirms_action(key) => {
                    if let Some((task, operation)) = self.task_confirmation.take() {
                        self.notice = match monitor::signal(&task, &operation) {
                            Ok(()) => format!(
                                "{operation} signal sent to {} ({}); inspect the next sample",
                                task.name, task.pid
                            ),
                            Err(error) => format!("Task action failed: {error}"),
                        };
                        self.refresh_monitor();
                    }
                    if let Some((name, value)) = self.setting_confirmation.take() {
                        match preferences::system_change(&name, &value) {
                            Ok((program, operation)) => {
                                // Validation permits only a single tool argument. The PTY
                                // preserves sudo's normal authorization and secret masking.
                                self.run_native_command(format!(
                                    "sudo -- {program} {operation} '{value}'"
                                ))?;
                            }
                            Err(error) => self.notice = error.to_string(),
                        }
                    }
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                    self.task_confirmation = None;
                    self.setting_confirmation = None;
                    self.notice = "Cancelled. No change made.".into();
                }
                _ => {}
            }
            return Ok(None);
        }
        if let Some(action) = self.confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') if confirms_action(key) => {
                    self.confirm = None;
                    let operation = if action == Action::Reboot {
                        "reboot"
                    } else {
                        "poweroff"
                    };
                    let outcome = Command::new("/usr/bin/systemctl").arg(operation).output()?;
                    if outcome.status.success() {
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
        if self.view == View::Tasks && self.input.is_empty() {
            match key.code {
                KeyCode::Up => self.task_selected = self.task_selected.saturating_sub(1),
                KeyCode::Down => {
                    self.task_selected =
                        (self.task_selected + 1).min(self.monitor.tasks.len().saturating_sub(1))
                }
                _ => {}
            }
            if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                self.content = self.monitor.display(self.task_selected, self.task_rows());
                return Ok(None);
            }
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('t') {
                return Ok(Some(Exit::Shell));
            }
            if key.code == KeyCode::Char('l') {
                self.terminal_output.clear();
                if self.view == View::Terminal {
                    self.content.clear();
                    self.scroll = 0;
                }
                return Ok(None);
            }
            if key.code == KeyCode::Char('c') {
                self.input.clear();
                self.cursor = 0;
                self.history_index = None;
                if self.view == View::Terminal
                    && let Some(shell) = &mut self.session
                {
                    shell.key(3)?;
                }
                return Ok(None);
            }
            match key.code {
                KeyCode::Char('a') => self.cursor = 0,
                KeyCode::Char('e') => self.cursor = self.input.len(),
                KeyCode::Char('u') => {
                    self.input.drain(..self.cursor);
                    self.cursor = 0;
                }
                KeyCode::Char('k') => {
                    self.input.truncate(self.cursor);
                }
                KeyCode::Char('w') => {
                    let before = &self.input[..self.cursor];
                    let start = before
                        .trim_end()
                        .rfind(char::is_whitespace)
                        .map(|i| i + before[i..].chars().next().unwrap().len_utf8())
                        .unwrap_or(0);
                    self.input.drain(start..self.cursor);
                    self.cursor = start;
                }
                KeyCode::Char('d') if self.view == View::Terminal && self.command_pending => {
                    if let Some(shell) = &mut self.session {
                        shell.key(4)?;
                    }
                }
                _ => {}
            }
            return Ok(None);
        }
        if self.view == View::Assets && self.input.is_empty() {
            match key.code {
                KeyCode::Up => {
                    self.assets.select_previous();
                    self.content = self.assets.display();
                    self.scroll = self
                        .assets
                        .selected
                        .saturating_sub(5)
                        .min(u16::MAX as usize) as u16;
                    return Ok(None);
                }
                KeyCode::Down => {
                    self.assets.select_next();
                    self.content = self.assets.display();
                    self.scroll = self
                        .assets
                        .selected
                        .saturating_sub(5)
                        .min(u16::MAX as usize) as u16;
                    return Ok(None);
                }
                KeyCode::Enter => {
                    match self.assets.activate() {
                        Ok(Some(path)) => {
                            self.notice = format!("File selected: {}", path.display())
                        }
                        Ok(None) => self.notice = "Folder opened".into(),
                        Err(error) => self.notice = format!("Cannot open: {error}"),
                    }
                    self.content = self.assets.display();
                    self.scroll = 0;
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
                if !c.is_control() && self.input.len() + c.len_utf8() <= 4096 {
                    self.input.insert(self.cursor, c);
                    self.cursor += c.len_utf8();
                    self.highlighted = 0;
                }
            }
            KeyCode::Backspace => {
                if let Some((previous, _)) = self.input[..self.cursor].char_indices().next_back() {
                    self.input.drain(previous..self.cursor);
                    self.cursor = previous;
                }
                self.highlighted = 0;
            }
            KeyCode::Delete => {
                if let Some(c) = self.input[self.cursor..].chars().next() {
                    self.input.drain(self.cursor..self.cursor + c.len_utf8());
                }
            }
            KeyCode::Left => {
                self.cursor = self.input[..self.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
            }
            KeyCode::Right => {
                self.cursor += self.input[self.cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0);
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),
            KeyCode::Esc => {
                self.input.clear();
                self.cursor = 0;
                self.history_index = None;
                self.highlighted = 0;
                self.notice = "Ready".into();
            }
            KeyCode::Up => {
                if self.input.starts_with('/') {
                    self.highlighted = self.highlighted.saturating_sub(1);
                } else if !self.password_input_active() && !self.history.is_empty() {
                    if self.history_index.is_none() {
                        self.history_draft.clone_from(&self.input);
                    }
                    let index = self
                        .history_index
                        .unwrap_or(self.history.len())
                        .saturating_sub(1);
                    self.input.clone_from(&self.history[index]);
                    self.history_index = Some(index);
                    self.cursor = self.input.len();
                }
            }
            KeyCode::Down if self.history_index.is_some() && !self.password_input_active() => {
                let index = self.history_index.unwrap() + 1;
                if index < self.history.len() {
                    self.input.clone_from(&self.history[index]);
                    self.history_index = Some(index);
                } else {
                    self.input.clone_from(&self.history_draft);
                    self.history_index = None;
                }
                self.cursor = self.input.len();
            }
            KeyCode::Down | KeyCode::Tab => {
                let total = self.suggestions().len();
                if total > 0 {
                    self.highlighted = (self.highlighted + 1).min(total - 1);
                }
            }
            KeyCode::Enter => {
                let waiting_for_secret = self.password_input_active();
                if self.input.is_empty() && !waiting_for_secret && !self.command_pending {
                    return Ok(None);
                }
                if waiting_for_secret || (self.view == View::Terminal && self.command_pending) {
                    let secret = std::mem::take(&mut self.input);
                    self.cursor = 0;
                    if let Some(shell) = &mut self.session {
                        shell.command(&secret)?;
                    }
                    return Ok(None);
                }
                if let Some(command) = self.input.strip_prefix("/shell ") {
                    self.native_command = Some(command.to_owned());
                    self.input.clear();
                    self.cursor = 0;
                    return Ok(Some(Exit::Shell));
                }
                if needs_native_terminal(&self.input) {
                    self.native_command = Some(std::mem::take(&mut self.input));
                    self.cursor = 0;
                    return Ok(Some(Exit::Shell));
                }
                if self.input.starts_with("/settings ") {
                    let input = std::mem::take(&mut self.input);
                    self.cursor = 0;
                    let parts: Vec<_> = input.split_whitespace().collect();
                    self.navigate(View::Settings);
                    self.notice = match parts.as_slice() {
                        [_, name @ ("hostname" | "timezone"), value] => {
                            match preferences::system_change(name, value) {
                                Ok(_) => {
                                    self.setting_confirmation =
                                        Some(((*name).into(), (*value).into()));
                                    format!(
                                        "Change {name} to {value}? Y confirm, Enter cancel. Administrator authorization follows."
                                    )
                                }
                                Err(error) => error.to_string(),
                            }
                        }
                        [_, name, value] => match self.preferences.set(name, value) {
                            Ok(()) => format!(
                                "Saved {name}: {value}{}",
                                if *name == "default" {
                                    ". Effective at next sign-in."
                                } else {
                                    ""
                                }
                            ),
                            Err(error) => format!("Settings unchanged: {error}"),
                        },
                        _ => {
                            "Usage: /settings default|color|refresh|hostname|timezone VALUE".into()
                        }
                    };
                    self.content = self.settings();
                    return Ok(None);
                }
                if self.input.starts_with("/tasks ") {
                    let input = std::mem::take(&mut self.input);
                    self.cursor = 0;
                    let parts: Vec<_> = input.split_whitespace().collect();
                    self.navigate(View::Tasks);
                    self.notice = match parts.as_slice() {
                        [
                            _,
                            operation @ ("pause" | "resume" | "terminate" | "kill"),
                            pid,
                        ] => {
                            let task = pid
                                .parse::<i32>()
                                .ok()
                                .and_then(|pid| self.monitor.tasks.iter().find(|t| t.pid == pid));
                            if let Some(task) = task {
                                self.task_confirmation = Some((task.clone(), (*operation).into()));
                                format!(
                                    "{operation} {} (PID {})? {} Y confirm, Enter cancel",
                                    task.name,
                                    task.pid,
                                    if *operation == "kill" {
                                        "Force kill can lose unsaved work."
                                    } else {
                                        ""
                                    }
                                )
                            } else {
                                "Task not found. It may have exited.".into()
                            }
                        }
                        _ => "Usage: /tasks pause|resume|terminate|kill PID".into(),
                    };
                    return Ok(None);
                }
                let action = self
                    .suggestions()
                    .get(self.highlighted)
                    .map(|item| item.0)
                    .or_else(|| action_for(&self.input));
                if let Some(rest) = self.input.strip_prefix("/store ") {
                    let query = rest.to_owned();
                    self.input.clear();
                    self.cursor = 0;
                    self.navigate(View::Store);
                    self.content = self.load_content(move |cancel| {
                    let parts: Vec<_> = query.split_whitespace().collect();
                    match parts.as_slice() {
                        ["installed"] => store::installed(cancel),
                        ["search", words @ ..] if !words.is_empty() => store::search(&words.join(" "), cancel),
                        ["info", package] => store::details(package, cancel),
                        ["install", package] => store::preview("install", package, cancel),
                        ["remove", package] => store::preview("remove", package, cancel),
                        _ => "Usage: /store installed | search QUERY | info PACKAGE | install PACKAGE | remove PACKAGE".into(),
                    }
                    });
                    return Ok(None);
                }
                if let Some(path) = self.input.strip_prefix("/assets/") {
                    let path = std::path::PathBuf::from(format!("/{path}"));
                    let result = self.assets.open(&path);
                    self.input.clear();
                    self.cursor = 0;
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
                self.cursor = 0;
                self.history_index = None;
                if self.history.last() != Some(&command) {
                    if self.history.len() == 100 {
                        self.history.remove(0);
                    }
                    self.history.push(command.clone());
                }
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

fn system_output(program: &str, args: &[&str], cancel: &query::Cancellation) -> String {
    query::text(program, args, cancel)
        .map(|text| text.chars().take(8000).collect())
        .unwrap_or_else(|error| format!("Unable to read system information: {error}"))
}

fn updates(cancel: &query::Cancellation) -> String {
    let version = system_output(
        "/usr/bin/dpkg-query",
        &["-W", "-f=${Version}", "linos"],
        cancel,
    );
    format!(
        "Linos updates\nInstalled package      linos {}\n\nUpdate channel         Development\nAutomatic OS updates   Disabled (not configured)\nRollback               Not implemented\n\nSystem packages currently use Debian apt. No update was started.\nA verified update mechanism and rollback policy are required before automatic Linos updates can be enabled.",
        version.trim(),
    )
}

fn paragraph_at(frame: &mut Frame, rect: Rect, text: Vec<Line<'_>>) {
    frame.render_widget(Paragraph::new(text), rect);
}

fn input_window(input: &str, cursor: usize, width: u16, masked: bool) -> (String, u16) {
    let cursor = cursor.min(input.len());
    let before = input.get(..cursor).unwrap_or_default();
    let display = if masked {
        "*".repeat(input.chars().count())
    } else {
        input.to_owned()
    };
    let position = if masked {
        before.chars().count()
    } else {
        before.len()
    };
    let mut start = 0;
    while Span::raw(&display[start..position]).width() >= usize::from(width.max(1)) {
        let Some(c) = display[start..position].chars().next() else {
            break;
        };
        start += c.len_utf8();
    }
    let cursor_cells = Span::raw(&display[start..position]).width() as u16;
    let mut end = start;
    let mut cells = 0;
    for c in display[start..].chars() {
        let size = Span::raw(c.to_string()).width();
        if cells + size > usize::from(width) {
            break;
        }
        cells += size;
        end += c.len_utf8();
    }
    (display[start..end].to_owned(), cursor_cells)
}

fn transcript_lines(output: &str, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    let mut lines = Vec::new();
    for source in output.split('\n') {
        let mut row = String::new();
        let mut cells = 0;
        for c in source.chars() {
            let size = if c == '\t' {
                8 - cells % 8
            } else {
                Span::raw(c.to_string()).width()
            };
            if cells + size > width && !row.is_empty() {
                lines.push(Line::raw(std::mem::take(&mut row)));
                cells = 0;
            }
            if c == '\t' {
                row.push_str(&" ".repeat(size.min(width)));
            } else {
                row.push(c);
            }
            cells += size;
        }
        lines.push(Line::raw(row));
    }
    lines
}

fn draw(frame: &mut Frame, app: &App) {
    draw_workspace(frame, app);
    let question = if let Some((task, operation)) = &app.task_confirmation {
        Some(format!(
            "{operation} {} (PID {})?\n\n{}\nKernel permissions apply.\n\nY confirms. Enter or Esc cancels.",
            task.name,
            task.pid,
            if operation == "kill" {
                "Force kill can lose unsaved work."
            } else {
                "The process may be running server work."
            }
        ))
    } else if let Some((name, value)) = &app.setting_confirmation {
        Some(format!(
            "Change {name} to {value}?\n\nAdministrator authorization follows.\nReview the command result before assuming success.\n\nY confirms. Enter or Esc cancels."
        ))
    } else {
        app.confirm.map(|action| format!("{} server?\n\nActive server work will be interrupted.\n\nY confirms. Enter or Esc cancels.", if action == Action::Reboot { "Restart" } else { "Shut down" }))
    };
    if let Some(question) = question {
        let area = frame.area();
        let width = area.width.saturating_sub(4).min(76);
        let height = area.height.saturating_sub(2).min(12);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(question).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .title(" Confirm action ")
                    .border_style(Style::default().fg(ACCENT)),
            ),
            rect,
        );
    }
    if !app.preferences.colors() {
        for cell in frame.buffer_mut().content.iter_mut() {
            cell.set_fg(Color::Reset).set_bg(Color::Reset);
        }
    }
}

fn draw_workspace(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 36 || area.height < 14 {
        frame.render_widget(
            Paragraph::new("Linos 13 Server\n\nResize the terminal for the interactive workspace.\nCtrl+T: shell"),
            area,
        );
        return;
    }

    // Linux commands are not dashboard cards. Keep their transcript close to
    // the top and use a single compact command line instead of a framed
    // composer, repeated shell prompt, directory banner and footer.
    if app.view == View::Terminal {
        let left = area.x.saturating_add(2);
        let width = area.width.saturating_sub(4);
        paragraph_at(
            frame,
            Rect::new(left, area.y.saturating_add(1), width, 1),
            vec![Line::styled(
                "Linos 13 · Server",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )],
        );
        let output = app.terminal_output.as_str();
        let transcript_height = area.height.saturating_sub(6);
        let lines = transcript_lines(output, width);
        let transcript_scroll = lines
            .len()
            .saturating_sub(usize::from(transcript_height))
            .min(u16::MAX as usize) as u16;
        frame.render_widget(
            Paragraph::new(lines).scroll((
                if app.follow_tail {
                    transcript_scroll
                } else {
                    app.scroll
                },
                0,
            )),
            Rect::new(left, area.y.saturating_add(3), width, transcript_height),
        );
        let home = env::var("HOME").unwrap_or_default();
        let directory = app
            .cwd
            .strip_prefix(&home)
            .ok()
            .map(|relative| {
                if relative.as_os_str().is_empty() {
                    "~".to_owned()
                } else {
                    format!("~/{}", relative.display())
                }
            })
            .unwrap_or_else(|| app.cwd.display().to_string());
        let waiting_for_secret = app.password_input_active();
        let prompt = if waiting_for_secret {
            "Password (hidden): ".to_owned()
        } else {
            format!(
                "{}:{}{} ",
                host(),
                directory,
                if app.user == "root" { '#' } else { '$' }
            )
        };
        let prompt: String = prompt.chars().take(usize::from(width / 2)).collect();
        let prompt_width = Span::raw(&prompt).width().min(usize::from(width)) as u16;
        let (input, cursor_cells) = input_window(
            &app.input,
            app.cursor,
            width.saturating_sub(prompt_width),
            waiting_for_secret,
        );
        let input_y = area.bottom().saturating_sub(2);
        paragraph_at(
            frame,
            Rect::new(left, input_y, width, 1),
            vec![Line::from(vec![
                Span::styled(prompt.clone(), Style::default().fg(ACCENT)),
                Span::raw(input.clone()),
            ])],
        );
        let cursor_x = left
            .saturating_add(prompt_width)
            .saturating_add(cursor_cells)
            .min(left.saturating_add(width.saturating_sub(1)));
        frame.set_cursor_position((cursor_x, input_y));
        paragraph_at(
            frame,
            Rect::new(left, area.bottom().saturating_sub(1), width, 1),
            vec![Line::styled(&app.notice, Style::default().fg(DIM))],
        );
        return;
    }

    let width = area
        .width
        .saturating_sub(if area.width > 100 { 8 } else { 2 });
    let x = area.x + (area.width - width) / 2;
    let top = area.y + 1;
    let composer_y = area.bottom().saturating_sub(5);
    let left = x + 2;
    let inner_width = width.saturating_sub(4);

    paragraph_at(
        frame,
        Rect::new(x, top, width, 1),
        vec![Line::from(vec![
            Span::styled(
                "◆  Linos",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  13  /  Server", Style::default().fg(DIM)),
            Span::styled(
                format!("    •    {}@{}", app.user, host()),
                Style::default().fg(DIM),
            ),
        ])],
    );

    if app.view == View::Home {
        let hero_y = top + 3;
        paragraph_at(
            frame,
            Rect::new(left, hero_y, inner_width, 2),
            vec![
                Line::styled(
                    format!("Welcome back, {}.", app.user),
                    Style::default()
                        .fg(Color::Reset)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    "What would you like to do? /status shows current checks.",
                    Style::default().fg(DIM),
                ),
            ],
        );

        if composer_y > hero_y + 11 {
            let status_y = hero_y + 4;
            frame.render_widget(
                Block::default()
                    .title("  Server overview  ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
                Rect::new(x, status_y, width, 6),
            );
            paragraph_at(
                frame,
                Rect::new(left, status_y + 1, inner_width, 4),
                vec![
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
                ],
            );

            if composer_y > status_y + 9 {
                paragraph_at(
                    frame,
                    Rect::new(left, status_y + 7, inner_width, 2),
                    vec![
                        Line::styled(
                            "Explore your server",
                            Style::default().fg(DIM).add_modifier(Modifier::BOLD),
                        ),
                        Line::from(vec![
                            Span::styled("/status  ", Style::default().fg(ACCENT)),
                            Span::styled("Overview    ", Style::default().fg(DIM)),
                            Span::styled("/network  ", Style::default().fg(ACCENT)),
                            Span::styled("Network    ", Style::default().fg(DIM)),
                            Span::styled("/services  ", Style::default().fg(ACCENT)),
                            Span::styled("Services", Style::default().fg(DIM)),
                        ]),
                    ],
                );
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
            View::Tasks => "Tasks",
            View::Assets => "Assets",
            View::Settings => "Settings",
            View::Updates => "Updates",
            View::Hardware => "Hardware",
            View::Store => "Linus store",
            View::Terminal => "Terminal",
        };
        paragraph_at(
            frame,
            Rect::new(left, top + 3, inner_width, 1),
            vec![Line::styled(
                title,
                Style::default()
                    .fg(Color::Reset)
                    .add_modifier(Modifier::BOLD),
            )],
        );
    }

    let waiting_for_secret = app.password_input_active();
    let suggestions = if waiting_for_secret {
        Vec::new()
    } else {
        app.suggestions()
    };
    let available_suggestions = composer_y.saturating_sub(top + 8) as usize;
    let count = suggestions.len().min(5).min(available_suggestions);
    let suggestion_y = composer_y.saturating_sub(count as u16 + 1);
    if app.view != View::Home && suggestion_y > top + 6 {
        frame.render_widget(
            Paragraph::new(app.content.as_str())
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0))
                .style(Style::default().fg(Color::Reset)),
            Rect::new(left, top + 6, inner_width, suggestion_y - (top + 6)),
        );
    }

    let first_suggestion = app.highlighted.saturating_sub(count.saturating_sub(1));
    for (index, (_, command, description)) in suggestions
        .iter()
        .skip(first_suggestion)
        .take(count)
        .enumerate()
    {
        let active = index + first_suggestion == app.highlighted;
        paragraph_at(
            frame,
            Rect::new(left, suggestion_y + index as u16, inner_width, 1),
            vec![Line::from(vec![
                Span::styled(
                    format!("{} {command:<12}", if active { "›" } else { " " }),
                    Style::default().fg(if active { ACCENT } else { DIM }),
                ),
                Span::styled(
                    *description,
                    Style::default().fg(if active { Color::Reset } else { DIM }),
                ),
            ])],
        );
    }

    paragraph_at(
        frame,
        Rect::new(left, composer_y.saturating_sub(1), inner_width, 1),
        vec![Line::styled(
            format!(
                "  {}  {}",
                if app.view == View::Assets {
                    "Assets"
                } else {
                    "Directory"
                },
                if app.view == View::Assets {
                    app.assets.path.display().to_string()
                } else {
                    app.cwd.display().to_string()
                }
            ),
            Style::default().fg(DIM),
        )],
    );
    frame.render_widget(
        Block::default()
            .title("  Command  ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if app.input.is_empty() {
                Color::DarkGray
            } else {
                ACCENT
            })),
        Rect::new(x, composer_y, width, 3),
    );
    let (display_input, cursor_cells) = input_window(
        &app.input,
        app.cursor,
        inner_width.saturating_sub(2),
        waiting_for_secret,
    );
    paragraph_at(
        frame,
        Rect::new(left, composer_y + 1, inner_width, 1),
        vec![Line::from(vec![
            Span::styled("› ", Style::default().fg(ACCENT)),
            Span::styled(
                if app.input.is_empty() {
                    if waiting_for_secret {
                        "Password (hidden)"
                    } else {
                        "Type / for commands or enter a Linux command"
                    }
                } else {
                    &display_input
                },
                Style::default().fg(if app.input.is_empty() {
                    DIM
                } else {
                    Color::Reset
                }),
            ),
        ])],
    );
    // The cursor follows the user input rather than showing a static cursor glyph.
    let cursor_x = left
        .saturating_add(2)
        .saturating_add(cursor_cells)
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
    paragraph_at(
        frame,
        Rect::new(x, area.bottom().saturating_sub(1), width, 1),
        vec![Line::from(vec![
            Span::styled(format!("◆  {footer}"), Style::default().fg(DIM)),
            Span::styled(
                "    / Commands · Ctrl+T Shell · Ctrl+R Fit",
                Style::default().fg(DIM),
            ),
        ])],
    );
}

fn needs_native_terminal(command: &str) -> bool {
    let mut words = command.split_whitespace();
    let first = words.next().unwrap_or_default();
    let name = std::path::Path::new(first)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    match name {
        "top" => !words.any(|s| s == "-b" || s.starts_with("-b")),
        "vim" | "vi" | "nvim" | "nano" | "emacs" | "less" | "more" | "man" | "htop" | "btop"
        | "watch" | "ssh" | "tmux" | "screen" | "codex" | "claude" => true,
        _ => false,
    }
}

fn interactive_shell(
    command: Option<&str>,
    cwd: &std::path::Path,
) -> io::Result<std::process::ExitStatus> {
    struct ForegroundGuard(libc::pid_t);
    impl Drop for ForegroundGuard {
        fn drop(&mut self) {
            if self.0 <= 0 {
                return;
            }
            // A job-controlling child can leave Home in the background when
            // it exits. Block SIGTTOU only in this thread while restoring our
            // foreground group, then restore the original signal mask.
            unsafe {
                let mut signals: libc::sigset_t = std::mem::zeroed();
                let mut previous: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&raw mut signals);
                libc::sigaddset(&raw mut signals, libc::SIGTTOU);
                if libc::pthread_sigmask(libc::SIG_BLOCK, &signals, &raw mut previous) == 0 {
                    libc::tcsetpgrp(libc::STDIN_FILENO, self.0);
                    libc::pthread_sigmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut());
                }
            }
        }
    }
    let foreground = ForegroundGuard(unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) });
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
    if bash && command.is_none() {
        child.args(["--rcfile", "/usr/lib/linos/linos-shell.bashrc"]);
    }
    if let Some(command) = command {
        // Interactive job control gives the child its own foreground process
        // group. Ctrl+C must interrupt its command without killing Home.
        child.args(["-i", "-c", command]);
    } else {
        child.arg("-i");
    }
    let status = child
        .current_dir(cwd)
        .env("LINOS_NO_WELCOME", "1")
        .status()?;
    drop(foreground);
    Ok(status)
}

pub fn run() -> io::Result<ExitCode> {
    let mut app = App::new();
    if let Ok((_, typed)) = geometry::negotiate()
        && !typed.is_empty()
    {
        app.paste(&String::from_utf8_lossy(&typed));
    }
    loop {
        // Ratatui owns the terminal only while Linos Home is active. Leaving
        // this scope restores the real terminal before entering a child shell.
        let result = ratatui::run(|terminal| {
            struct PasteGuard;
            impl Drop for PasteGuard {
                fn drop(&mut self) {
                    let _ = crossterm::execute!(io::stdout(), event::DisableBracketedPaste);
                }
            }
            crossterm::execute!(io::stdout(), event::EnableBracketedPaste)?;
            let _paste_guard = PasteGuard;
            let mut last_refresh = Instant::now();
            let mut dirty = true;
            'workspace: loop {
                dirty |= app.poll_content();
                if matches!(app.view, View::Resources | View::Tasks)
                    && last_refresh.elapsed() >= Duration::from_secs(app.preferences.refresh)
                {
                    app.refresh_monitor();
                    last_refresh = Instant::now();
                    dirty = true;
                }
                if let Some(shell) = &mut app.session {
                    if let Some(cwd) = shell.current_dir()
                        && app.cwd != cwd
                    {
                        app.cwd = cwd;
                        dirty = true;
                    }
                    let changed = shell.drain(&mut app.terminal_output);
                    if changed && shell.is_ready() && app.command_pending {
                        app.command_pending = false;
                        app.notice = match shell.exit_status() {
                            Some(0) => "Command completed · exit 0 · /home returns to Home".into(),
                            Some(status) => {
                                format!("Command failed · exit {status} · /home returns to Home")
                            }
                            None => "Command completed · /home returns to Home".into(),
                        };
                    }
                    if changed && app.view == View::Terminal {
                        app.content.clone_from(&app.terminal_output);
                        if app.follow_tail {
                            let height = crossterm::terminal::size()
                                .map(|(_, h)| h.saturating_sub(14))
                                .unwrap_or(10);
                            app.scroll = app
                                .content
                                .lines()
                                .count()
                                .saturating_sub(usize::from(height))
                                .min(usize::from(u16::MAX))
                                as u16;
                        }
                        dirty = true;
                    }
                }
                if app
                    .session
                    .as_mut()
                    .is_some_and(|shell| shell.exited().unwrap_or(false))
                {
                    app.session = None;
                    app.command_pending = false;
                    app.notice =
                        "Command session ended. Enter a command to start a new session.".into();
                    dirty = true;
                }
                if dirty {
                    terminal.draw(|frame| draw(frame, &app))?;
                    dirty = false;
                }
                // Preserve event order while avoiding a frame for every key
                // already queued by a serial terminal or a pasted command.
                for event_index in 0..32 {
                    let wait = if event_index > 0 {
                        Duration::ZERO
                    } else if app.command_pending || app.view_job.is_some() {
                        Duration::from_millis(20)
                    } else {
                        Duration::from_millis(100)
                    };
                    if !event::poll(wait)? {
                        break;
                    }
                    match event::read()? {
                        Event::Key(key) => {
                            if key.kind == KeyEventKind::Press
                                && key.code == KeyCode::Char('r')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                let (sized, typed) = geometry::negotiate()?;
                                app.notice = if sized {
                                    "Terminal geometry refreshed".into()
                                } else {
                                    "Terminal does not support size reporting; use stty rows/cols"
                                        .into()
                                };
                                if !typed.is_empty() {
                                    app.paste(&String::from_utf8_lossy(&typed));
                                }
                                terminal.autoresize()?;
                                let size = terminal.size()?;
                                if let Some(shell) = &mut app.session {
                                    shell.resize(
                                        size.height.saturating_sub(6),
                                        size.width.saturating_sub(4),
                                    )?;
                                }
                                dirty = true;
                                continue;
                            }
                            if let Some(exit) = app.key(key)? {
                                break 'workspace Ok::<Exit, io::Error>(exit);
                            }
                            dirty = true;
                        }
                        Event::Resize(width, height) => {
                            if let Some(shell) = &mut app.session {
                                shell.resize(height.saturating_sub(12), width.saturating_sub(4))?;
                            }
                            dirty = true;
                        }
                        Event::Paste(text) => {
                            app.paste(&text);
                            dirty = true;
                        }
                        _ => {}
                    }
                }
            }
        })?;
        app.view_job = None;
        match result {
            Exit::Shell => {
                // Exiting the nested shell (exit/Ctrl+D) returns here rather
                // than terminating the Linos Home session.
                let command = app.native_command.take();
                let return_view = if app.command_pending {
                    app.view
                } else {
                    View::Home
                };
                let result = interactive_shell(command.as_deref(), &app.cwd);
                app.navigate(return_view);
                app.notice = match result {
                    Ok(status) if status.success() => {
                        if return_view == View::Terminal {
                            "Returned to command session".into()
                        } else {
                            "Returned to Home".into()
                        }
                    }
                    Ok(status) => format!("Native command ended: {status}"),
                    Err(error) => format!("Unable to start native command: {error}"),
                };
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
    fn held_backspace_edits_the_composer() {
        let mut app = App::new();
        for c in "hello".chars() {
            app.key(press(KeyCode::Char(c))).unwrap();
        }
        let mut repeat = press(KeyCode::Backspace);
        repeat.kind = KeyEventKind::Repeat;
        app.key(repeat).unwrap();
        assert_eq!(app.input, "hell");
    }

    #[test]
    fn cursor_editing_preserves_unicode() {
        let mut app = App::new();
        for c in "a界b".chars() {
            app.key(press(KeyCode::Char(c))).unwrap();
        }
        app.key(press(KeyCode::Left)).unwrap();
        app.key(press(KeyCode::Backspace)).unwrap();
        assert_eq!(app.input, "ab");
        app.key(press(KeyCode::Delete)).unwrap();
        assert_eq!(app.input, "a");
    }

    #[test]
    fn ctrl_c_cancels_unsubmitted_input() {
        let mut app = App::new();
        app.input = "unsubmitted command".into();
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(app.input.is_empty());
    }

    #[test]
    fn paste_is_bounded_and_never_executes_commands() {
        let mut app = App::new();
        app.paste("printf hello\nreboot\x1b[2J");
        assert!(!app.input.contains('\n'));
        assert!(!app.input.contains('\x1b'));
        assert!(app.session.is_none());
        assert!(app.confirm.is_none());
        app.paste(&"界".repeat(5000));
        assert!(app.input.len() <= 4096);
        assert!(app.input.is_char_boundary(app.cursor));
    }

    #[test]
    fn slow_view_reads_leave_input_usable_and_cannot_overwrite_navigation() {
        let mut app = App::new();
        let (release, pending) = std::sync::mpsc::channel();
        app.content = app.load_content(move |_| {
            let _ = pending.recv();
            "Old view result".into()
        });
        app.key(press(KeyCode::Char('x'))).unwrap();
        assert_eq!(app.input, "x");
        assert!(!app.poll_content());
        app.navigate(View::Home);
        let _ = release.send(());
        assert!(!app.poll_content());
        assert!(app.content.is_empty());
    }

    #[test]
    fn leaving_a_view_cancels_its_query_context() {
        let mut app = App::new();
        let (sender, receiver) = std::sync::mpsc::channel();
        app.content = app.load_content(move |cancel| {
            sender.send(cancel.clone()).unwrap();
            String::new()
        });
        let cancel = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!cancel.is_cancelled());
        app.navigate(View::Home);
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn control_y_does_not_confirm_a_task_action() {
        let mut app = App::new();
        app.monitor.sample();
        let own = app
            .monitor
            .tasks
            .iter()
            .find(|task| task.pid == std::process::id() as i32)
            .unwrap()
            .clone();
        // Home's self-protection also makes this a harmless regression probe
        // before the confirmation shortcut is fixed.
        app.task_confirmation = Some((own, "terminate".into()));
        app.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(
            app.task_confirmation.is_some(),
            "Ctrl+Y confirmed a destructive action"
        );
    }

    #[test]
    fn confirmation_requires_an_explicit_unmodified_press() {
        assert!(confirms_action(press(KeyCode::Char('y'))));
        assert!(confirms_action(KeyEvent::new(
            KeyCode::Char('Y'),
            KeyModifiers::SHIFT
        )));
        for modifier in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SUPER,
            KeyModifiers::META,
            KeyModifiers::HYPER,
        ] {
            assert!(!confirms_action(KeyEvent::new(
                KeyCode::Char('y'),
                modifier
            )));
        }
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            let mut key = press(KeyCode::Char('y'));
            key.kind = kind;
            assert!(!confirms_action(key));
        }
    }

    #[test]
    fn long_unicode_input_keeps_the_cursor_visible_and_masks_secrets() {
        let input = "abcdef界界界12345";
        let (visible, cursor) = input_window(input, input.len(), 8, false);
        assert!(visible.ends_with("12345"));
        assert!(cursor < 8);
        assert!(Span::raw(visible).width() <= 8);
        let (masked, _) = input_window(input, input.len(), 8, true);
        assert!(masked.chars().all(|c| c == '*'));
        assert_eq!(transcript_lines("界界界界\n123456789", 4).len(), 5);
    }

    #[test]
    fn native_terminal_programs_leave_home_before_running() {
        for command in ["top", "/usr/bin/vim file", "ssh server", "/shell sudo top"] {
            let mut app = App::new();
            app.paste(command);
            assert_eq!(app.key(press(KeyCode::Enter)).unwrap(), Some(Exit::Shell));
            assert!(app.native_command.is_some());
            assert!(app.session.is_none());
        }
        assert!(!needs_native_terminal("top -b -n 1"));
        assert!(!needs_native_terminal("printf top"));
    }

    #[test]
    fn system_setting_confirmation_cancels_without_mutation() {
        let mut app = App::new();
        app.paste("/settings hostname proposed-name");
        app.key(press(KeyCode::Enter)).unwrap();
        assert_eq!(app.view, View::Settings);
        assert!(app.setting_confirmation.is_some());
        assert!(app.session.is_none());
        app.key(press(KeyCode::Enter)).unwrap();
        assert!(app.setting_confirmation.is_none());
        assert!(app.session.is_none());
        assert!(app.notice.contains("No change made"));
    }

    #[test]
    fn task_controls_require_confirmation_and_default_to_cancel() {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let mut app = App::new();
        app.paste(&format!("/tasks terminate {}", child.id()));
        app.key(press(KeyCode::Enter)).unwrap();
        assert!(app.task_confirmation.is_some());
        app.key(press(KeyCode::Enter)).unwrap();
        assert!(app.task_confirmation.is_none());
        let still_running = child.try_wait().unwrap().is_none();
        let _ = child.kill();
        let _ = child.wait();
        assert!(still_running);
    }

    #[test]
    fn palette_scrolls_to_the_action_enter_will_execute() {
        let mut app = App::new();
        app.paste("/s");
        for _ in 0..6 {
            app.key(press(KeyCode::Down)).unwrap();
        }
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(screen.contains("› /shutdown"));
        app.key(press(KeyCode::Enter)).unwrap();
        assert_eq!(app.confirm, Some(Action::Shutdown));
    }

    #[test]
    fn management_views_render_at_small_sizes_without_color() {
        let mut app = App::new();
        app.preferences.color = "never".into();
        for view in [
            View::Home,
            View::Resources,
            View::Tasks,
            View::Settings,
            View::Terminal,
        ] {
            app.navigate(view);
            for (width, height) in [(30, 10), (36, 14), (48, 18), (80, 24), (120, 32)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| draw(frame, &app)).unwrap();
                for cell in &terminal.backend().buffer().content {
                    assert_eq!(cell.fg, Color::Reset);
                    assert_eq!(cell.bg, Color::Reset);
                }
            }
        }
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
            assert!(
                screen.contains("Linos"),
                "Missing Linos identity at {width}x{height}"
            );
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
        assert_eq!(app.suggestions().len(), 6); // status, services, settings, store, shell, shutdown
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
    fn idle_terminal_does_not_invent_password_request() {
        let mut app = App::new();
        app.view = View::Terminal;
        app.command_pending = false;
        assert!(!app.password_input_active());
    }

    fn secret_prompt() -> App {
        let mut app = App::new();
        app.run_native_command(
            "stty -echo; printf 'Password: '; read secret; printf '\\nSECRET_ACCEPTED\\n'".into(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.session
                .as_ref()
                .unwrap()
                .drain(&mut app.terminal_output);
            if app.terminal_output.ends_with("Password: ") && app.password_input_active() {
                return app;
            }
            assert!(Instant::now() < deadline, "Secret prompt did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn compact_terminal_never_renders_secret_input() {
        let mut app = secret_prompt();
        app.input = "private-test-secret".into();
        for (width, height) in [(48, 18), (80, 24), (120, 32)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &app)).unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol().to_owned())
                .collect();
            assert!(
                !screen.contains(&app.input),
                "Secret exposed at {width}x{height}"
            );
            assert!(screen.contains("Password (hidden)"));
        }
    }

    #[test]
    fn secret_input_bypasses_dashboard_routes() {
        for secret in ["/store installed", "/assets/etc", "/clear", "/shell"] {
            let mut app = secret_prompt();
            app.input = secret.into();
            assert_eq!(app.key(press(KeyCode::Enter)).unwrap(), None);
            assert_eq!(app.view, View::Terminal);
            assert!(app.input.is_empty());
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                app.session
                    .as_ref()
                    .unwrap()
                    .drain(&mut app.terminal_output);
                if app.terminal_output.contains("\nSECRET_ACCEPTED\n") {
                    break;
                }
                assert!(Instant::now() < deadline, "Secret was not sent to the PTY");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!app.terminal_output.contains(secret));
        }
    }

    #[test]
    fn secret_masking_survives_cleared_and_localized_prompts() {
        let mut app = secret_prompt();
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(
            app.password_input_active(),
            "Clearing output exposed secret input"
        );
        app.terminal_output = "Mot de passe : ".into();
        assert!(
            app.password_input_active(),
            "Translated prompt exposed secret input"
        );
        app.navigate(View::Home);
        assert!(
            app.password_input_active(),
            "Changing views exposed secret input"
        );
    }

    #[test]
    fn resource_bars_handle_zero_and_percentages() {
        assert!(monitor::bar(0, 0, 12).contains("Not configured"));
        assert!(monitor::bar(5, 10, 10).contains("50%"));
        assert!(monitor::bar(15, 10, 10).contains("100%"));
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
