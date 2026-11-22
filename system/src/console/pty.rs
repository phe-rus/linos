//! A long-lived real Linux shell connected to a pseudo-terminal.
//! Home sends complete commands to this PTY; the shell retains its cwd,
//! environment and job state. Full-screen terminal applications belong in
//! the native Shell mode until an ANSI terminal emulator is integrated.
use std::{
    cell::Cell,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

enum Output {
    Text(String),
    Prompt(i32),
}

#[derive(Debug, PartialEq)]
enum Part {
    Bytes(Vec<u8>),
    Prompt(i32),
}

pub struct Session {
    writer: File,
    output: Receiver<Output>,
    carriage_return: Cell<bool>,
    ready: Cell<bool>,
    exit_status: Cell<Option<i32>>,
    child: Child,
    slave_tty: File,
    stop_reader: Arc<AtomicBool>,
}

impl Session {
    pub fn start(rows: u16, cols: u16) -> io::Result<Self> {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: rows.max(2),
            ws_col: cols.max(20),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty writes two owned file descriptors, initialized above.
        if unsafe {
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openpty returned unique owned descriptors.
        let writer = unsafe { File::from_raw_fd(master) };
        let slave_file = unsafe { File::from_raw_fd(slave) };
        // The embedded command protocol is POSIX. Native Shell mode still
        // honors the user's preferred shell and startup configuration.
        let shell = if std::path::Path::new("/bin/bash").is_file() {
            "/bin/bash"
        } else {
            "/bin/sh"
        };
        let mut cmd = Command::new(shell);
        // The embedded shell must never source user startup files that can
        // automatically launch Linos again. Native Shell mode keeps .bashrc.
        if cmd.get_program().to_string_lossy().ends_with("/bash") {
            cmd.args(["--noprofile", "--norc"]);
        }
        let marker = format!(
            "\x1b]777;linos-ready-{}-{};",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        cmd.arg("-i")
            .env("LINOS_NO_WELCOME", "1")
            .env("LINOS_EMBEDDED_PTY", "1")
            // An explicit prompt handshake keeps command completion separate
            // from arbitrary output, including text resembling a shell prompt.
            .env("PS1", format!("{marker}$?\x07"))
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave_file.try_clone()?))
            .stdout(Stdio::from(slave_file.try_clone()?))
            .stderr(Stdio::from(slave_file.try_clone()?));
        // SAFETY: pre_exec runs in the child after fork. Use only
        // async-signal-safe libc functions to establish controlling TTY.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY, 0) == -1
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn()?;
        let slave_tty = slave_file;
        let mut reader = writer.try_clone()?;
        let (tx, output) = mpsc::sync_channel(128);
        let stop_reader = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop_reader);
        thread::spawn(move || {
            let mut data = [0u8; 8192];
            let mut escape_state = 0u8;
            let mut pending_utf8 = Vec::new();
            let mut pending_marker = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                let mut poll = libc::pollfd {
                    fd: reader.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // Polling allows session teardown even if a background child
                // keeps the slave open without producing any output.
                let ready = unsafe { libc::poll(&raw mut poll, 1, 100) };
                if ready == 0 {
                    continue;
                }
                if ready < 0 {
                    break;
                }
                let Ok(len) = reader.read(&mut data) else {
                    break;
                };
                if len == 0 {
                    break;
                }
                for part in split_markers(&mut pending_marker, &data[..len], marker.as_bytes()) {
                    let event = match part {
                        Part::Bytes(bytes) => {
                            let clean = plain_text(&bytes, &mut escape_state);
                            let text = decode_utf8(&mut pending_utf8, &clean);
                            if text.is_empty() {
                                continue;
                            }
                            Output::Text(text)
                        }
                        Part::Prompt(status) => Output::Prompt(status),
                    };
                    if tx.send(event).is_err() {
                        return;
                    }
                }
            }
        });
        let session = Self {
            writer,
            output,
            child,
            slave_tty,
            carriage_return: Cell::new(false),
            ready: Cell::new(false),
            exit_status: Cell::new(None),
            stop_reader,
        };
        // Consume startup output before sending the first command. Otherwise
        // the initial prompt could falsely complete that command.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match session
                .output
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(Output::Prompt(status)) => {
                    session.ready.set(true);
                    session.exit_status.set(Some(status));
                    return Ok(session);
                }
                Ok(Output::Text(_)) => {}
                Err(error) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("Shell did not become ready: {error}"),
                    ));
                }
            }
        }
    }

    pub fn current_dir(&self) -> Option<std::path::PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.child.id())).ok()
    }

    pub fn command(&mut self, command: &str) -> io::Result<()> {
        self.ready.set(false);
        self.exit_status.set(None);
        self.writer.write_all(command.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    pub fn key(&mut self, byte: u8) -> io::Result<()> {
        self.writer.write_all(&[byte])?;
        self.writer.flush()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        let size = libc::winsize {
            ws_row: rows.max(2),
            ws_col: cols.max(20),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: ioctl only reads this initialized struct; writer owns a PTY.
        if unsafe {
            libc::ioctl(
                std::os::fd::AsRawFd::as_raw_fd(&self.writer),
                libc::TIOCSWINSZ,
                &size,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.ready.get()
    }

    pub fn exit_status(&self) -> Option<i32> {
        self.exit_status.get()
    }

    pub fn exited(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }

    pub fn echo_enabled(&self) -> bool {
        // Read the PTY slave's termios, not the master. On failure, never
        // invent a password prompt simply because the query failed.
        let mut termios: libc::termios = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::tcgetattr(self.slave_tty.as_raw_fd(), &raw mut termios) };
        result != 0 || (termios.c_lflag & libc::ECHO != 0)
    }

    pub fn drain(&self, output: &mut String) -> bool {
        let mut changed = false;
        // A noisy child must not monopolize Home's input/render loop.
        for _ in 0..16 {
            let Ok(event) = self.output.try_recv() else {
                break;
            };
            changed = true;
            match event {
                Output::Text(text) => append_preview(output, &text, &self.carriage_return),
                Output::Prompt(status) => {
                    self.ready.set(true);
                    self.exit_status.set(Some(status));
                }
            }
            if output.len() > 32_000 {
                let mut boundary = output.len() - 24_000;
                while !output.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                output.drain(..boundary);
            }
        }
        changed
    }
}

// Retain only a possible partial marker between reads. No assumption is made
// about how the kernel splits escape sequences or prompt bytes across reads.
fn split_markers(pending: &mut Vec<u8>, bytes: &[u8], marker: &[u8]) -> Vec<Part> {
    pending.extend_from_slice(bytes);
    let mut parts = Vec::new();
    loop {
        if let Some(index) = pending.windows(marker.len()).position(|s| s == marker) {
            if index > 0 {
                parts.push(Part::Bytes(pending.drain(..index).collect()));
            }
            let Some(end) = pending[marker.len()..]
                .iter()
                .position(|&b| b == 7)
                .map(|i| marker.len() + i)
            else {
                if pending.len() <= marker.len() + 12 {
                    break;
                }
                parts.push(Part::Bytes(pending.drain(..1).collect()));
                continue;
            };
            let status = std::str::from_utf8(&pending[marker.len()..end])
                .ok()
                .and_then(|s| s.parse::<i32>().ok());
            if let Some(status) = status {
                parts.push(Part::Prompt(status));
            } else {
                parts.push(Part::Bytes(pending[..=end].to_vec()));
            }
            pending.drain(..=end);
            continue;
        }
        let keep = (1..marker.len().min(pending.len() + 1))
            .rev()
            .find(|&n| pending.ends_with(&marker[..n]))
            .unwrap_or(0);
        let emit = pending.len() - keep;
        if emit > 0 {
            parts.push(Part::Bytes(pending.drain(..emit).collect()));
        }
        break;
    }
    parts
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop_reader.store(true, Ordering::Relaxed);
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        // The PTY session is separate from Home's process group. Hang up its
        // foreground work and let the shell clean up its own jobs first.
        let foreground = unsafe { libc::tcgetpgrp(self.slave_tty.as_raw_fd()) };
        if foreground > 1 {
            unsafe {
                libc::kill(-foreground, libc::SIGHUP);
            }
        }
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGHUP);
        }
        for _ in 0..10 {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// Read-only output preview, not a terminal emulator. Keeps command content
// while avoiding escape sequences that might corrupt Ratatui rendering.
pub fn preview(bytes: &[u8]) -> String {
    let clean = plain_text(bytes, &mut 0);
    let mut output = String::new();
    append_preview(
        &mut output,
        &String::from_utf8_lossy(&clean),
        &Cell::new(false),
    );
    output
}

fn plain_text(bytes: &[u8], state: &mut u8) -> Vec<u8> {
    let mut clean = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        match *state {
            0 if byte == 0x1b => *state = 1,
            0 if matches!(byte, b'\r' | b'\n' | b'\t' | 8) || (byte >= 0x20 && byte != 0x7f) => {
                clean.push(byte)
            }
            1 if byte == b'[' => *state = 2,
            1 if byte == b']' => *state = 3,
            // DCS, SOS, PM and APC are strings terminated by ST, not CSI.
            1 if matches!(byte, b'P' | b'X' | b'^' | b'_') => *state = 5,
            1 => *state = 0,
            2 if (0x40..=0x7e).contains(&byte) => *state = 0,
            3 if byte == 0x07 => *state = 0,
            3 if byte == 0x1b => *state = 4,
            4 => *state = if byte == b'\\' { 0 } else { 3 },
            5 if byte == 0x1b => *state = 6,
            6 => *state = if byte == b'\\' { 0 } else { 5 },
            _ => {}
        }
    }
    clean
}

fn decode_utf8(pending: &mut Vec<u8>, bytes: &[u8]) -> String {
    pending.extend_from_slice(bytes);
    let mut output = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                output.push_str(text);
                pending.clear();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                output.push_str(std::str::from_utf8(&pending[..valid]).unwrap());
                pending.drain(..valid);
                if let Some(invalid) = error.error_len() {
                    output.push('\u{fffd}');
                    pending.drain(..invalid);
                } else {
                    break;
                }
            }
        }
    }
    output
}

