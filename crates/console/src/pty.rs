//! A long-lived real Linux shell connected to a pseudo-terminal.
//! Home sends complete commands to this PTY; the shell retains its cwd,
//! environment and job state. Full-screen terminal applications belong in
//! the native Shell mode until an ANSI terminal emulator is integrated.
use std::{
    env, fs::File, io::{self, Read, Write},
    os::{fd::{AsRawFd, FromRawFd}, unix::process::CommandExt},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
};

pub struct Session {
    writer: File,
    output: Receiver<Vec<u8>>,
    child: Child,
    slave_tty: File,
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
        if unsafe { libc::openpty(&raw mut master, &raw mut slave, std::ptr::null_mut(), std::ptr::null(), &size) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openpty returned unique owned descriptors.
        let writer = unsafe { File::from_raw_fd(master) };
        let slave_file = unsafe { File::from_raw_fd(slave) };
        let shell = env::var("SHELL")
            .ok()
            .filter(|path| path.starts_with('/') && std::path::Path::new(path).is_file())
            .unwrap_or_else(|| "/bin/sh".into());
        let mut cmd = Command::new(shell);
        // The embedded shell must never source user startup files that can
        // automatically launch Linos again. Native Shell mode keeps .bashrc.
        if cmd.get_program().to_string_lossy().ends_with("/bash") {
            cmd.args(["--noprofile", "--norc"]);
        }
        cmd.arg("-i")
            .env("LINOS_NO_WELCOME", "1")
            .env("LINOS_EMBEDDED_PTY", "1")
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave_file.try_clone()?))
            .stdout(Stdio::from(slave_file.try_clone()?))
            .stderr(Stdio::from(slave_file.try_clone()?));
        // SAFETY: pre_exec runs in the child after fork. Use only
        // async-signal-safe libc functions to establish controlling TTY.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn()?;
        let slave_tty = slave_file;
        let mut reader = writer.try_clone()?;
        let (tx, output) = mpsc::sync_channel(128);
        thread::spawn(move || {
            let mut data = [0u8; 8192];
            let mut escape_state = 0u8;
            while let Ok(len) = reader.read(&mut data) {
                if len == 0 { break; }
                let clean = plain_text(&data[..len], &mut escape_state);
                if !clean.is_empty() && tx.send(clean).is_err() { break; }
            }
        });
        Ok(Self { writer, output, child, slave_tty })
    }

    pub fn current_dir(&self) -> Option<std::path::PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.child.id())).ok()
    }

    pub fn command(&mut self, command: &str) -> io::Result<()> {
        self.writer.write_all(command.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    pub fn key(&mut self, byte: u8) -> io::Result<()> {
        self.writer.write_all(&[byte])?;
        self.writer.flush()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        let size = libc::winsize { ws_row: rows.max(2), ws_col: cols.max(20), ws_xpixel: 0, ws_ypixel: 0 };
        // SAFETY: ioctl only reads this initialized struct; writer owns a PTY.
        if unsafe { libc::ioctl(std::os::fd::AsRawFd::as_raw_fd(&self.writer), libc::TIOCSWINSZ, &size) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn echo_enabled(&self) -> bool {
        // Read the PTY slave's termios, not the master. On failure, never
        // invent a password prompt simply because the query failed.
        let mut termios: libc::termios = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::tcgetattr(self.slave_tty.as_raw_fd(), &raw mut termios) };
        result != 0 || (termios.c_lflag & libc::ECHO != 0)
    }

    pub fn drain(&self, output: &mut String) {
        while let Ok(bytes) = self.output.try_recv() {
            output.push_str(&String::from_utf8_lossy(&bytes));
            if output.len() > 32_000 {
                let mut boundary = output.len() - 24_000;
                while !output.is_char_boundary(boundary) { boundary -= 1; }
                output.drain(..boundary);
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// Read-only output preview, not a terminal emulator. Keeps command content
// while avoiding escape sequences that might corrupt Ratatui rendering.
fn plain_text(bytes: &[u8], state: &mut u8) -> Vec<u8> {
    let mut clean = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        match *state {
            0 if byte == 0x1b => *state = 1,
            0 if byte == b'\r' => {},
            0 if byte == b'\n' || byte == b'\t' || byte >= 0x20 => clean.push(byte),
            1 if byte == b'[' => *state = 2,
            1 if byte == b']' => *state = 3,
            1 => *state = 0,
            2 if (0x40..=0x7e).contains(&byte) => *state = 0,
            3 if byte == 0x07 => *state = 0,
            3 if byte == 0x1b => *state = 4,
            4 => *state = if byte == b'\\' { 0 } else { 3 },
            _ => {},
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_split_escape_sequences() {
        let mut state = 0;
        assert_eq!(plain_text(b"ready\x1b[", &mut state), b"ready");
        assert_eq!(plain_text(b"2Jokay\n", &mut state), b"okay\n");
    }

    #[test]
    fn strips_terminal_controls() {
        let mut state = 0;
        assert_eq!(plain_text(b"hello\\x1b[31m world\\x1b[0m\\r\\n", &mut state), b"hello world\\n");
    }
}
