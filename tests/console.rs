//! Drive the compiled Home through a real PTY and inspect assembled VT screens.
//! The ignored guest test uses only a disposable image explicitly supplied by the caller.
use std::{
    fs::{self, File},
    io::{Read, Write},
    num::NonZeroU16,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Console {
    child: Child,
    master: File,
    parser: vt100::Parser,
    output: Vec<u8>,
    query: Vec<u8>,
    directory: PathBuf,
    redactions: Vec<String>,
}

impl Console {
    fn start(mut command: Command, label: &str) -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/console-qualification");
        fs::create_dir_all(&directory).unwrap();
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: 32,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &raw mut master,
                    &raw mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &size,
                )
            },
            0
        );
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        command
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        let mut console = Self {
            child,
            master,
            parser: vt100::Parser::new(
                NonZeroU16::new(32).unwrap(),
                NonZeroU16::new(100).unwrap(),
                0,
            ),
            output: Vec::new(),
            query: Vec::new(),
            directory: directory.join(label),
            redactions: vec!["ui-private-test".into()],
        };
        fs::create_dir_all(&console.directory).unwrap();
        console.pump(Duration::from_millis(100));
        console
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
        self.master.flush().unwrap();
    }

    fn pump(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            let mut poll = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&raw mut poll, 1, 20) } <= 0 {
                continue;
            }
            let mut bytes = [0u8; 65536];
            let Ok(count) = self.master.read(&mut bytes) else {
                return;
            };
            if count == 0 {
                return;
            }
            self.parser.process(&bytes[..count]);
            self.output.extend_from_slice(&bytes[..count]);
            self.query.extend_from_slice(&bytes[..count]);
            if self.query.windows(5).any(|s| s == b"\x1b[18t") {
                self.send(b"\x1b[8;32;100t");
            }
            let keep = self.query.len().saturating_sub(4);
            self.query.drain(..keep);
        }
    }

    fn wait(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut last_change = Instant::now();
        loop {
            let previous = self.output.len();
            self.pump(Duration::from_millis(40));
            if self.output.len() != previous {
                last_change = Instant::now();
            }
            let screen = self.parser.screen().contents();
            // Serial output can deliver the title well before the rest of the
            // frame. Wait for a quiet interval before inspecting or typing.
            if screen.contains(text) && last_change.elapsed() >= Duration::from_millis(80) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Timed out waiting for {text:?}\n{}",
                self.redact(&screen)
            );
        }
    }

    fn submit(&mut self, command: &str) {
        self.pump(Duration::from_millis(120));
        self.send(command.as_bytes());
        self.send(b"\r");
        self.pump(Duration::from_millis(120));
    }

    fn wait_for_loaded_view(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(12);
        while self
            .parser
            .screen()
            .contents()
            .contains("Reading system information")
        {
            self.pump(Duration::from_millis(50));
            assert!(
                Instant::now() < deadline,
                "System view did not finish loading"
            );
        }
    }

    fn snapshot(&self, name: &str) {
        fs::write(
            self.directory.join(format!("{name}.txt")),
            self.redact(&self.parser.screen().contents()),
        )
        .unwrap();
    }

    fn redact(&self, text: &str) -> String {
        self.redactions
            .iter()
            .fold(text.to_owned(), |text, secret| {
                text.replace(secret, "<REDACTED>")
            })
    }

    fn exercise(&mut self) {
        self.wait("Welcome back");
        for (route, title) in [
            ("/status", "System overview"),
            ("/assets", "Assets"),
            ("/network", "Network interfaces"),
            ("/services", "Running services"),
            ("/logs", "System logs"),
            ("/hardware", "Hardware"),
            ("/store", "Linus store"),
            ("/updates", "Updates"),
            ("/help", "Help and shortcuts"),
        ] {
            self.submit(route);
            self.wait(title);
            self.wait_for_loaded_view();
            self.snapshot(route.trim_start_matches('/'));
        }
        self.submit("/resources");
        self.wait("CPU activity, whole machine");
        self.pump(Duration::from_millis(2200));
        assert!(
            !self
                .parser
                .screen()
                .contents()
                .contains("waiting for next sample")
        );
        self.snapshot("resources");
        self.submit("/tasks");
        self.wait("visible tasks, sorted by memory");
        self.snapshot("tasks");
        self.submit("/settings");
        self.wait("Your preferences");
        for (command, expected) in [
            ("/settings refresh 1", "Saved refresh: 1"),
            ("/settings color never", "Saved color: never"),
            ("/settings default shell", "Saved default: shell"),
        ] {
            self.submit(command);
            self.wait(expected);
        }
        self.snapshot("settings");
        self.submit("/settings hostname console-test-cancelled");
        self.wait("Confirm action");
        self.snapshot("confirmation");
        self.send(b"\r");
        self.wait("Cancelled");
        self.submit("/home");
        self.wait("Welcome back");
        self.send(b"printf '%s%s\\n' EDIT_ OXX\x7f\x7fK\r");
        self.wait("EDIT_OK");
        self.submit("printf '%s%s\\n' PIPE_ OK | cat");
        self.wait("PIPE_OK");
        self.send(b"printf '%s%s\\n' CURSOR_ OXK\x1b[D\x7f\x1b[F\r");
        self.wait("CURSOR_OK");
        self.send(b"\x0c");
        self.pump(Duration::from_millis(150));
        assert!(!self.parser.screen().contents().contains("CURSOR_OK"));
        self.send(b"\x1b[A\r");
        self.wait("CURSOR_OK");
        self.submit("stty -echo; printf '%s%s' SECRET_ PROMPT; read value; stty echo; printf '\\n%s%s\\n' SECRET_ ACCEPTED");
        self.wait("SECRET_PROMPT");
        self.send(b"ui-private-test");
        self.wait("Password (hidden)");
        assert!(!self.parser.screen().contents().contains("ui-private-test"));
        self.snapshot("hidden-input");
        self.send(b"\r");
        self.wait("SECRET_ACCEPTED");
        self.submit("clear");
        self.submit("printf '%s%s\\n' AFTER_CLEAR_ OK");
        self.wait("AFTER_CLEAR_OK");
        self.submit("false");
        self.wait("Command failed · exit 1");
        self.submit("/home");
        self.wait("Welcome back");
        self.submit("sleep 60 & printf 'TASK_PID=%s\\n' \"$!\"");
        let deadline = Instant::now() + Duration::from_secs(12);
        let pid = loop {
            self.pump(Duration::from_millis(50));
            let screen = self.parser.screen().contents();
            if let Some(pid) = screen.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("TASK_PID=")
                    .and_then(|s| s.parse::<u32>().ok())
            }) {
                break pid;
            }
            assert!(
                Instant::now() < deadline,
                "Task PID not displayed\n{screen}"
            );
        };
        for operation in ["pause", "resume", "terminate"] {
            self.submit(&format!("/tasks {operation} {pid}"));
            self.wait("Confirm action");
            self.send(b"\x19");
            self.pump(Duration::from_millis(150));
            assert!(
                self.parser.screen().contents().contains("Confirm action"),
                "Ctrl+Y approved a task action"
            );
            self.send(b"y");
            self.wait(&format!("{operation} signal sent"));
        }
        self.submit("/shell sleep 30");
        self.pump(Duration::from_millis(400));
        self.send(b"\x03");
        self.wait("Native command ended");
        self.submit("/shell printf '%s%s\\n' NATIVE_ OK");
        // Home restores the alternate screen quickly. The native output lives
        // in the normal screen, so check the actual output stream as well.
        self.wait("Returned to Home");
        assert!(
            self.output
                .windows(b"NATIVE_OK".len())
                .any(|s| s == b"NATIVE_OK")
        );
        assert!(
            !self
                .output
                .windows(b"ui-private-test".len())
                .any(|s| s == b"ui-private-test")
        );
        self.snapshot("home-return");
        self.submit("/logout");
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        // Every child belongs to this harness. Never inspect or stop other VMs.
        let _ = fs::write(
            self.directory.join("terminal.log"),
            self.redact(&String::from_utf8_lossy(&self.output)),
        );
        let _ = fs::write(
            self.directory.join("last-screen.txt"),
            self.redact(&self.parser.screen().contents()),
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn home_through_real_terminal() {
    let home = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/console-qualification/home-{}",
        std::process::id()
    ));
    fs::create_dir_all(&home).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_linos"));
    command
        .env("TERM", "xterm-256color")
        .env("HOME", &home)
        .env("USER", "linostest")
        .env("SHELL", "/bin/bash");
    let mut console = Console::start(command, "host");
    console.exercise();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = console.child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(20));
            break;
        }
        assert!(Instant::now() < deadline, "Logout did not exit");
        console.pump(Duration::from_millis(20));
    }
    for (name, expected) in [
        ("refresh-seconds", "1"),
        ("color", "never"),
        ("default-mode", "shell"),
    ] {
        assert_eq!(
            fs::read_to_string(home.join(".config/linos").join(name))
                .unwrap()
                .trim(),
            expected
        );
    }
    fs::remove_dir_all(home).unwrap();
}

