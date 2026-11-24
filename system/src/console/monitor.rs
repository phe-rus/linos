//! Linux resource samples. Percentages come from counter deltas, never load average.
use std::{
    collections::BTreeMap,
    fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

#[derive(Clone, Copy, Debug)]
struct Cpu {
    total: u64,
    idle: u64,
}
impl Cpu {
    fn parse(text: &str) -> Option<Self> {
        let fields = text
            .lines()
            .next()?
            .strip_prefix("cpu ")?
            .split_whitespace()
            .take(8)
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if fields.len() < 4 {
            return None;
        }
        Some(Self {
            total: fields.iter().sum(),
            idle: fields[3] + fields.get(4).copied().unwrap_or(0),
        })
    }
    fn delta(self, previous: Self) -> Option<(u64, u64)> {
        let total = self.total.checked_sub(previous.total)?;
        let idle = self.idle.checked_sub(previous.idle)?;
        (total > 0 && idle <= total).then_some((total - idle, total))
    }
}

#[derive(Clone, Debug)]
pub struct Task {
    pub pid: i32,
    pub name: String,
    pub state: char,
    pub resident: u64,
    pub cpu: Option<f64>,
    pub start: u64,
    ticks: u64,
}

impl Task {
    fn parse(pid: i32, stat: &str, page_size: u64) -> Option<Self> {
        let begin = stat.find('(')?;
        let end = stat.rfind(')')?;
        let values: Vec<_> = stat.get(end + 2..)?.split_whitespace().collect();
        let name = stat
            .get(begin + 1..end)?
            .chars()
            .filter(|c| !c.is_control())
            .take(40)
            .collect();
        Some(Self {
            pid,
            name,
            state: values.first()?.chars().next()?,
            ticks: values
                .get(11)?
                .parse::<u64>()
                .ok()?
                .checked_add(values.get(12)?.parse::<u64>().ok()?)?,
            start: values.get(19)?.parse().ok()?,
            resident: values
                .get(21)?
                .parse::<u64>()
                .ok()?
                .saturating_mul(page_size),
            cpu: None,
        })
    }
}

pub struct Monitor {
    previous: Option<Cpu>,
    cpu: Option<(u64, u64)>,
    pub tasks: Vec<Task>,
    samples: u64,
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            previous: None,
            cpu: None,
            tasks: Vec::new(),
            samples: 0,
        }
    }

    pub fn sample(&mut self) {
        let next = fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| Cpu::parse(&s));
        self.cpu = next
            .zip(self.previous)
            .and_then(|(new, old)| new.delta(old));
        self.previous = next;
        let previous: BTreeMap<_, _> = self
            .tasks
            .iter()
            .map(|task| (task.pid, (task.start, task.ticks)))
            .collect();
        self.tasks.clear();
        // SAFETY: sysconf has no pointer arguments; a failed query uses the Linux default.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let page = if page > 0 { page as u64 } else { 4096 };
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<i32>().ok())
                else {
                    continue;
                };
                let Some(mut task) = fs::read_to_string(entry.path().join("stat"))
                    .ok()
                    .and_then(|s| Task::parse(pid, &s, page))
                else {
                    continue;
                };
                if let Some((_, total)) = self.cpu
                    && let Some((start, ticks)) = previous.get(&pid)
                    && *start == task.start
                    && let Some(delta) = task.ticks.checked_sub(*ticks)
                {
                    task.cpu = Some((delta as f64 * 100.0 / total as f64).min(100.0));
                }
                self.tasks.push(task);
            }
        }
        self.tasks
            .sort_by(|a, b| b.resident.cmp(&a.resident).then(a.pid.cmp(&b.pid)));
        self.samples = self.samples.saturating_add(1);
    }

    pub fn resources(&self) -> String {
        let cpu = self
            .cpu
            .map(|(used, total)| bar(used, total, 24))
            .unwrap_or_else(|| {
                if self.previous.is_some() {
                    "Measuring, waiting for next sample".into()
                } else {
                    "Unavailable: CPU counters could not be read".into()
                }
            });
        let memory = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let field = |key| {
            memory
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<u64>().ok())
        };
        let capacity = |total: Option<u64>, available: Option<u64>| match total.zip(available) {
            Some((0, _)) => "Not configured".into(),
            Some((total, available)) if available <= total => format!(
                "{}\n{} / {} MiB used, {} MiB available",
                bar(total - available, total, 24),
                (total - available) / 1024,
                total / 1024,
                available / 1024
            ),
            _ => "Unavailable".into(),
        };
        let load = fs::read_to_string("/proc/loadavg")
            .ok()
            .map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join("  "))
            .unwrap_or_else(|| "Unavailable".into());
        let disk = disk_usage()
            .map(|(used, total, available)| {
                format!(
                    "{}\n{} / {} MiB used, {} MiB available",
                    bar(used, total, 24),
                    used / 1024 / 1024,
                    total / 1024 / 1024,
                    available / 1024 / 1024
                )
            })
            .unwrap_or_else(|_| "Unavailable".into());
        format!(
            "CPU activity, whole machine\n{cpu}\nLoad (1 / 5 / 15 min): {load}\n\nMemory\n{}\n\nSwap\n{}\n\nStorage, root filesystem\n{disk}\n\n{} visible tasks. /tasks to inspect and control.\nCPU uses measured counter changes. Load counts runnable/waiting work.\nRefresh uses your Settings interval. Reserved disk space is excluded from available.",
            capacity(field("MemTotal:"), field("MemAvailable:")),
            capacity(field("SwapTotal:"), field("SwapFree:")),
            self.tasks.len()
        )
    }

    pub fn display(&self, selected: usize, limit: usize) -> String {
        let mut text = format!(
            "{} visible tasks, sorted by memory\nCPU is share of whole machine, measured between samples.\n\n   PID    CPU     MiB  State  Name\n",
            self.tasks.len()
        );
        let first = selected.saturating_sub(limit.saturating_sub(1));
        for (index, task) in self.tasks.iter().enumerate().skip(first).take(limit) {
            let cpu = task
                .cpu
                .map(|n| format!("{n:5.1}%"))
                .unwrap_or_else(|| "    --".into());
            text.push_str(&format!(
                "{} {:>6} {cpu} {:>7}    {}    {}\n",
                if index == selected { '›' } else { ' ' },
                task.pid,
                task.resident / 1024 / 1024,
                task.state,
                task.name
            ));
        }
        if let Some(task) = self.tasks.get(selected) {
            text.push_str(&format!("\nSelected: {} (PID {})\nUp/Down select. /tasks pause|resume|terminate|kill {}\nR running, S sleeping, D waiting for I/O, T stopped, Z exited.\nActions ask for confirmation. Kernel permissions apply.", task.name, task.pid, task.pid));
        }
        text.push_str(&format!(
            "\nSnapshot {}. Tasks can exit between refreshes.",
            self.samples
        ));
        text
    }
}

