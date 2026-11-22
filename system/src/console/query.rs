//! Bounded, cancellable read-only command queries for Home inspection views.
use std::{
    io::{self, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy)]
pub struct Policy {
    pub timeout: Duration,
    pub max_bytes: usize,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            max_bytes: 4 * 1024 * 1024,
        }
    }
}

struct Process {
    child: Child,
    active: bool,
}
impl Process {
    fn stop_group(&self) {
        // Each query creates its own process group. Keep the leader unreaped
        // until cleanup so the group identifier cannot refer to another job.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.active {
            self.stop_group();
            let _ = self.child.wait();
        }
    }
}

fn nonblocking(fd: libc::c_int) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn read_available(
    stream: &mut impl Read,
    output: &mut Vec<u8>,
    budget: &mut usize,
) -> io::Result<bool> {
    let mut bytes = [0u8; 8192];
    // Yield between batches even when a child writes continuously.
    for _ in 0..16 {
        match stream.read(&mut bytes) {
            Ok(0) => return Ok(false),
            Ok(count) => {
                if count > *budget {
                    return Err(io::Error::new(
                        io::ErrorKind::FileTooLarge,
                        "Query output exceeded its limit; narrow the query",
                    ));
                }
                *budget -= count;
                output.extend_from_slice(&bytes[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

pub fn run(
    program: &str,
    args: &[&str],
    cancel: &Cancellation,
    policy: Policy,
) -> io::Result<Output> {
    if cancel.is_cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Query cancelled",
        ));
    }
    let started = Instant::now();
    let child = Command::new(program)
        .args(args)
        .process_group(0)
        .env("SYSTEMD_COLORS", "0")
        .env("SYSTEMD_PAGER", "cat")
        .env("PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut process = Process {
        child,
        active: true,
    };
    let mut stdout = process.child.stdout.take().unwrap();
    let mut stderr = process.child.stderr.take().unwrap();
    nonblocking(stdout.as_raw_fd())?;
    nonblocking(stderr.as_raw_fd())?;
    let (mut stdout_open, mut stderr_open) = (true, true);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut budget = policy.max_bytes;
    loop {
        if cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Query cancelled",
            ));
        }
        if started.elapsed() >= policy.timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "System query timed out; try again",
            ));
        }
        let mut descriptors = [
            libc::pollfd {
                fd: if stdout_open { stdout.as_raw_fd() } else { -1 },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if stderr_open { stderr.as_raw_fd() } else { -1 },
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let remaining = policy
            .timeout
            .saturating_sub(started.elapsed())
            .as_millis()
            .min(20) as i32;
        if unsafe { libc::poll(descriptors.as_mut_ptr(), 2, remaining) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if stdout_open && descriptors[0].revents != 0 {
            stdout_open = read_available(&mut stdout, &mut out, &mut budget)?;
        }
        if stderr_open && descriptors[1].revents != 0 {
            stderr_open = read_available(&mut stderr, &mut err, &mut budget)?;
        }
        // WNOWAIT observes exit without releasing the leader PID before its
        // process group is cleaned up, including children holding the pipes.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::waitid(
                libc::P_PID,
                process.child.id(),
                &raw mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } != 0
        {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if !stdout_open && !stderr_open && unsafe { info.si_pid() } != 0 {
            process.stop_group();
            let status = process.child.wait()?;
            process.active = false;
            return Ok(Output {
                status,
                stdout: out,
                stderr: err,
            });
        }
    }
}

pub fn text(program: &str, args: &[&str], cancel: &Cancellation) -> Result<String, String> {
    let output =
        run(program, args, cancel, Policy::default()).map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(super::pty::preview(&output.stdout));
    }
    let diagnostic = super::pty::preview(&output.stderr)
        .chars()
        .take(4000)
        .collect::<String>();
    Err(format!("{}: {}", output.status, diagnostic.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_output_that_exceeds_the_query_budget() {
        let policy = Policy {
            max_bytes: 1024,
            ..Policy::default()
        };
        let result = run(
            "/bin/sh",
            &["-c", "head -c 2048 /dev/zero"],
            &Cancellation::default(),
            policy,
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::FileTooLarge);
    }
    #[test]
    fn a_cancelled_query_does_not_execute() {
        let cancel = Cancellation::default();
        cancel.cancel();
        let result = run("/bin/true", &[], &cancel, Policy::default());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }
    #[test]
    fn queries_have_a_real_deadline() {
        let policy = Policy {
            timeout: Duration::from_millis(20),
            ..Policy::default()
        };
        let result = run(
            "/bin/sh",
            &["-c", "sleep 0.15"],
            &Cancellation::default(),
            policy,
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn reads_both_pipes_and_preserves_the_real_exit_status() {
        let output = run(
            "/bin/sh",
            &[
                "-c",
                "head -c 65536 /dev/zero; head -c 65536 /dev/zero >&2; exit 7",
            ],
            &Cancellation::default(),
            Policy::default(),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!((output.stdout.len(), output.stderr.len()), (65536, 65536));
        let result = text(
            "/bin/sh",
            &["-c", "printf '\\033[31mfailed\\033[0m\\n' >&2; exit 7"],
            &Cancellation::default(),
        )
        .unwrap_err();
        assert!(result.contains("failed"));
        assert!(result.contains('7'));
        assert!(!result.contains('\x1b'));
    }

    #[test]
    fn output_budget_is_shared_by_stdout_and_stderr() {
        let policy = Policy {
            max_bytes: 1024,
            ..Policy::default()
        };
        let result = run(
            "/bin/sh",
            &["-c", "head -c 768 /dev/zero; head -c 768 /dev/zero >&2"],
            &Cancellation::default(),
            policy,
        );
        assert_eq!(
            result.err().map(|e| e.kind()),
            Some(io::ErrorKind::FileTooLarge)
        );
    }

    #[test]
    fn cancellation_stops_descendants_even_after_the_leader_exits() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::current_dir().unwrap().join(format!(
            "target/query-child-{}-{unique}",
            std::process::id()
        ));
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let worker_path = path.clone();
        let worker = std::thread::spawn(move || {
            run(
                "/bin/sh",
                &[
                    "-c",
                    "sleep 30 & printf '%s\\n' \"$!\" > \"$1\"",
                    "query-fixture",
                    worker_path.to_str().unwrap(),
                ],
                &worker_cancel,
                Policy::default(),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let pid = loop {
            if let Some(pid) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| s.trim().parse::<i32>().ok())
            {
                break pid;
            }
            if Instant::now() >= deadline {
                cancel.cancel();
                let _ = worker.join();
                panic!("Query fixture did not start");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        cancel.cancel();
        let result = worker.join().unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(
            result.err().map(|e| e.kind()),
            Some(io::ErrorKind::Interrupted)
        );
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let alive = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .is_some_and(|s| {
                    s.rsplit_once(')')
                        .is_some_and(|(_, fields)| !fields.trim_start().starts_with('Z'))
                });
            if !alive {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Query descendant survived cancellation"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