fn disposable_guest(label: &str) -> Console {
    let guest = std::env::var_os("LINOS_CONSOLE_GUEST_DIR")
        .expect("Set LINOS_CONSOLE_GUEST_DIR to a disposable image directory");
    let guest = PathBuf::from(guest).canonicalize().unwrap();
    let image = guest.join("disk.ext4");
    assert!(image.is_file());
    let base = PathBuf::from(
        std::env::var_os("LINOS_CONSOLE_BOOT_DIR")
            .expect("Set LINOS_CONSOLE_BOOT_DIR to the matching boot artifact directory"),
    )
    .canonicalize()
    .unwrap();
    let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .canonicalize()
        .unwrap();
    assert!(
        guest.starts_with(target),
        "Guest image must be a disposable copy under target/"
    );
    let mut command = Command::new("qemu-system-x86_64");
    command.args(["-machine", "pc,accel=kvm", "-cpu", "host", "-smp", "1", "-m", "1024",
        "-display", "none", "-no-reboot", "-monitor", "none",
        "-chardev", "stdio,id=console,signal=off", "-serial", "chardev:console", "-net", "none", "-kernel"])
        .arg(base.join("boot/kernel")).arg("-initrd").arg(base.join("boot/initramfs"))
        .arg("-drive").arg(format!("file={},if=none,format=raw,id=rootfs", image.display()))
        .args(["-device", "virtio-blk-pci,drive=rootfs", "-append",
            "root=/dev/vda rootwait rw console=ttyS0,115200n8 quiet loglevel=3 systemd.show_status=no systemd.mask=linos-enroll.service systemd.mask=serial-getty@ttyS0.service systemd.debug_shell=ttyS0"]);
    let mut console = Console::start(command, label);
    console.wait("root@");
    console.pump(Duration::from_millis(1500));
    // Fresh images receive this through the Linos package dependency. Older
    // qualification images need the matching dependency staged while offline.
    console.submit("if ! dpkg-query -W -f='${Status}' libnss-myhostname 2>/dev/null | grep -q '^install ok installed$'; then dpkg -i /root/linos-test-nss.deb; fi; dpkg-query -W -f='${Status}' libnss-myhostname 2>/dev/null | grep -q '^install ok installed$' && printf '%s%s\\n' NSS_ READY");
    console.wait("NSS_READY");
    console
        .submit("dpkg -i /root/linos-test-release.deb && printf '%s%s\\n' RELEASE_PACKAGE_ READY");
    console.wait("RELEASE_PACKAGE_READY");
    console
}