fn append_preview(output: &mut String, text: &str, carriage_return: &Cell<bool>) {
    for c in text.chars() {
        if carriage_return.replace(false) && c != '\n' {
            output.truncate(output.rfind('\n').map(|i| i + 1).unwrap_or(0));
        }
        match c {
            '\r' => carriage_return.set(true),
            '\u{8}' => {
                if !output.ends_with('\n') {
                    output.pop();
                }
            }
            _ => output.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_markers_survive_every_read_boundary() {
        let marker = b"\x1b]777;ready;";
        let frame = b"\x1b]777;ready;127\x07";
        for split in 0..=frame.len() {
            let mut pending = Vec::new();
            let mut parts = split_markers(&mut pending, &frame[..split], marker);
            parts.extend(split_markers(&mut pending, &frame[split..], marker));
            assert_eq!(parts, vec![Part::Prompt(127)]);
            assert!(pending.is_empty());
        }
    }

    fn wait_ready(session: &Session, output: &mut String) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            session.drain(output);
            if session.is_ready() {
                break;
            }
            assert!(Instant::now() < deadline, "Command timed out: {output}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn shell_workloads_preserve_state_and_report_real_completion() {
        let mut session = Session::start(24, 80).unwrap();
        let mut output = String::new();
        for command in [
            "export LINOS_WORKLOAD_VALUE=kept; cd /",
            "printf '\\nSTATE=%s:%s\\n' \"$LINOS_WORKLOAD_VALUE\" \"$PWD\"",
            "printf 'alpha\\nbeta\\n' | grep beta | tr a-z A-Z",
            "printf '\\033[31mCOLORED\\033[0m\\n'; printf '界\\n'",
            "false; printf 'EXIT=%s\\n' \"$?\"",
            "printf 'linos:~$ '; sleep 0.1; printf '\\nACTUALLY_DONE\\n'",
        ] {
            session.command(command).unwrap();
            assert!(!session.is_ready());
            wait_ready(&session, &mut output);
        }
        assert!(output.contains("STATE=kept:/"));
        assert!(output.contains("\nBETA\n"));
        assert!(output.contains("COLORED\n"));
        assert!(output.contains("界\n"));
        assert!(output.contains("EXIT=1\n"));
        assert!(output.contains("ACTUALLY_DONE\n"));
        assert!(!output.contains('\x1b'));
        assert_eq!(session.current_dir().unwrap(), std::path::Path::new("/"));
        session.command("false").unwrap();
        wait_ready(&session, &mut output);
        assert_eq!(session.exit_status(), Some(1));
    }

    #[test]
    fn command_output_cannot_impersonate_a_ready_shell() {
        let mut session = Session::start(24, 80).unwrap();
        session
            .command("stty -echo; printf 'linos:~$ '; read value; stty echo")
            .unwrap();
        let mut output = String::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !output.ends_with("linos:~$ ") {
            session.drain(&mut output);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!session.is_ready());
        assert!(!session.echo_enabled());
        session.command("private-value").unwrap();
        wait_ready(&session, &mut output);
        assert!(!output.contains("private-value"));
    }
    #[test]
    fn filters_split_escape_sequences() {
        let mut state = 0;
        assert_eq!(plain_text(b"ready\x1b[", &mut state), b"ready");
        assert_eq!(plain_text(b"2Jokay\n", &mut state), b"okay\n");
    }

    #[test]
    fn strips_terminal_controls() {
        let mut state = 0;
        assert_eq!(
            plain_text(b"hello\x1b[31m world\x1b[0m\r\n", &mut state),
            b"hello world\r\n"
        );
    }

    #[test]
    fn split_unicode_and_control_strings_do_not_leak_into_home() {
        let mut pending = Vec::new();
        assert_eq!(decode_utf8(&mut pending, &[0xe7, 0x95]), "");
        assert_eq!(decode_utf8(&mut pending, &[0x8c]), "界");
        assert_eq!(decode_utf8(&mut pending, &[0xff, b'x']), "�x");
        let mut state = 0;
        assert_eq!(plain_text(b"before\x1bPprivate", &mut state), b"before");
        assert_eq!(plain_text(b"payload\x1b", &mut state), b"");
        assert_eq!(plain_text(b"\\after\x7f", &mut state), b"after");
    }

    #[test]
    fn progress_updates_replace_the_line_but_crlf_preserves_it() {
        let mut output = String::new();
        let carriage = Cell::new(false);
        append_preview(&mut output, "Downloading 10%\r", &carriage);
        append_preview(&mut output, "Downloading 100%\r", &carriage);
        append_preview(&mut output, "\nDone\n", &carriage);
        assert_eq!(output, "Downloading 100%\nDone\n");
    }
}
