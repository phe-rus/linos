//! Query the real terminal emulator for its window geometry.
//! Serial TTYs (including QEMU ttyS0) don't receive host SIGWINCH. Modern
//! terminal emulators can answer CSI 18 t with CSI 8 ; rows ; cols t.
//! This is best-effort; the existing TTY size remains the fallback.
use std::{
    io::{self, Write},
    os::fd::AsRawFd,
};

pub fn negotiate() -> io::Result<bool> {
    let fd = io::stdin().as_raw_fd();
    // SAFETY: the fd is our own stdin, and libc initializes saved termios.
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &raw mut saved) } != 0 {
        return Ok(false);
    }
    let mut raw = saved;
    // SAFETY: raw is a local valid termios value.
    unsafe { libc::cfmakeraw(&raw mut raw) };
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return Ok(false);
    }

    let response = (|| -> io::Result<Vec<u8>> {
        io::stdout().write_all(b"\x1b[18t")?;
        io::stdout().flush()?;
        let mut data = Vec::new();
        let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        while data.len() < 96 {
            // SAFETY: poll reads the initialized pollfd and does not outlive it.
            let ready = unsafe { libc::poll(&raw mut poll, 1, 180) };
            if ready <= 0 || poll.revents & libc::POLLIN == 0 {
                break;
            }
            let mut buffer = [0u8; 96];
            // SAFETY: the buffer is valid and the TTY is in raw mode.
            let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count <= 0 { break; }
            data.extend_from_slice(&buffer[..count as usize]);
            if data.ends_with(b"t") { break; }
        }
        Ok(data)
    })();
    // SAFETY: always restore the original terminal mode, including on errors.
    let restored = unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
    if restored != 0 { return Err(io::Error::last_os_error()); }
    let response = response?;
    if let Some((rows, cols)) = parse_report(&response) {
        let win = libc::winsize {
            ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0,
        };
        // SAFETY: win is valid, and TIOCSWINSZ accepts the TTY's window size.
        if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &win) } == 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_report(bytes: &[u8]) -> Option<(u16, u16)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let start = text.find("\x1b[8;")? + 4;
    let digits = text.get(start..)?.split_once('t')?.0;
    let (rows, cols) = digits.split_once(';')?;
    let rows = rows.parse::<u16>().ok()?;
    let cols = cols.parse::<u16>().ok()?;
    (rows >= 14 && cols >= 36 && rows <= 500 && cols <= 1000)
        .then_some((rows, cols))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognises_xterm_window_reports() {
        assert_eq!(parse_report(b"\x1b[8;42;132t"), Some((42, 132)));
        assert_eq!(parse_report(b"\x1b[8;0;0t"), None);
        assert_eq!(parse_report(b"\x1b[24;80t"), None);
    }
}