#[test]
#[ignore = "requires an explicit disposable AMD64 image and KVM; run sequentially"]
fn home_in_disposable_guest() {
    let mut console = disposable_guest("guest");
    console.submit("stty rows 32 cols 100; export TERM=xterm-256color HOME=/root USER=root SHELL=/bin/bash; /usr/bin/linos");
    console.exercise();
    console.wait("root@");
    console.submit("/usr/bin/linos");
    console.wait("Welcome back");
    for (command, expected) in [
        (
            "/settings hostname linos-console-test",
            "linos-console-test",
        ),
        ("/settings timezone Africa/Nairobi", "Africa/Nairobi"),
    ] {
        console.submit(command);
        console.wait("Confirm action");
        console.send(b"y");
        console.wait("Command completed · exit 0");
        console.submit("/settings");
        console.wait(expected);
    }
    console.snapshot("system-settings-applied");
    console.submit("/logout");
    console.wait("root@");
    console.submit("test \"$(cat /root/.config/linos/refresh-seconds)\" = 1 && test \"$(hostname)\" != console-test-cancelled && printf '%s%s\\n' GUEST_SETTINGS_ OK");
    console.wait("GUEST_SETTINGS_OK");
    console.submit("poweroff");
    let deadline = Instant::now() + Duration::from_secs(15);
    while console.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "Guest did not shut down");
        console.pump(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "requires a fresh disposable AMD64 image and KVM; run sequentially"]