pub fn bar(used: u64, total: u64, width: usize) -> String {
    if total == 0 {
        return "Not configured".into();
    }
    let used = used.min(total) as u128;
    let filled = (used * width as u128 / total as u128) as usize;
    format!(
        "[{}{}] {:>3}%",
        "█".repeat(filled),
        "░".repeat(width - filled),
        used * 100 / total as u128
    )
}

pub(super) fn disk_usage() -> io::Result<(u64, u64, u64)> {
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: the path is NUL terminated and stat points to writable storage.
    if unsafe { libc::statvfs(c"/".as_ptr(), &raw mut stat) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let unit = stat.f_frsize;
    Ok((
        (stat.f_blocks - stat.f_bfree).saturating_mul(unit),
        stat.f_blocks.saturating_mul(unit),
        stat.f_bavail.saturating_mul(unit),
    ))
}

pub fn signal(task: &Task, operation: &str) -> io::Result<()> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidInput, message);
    let signal = match operation {
        "pause" => libc::SIGSTOP,
        "resume" => libc::SIGCONT,
        "terminate" => libc::SIGTERM,
        "kill" => libc::SIGKILL,
        _ => return Err(invalid("Use pause, resume, terminate or kill")),
    };
    // Never allow the console to stop itself, PID 1 or its ancestor sessions.
    let mut ancestor = std::process::id() as i32;
    while ancestor > 1 {
        if task.pid == ancestor {
            return Err(invalid("Cannot control Home or its parent session"));
        }
        ancestor = fs::read_to_string(format!("/proc/{ancestor}/status"))
            .ok()
            .and_then(|s| {
                s.lines().find_map(|line| {
                    line.strip_prefix("PPid:")
                        .and_then(|s| s.trim().parse().ok())
                })
            })
            .unwrap_or(0);
    }
    if task.pid <= 1 {
        return Err(invalid("Cannot control the system init process"));
    }
    // Open a stable process handle before rechecking identity. A recycled PID
    // must never receive a signal intended for the task the user reviewed.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, task.pid, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    let stat = fs::read_to_string(format!("/proc/{}/stat", task.pid))?;
    let current =
        Task::parse(task.pid, &stat, 1).ok_or_else(|| invalid("Task identity unavailable"))?;
    if current.start != task.start {
        return Err(invalid("Task has exited or changed, refresh and try again"));
    }
    if unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            fd.as_raw_fd(),
            signal,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_uses_deltas_and_does_not_double_count_guest_time() {
        let before = Cpu::parse("cpu  100 0 30 500 20 0 0 0 40 0").unwrap();
        let after = Cpu::parse("cpu  120 0 40 550 40 0 0 0 50 0").unwrap();
        assert_eq!(after.delta(before), Some((30, 100)));
        assert_eq!(after.delta(after), None);
        assert_eq!(before.delta(after), None);
        assert!(Cpu::parse("cpu broken").is_none());
    }
    #[test]
    fn task_names_can_contain_spaces_and_parentheses() {
        let stat = "12 (worker (busy)) S 1 0 0 0 0 0 0 0 0 0 7 3 0 0 0 0 1 0 99 8192 2";
        let task = Task::parse(12, stat, 4096).unwrap();
        assert_eq!(task.name, "worker (busy)");
        assert_eq!((task.ticks, task.start, task.resident), (10, 99, 8192));
    }
    #[test]
    fn live_samples_and_control_use_real_processes() {
        let mut monitor = Monitor::new();
        monitor.sample();
        assert!(monitor.resources().contains("waiting for next sample"));
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        monitor.sample();
        let task = monitor
            .tasks
            .iter()
            .find(|t| t.pid == child.id() as i32)
            .unwrap()
            .clone();
        let result = (|| {
            let mut stale = task.clone();
            stale.start += 1;
            assert!(signal(&stale, "terminate").is_err());
            signal(&task, "pause")?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            loop {
                let stat = fs::read_to_string(format!("/proc/{}/stat", task.pid))?;
                if Task::parse(task.pid, &stat, 1).is_some_and(|t| t.state == 'T') {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "Task did not stop");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            signal(&task, "resume")?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            loop {
                let stat = fs::read_to_string(format!("/proc/{}/stat", task.pid))?;
                if Task::parse(task.pid, &stat, 1).is_some_and(|t| t.state != 'T') {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "Task did not resume");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            signal(&task, "terminate")
        })();
        let _ = child.kill();
        let _ = child.wait();
        result.unwrap();
        let own = monitor
            .tasks
            .iter()
            .find(|t| t.pid == std::process::id() as i32)
            .unwrap();
        assert!(signal(own, "pause").is_err());
    }
    #[test]
    fn bars_are_bounded_even_for_overflow_sized_values() {
        assert!(bar(u64::MAX, u64::MAX, 24).ends_with("100%"));
        assert!(bar(0, 100, 24).ends_with("  0%"));
        assert!(bar(1, 0, 24).contains("Not configured"));
    }
}