fn ordinary_administrator_authorization_in_guest() {
    let mut console = disposable_guest("guest-authorization");
    let mut entropy = [0u8; 18];
    File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut entropy)
        .unwrap();
    let password: String = entropy.iter().map(|b| format!("{b:02x}")).collect();
    console.redactions.push(password.clone());
    // Password provisioning happens only inside the disposable guest, with
    // terminal echo disabled before any credential crosses the PTY.
    console.submit("stty -echo; printf '%s%s\\n' FIXTURE_ READY");
    console.wait("FIXTURE_READY");
    console.submit(&format!("useradd -m -s /bin/bash -G sudo,adm linosqa && printf '%s\\n' 'linosqa:{password}' | chpasswd; printf '%s%s\\n' ACCOUNT_ READY"));
    console.wait("ACCOUNT_READY");
    console.submit("sleep 300 & printf 'ROOT_TASK=%s\\n' \"$!\"");
    console.wait("ROOT_TASK=");
    let root_pid: u32 = console
        .parser
        .screen()
        .contents()
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("ROOT_TASK=")
                .and_then(|s| s.parse().ok())
        })
        .unwrap();
    console.submit("stty echo; stty rows 32 cols 100; runuser -u linosqa -- env TERM=xterm-256color HOME=/home/linosqa USER=linosqa SHELL=/bin/bash /usr/bin/linos");
    console.wait("Welcome back, linosqa.");
    console.submit("id -u");
    console.wait("Command completed · exit 0");
    assert!(
        !console
            .parser
            .screen()
            .contents()
            .lines()
            .any(|line| line.trim() == "0")
    );
    console.submit(&format!("/tasks terminate {root_pid}"));
    console.wait("Confirm action");
    console.send(b"y");
    console.wait("Task action failed:");
    console.snapshot("cross-user-task-denied");

    console.submit("/settings hostname should-not-apply");
    console.wait("Confirm action");
    console.send(b"y");
    console.wait("Password (hidden)");
    console.send(b"\x03");
    console.wait("Command failed");
    console.submit("printf '%s%s\\n' AFTER_CANCEL_ OK");
    console.wait("AFTER_CANCEL_OK");
    console.submit("/settings");
    console.wait("Your preferences");
    assert!(
        !console
            .parser
            .screen()
            .contents()
            .contains("should-not-apply")
    );

    for (command, expected) in [
        ("/settings hostname linos-authorized", "linos-authorized"),
        ("/settings timezone Africa/Nairobi", "Africa/Nairobi"),
    ] {
        console.submit("sudo -k");
        console.wait("Command completed · exit 0");
        assert!(
            !console
                .parser
                .screen()
                .contents()
                .contains("unable to resolve host"),
            "Changing hostname broke local hostname resolution"
        );
        console.submit(command);
        console.wait("Confirm action");
        console.send(b"y");
        console.wait("Password (hidden)");
        console.send(password.as_bytes());
        console.pump(Duration::from_millis(120));
        assert!(!console.parser.screen().contents().contains(&password));
        console.send(b"\r");
        console.wait("Command completed · exit 0");
        console.submit("printf '%s%s\\n' AFTER_SUDO_ OK");
        console.wait("AFTER_SUDO_OK");
        console.submit("/settings");
        console.wait(expected);
    }
    console.snapshot("authorized-settings");
    console.submit("/logout");
    console.wait("root@");
    console.submit(&format!("kill -0 {root_pid} && printf '%s%s\\n' ROOT_TASK_ ALIVE; printf 'AUTH_HOST=%s\\n' \"$(hostname)\"; printf 'AUTH_ZONE=%s\\n' \"$(readlink -f /etc/localtime)\""));
    console.wait("ROOT_TASK_ALIVE");
    console.wait("AUTH_HOST=linos-authorized");
    console.wait("AUTH_ZONE=/usr/share/zoneinfo/Africa/Nairobi");
    console
        .submit("getent ahosts \"$(hostname)\" >/dev/null && printf '%s%s\\n' HOST_RESOLUTION_ OK");
    console.wait("HOST_RESOLUTION_OK");
    assert!(
        !console
            .output
            .windows(password.len())
            .any(|s| s == password.as_bytes()),
        "Credential exposed in terminal output"
    );
    console.submit("poweroff");
    let deadline = Instant::now() + Duration::from_secs(15);
    while console.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "Guest did not shut down");
        console.pump(Duration::from_millis(100));
    }
}
