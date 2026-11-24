//! Observed status with explicit uncertainty, never an overall health certificate.
use super::{monitor, query};
use std::fs;

fn memory_headroom(text: &str) -> Option<(u64, u64)> {
    let field = |key| {
        text.lines().find_map(|line| {
            line.strip_prefix(key)?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    (total > 0 && available <= total).then_some((available, total))
}

fn pressure(text: &str) -> Option<f64> {
    let line = text.lines().find(|line| line.starts_with("some "))?;
    let value = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))?
        .parse::<f64>()
        .ok()?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

pub fn pressure_report() -> String {
    let mut report =
        String::from("Resource pressure, last 10 seconds (time waiting, not utilization)");
    for (name, path) in [
        ("CPU", "/proc/pressure/cpu"),
        ("Memory", "/proc/pressure/memory"),
        ("I/O", "/proc/pressure/io"),
    ] {
        let value = fs::read_to_string(path)
            .ok()
            .and_then(|text| pressure(&text));
        report.push_str(&match value {
            Some(value) => format!("\n{name}: {value:.2}%"),
            None => format!("\n{name}: unavailable"),
        });
    }
    report
}

fn failed_services(result: std::io::Result<std::process::Output>) -> String {
    match result {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<_> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            if lines.is_empty() {
                return "Failed system services: none reported by systemd".into();
            }
            let mut report = format!("Failed system services: {}\n", lines.len());
            for line in lines.iter().take(12) {
                report.extend(line.chars().filter(|c| !c.is_control()).take(200));
                report.push('\n');
            }
            report.push_str("Inspect with systemctl --failed, then journalctl -u UNIT.\nA failed service is not automatically safe to restart.");
            report
        }
        Ok(_) => "Failed system services: unavailable (systemd inspection failed)".into(),
        Err(error) => format!("Failed system services: unavailable ({error})"),
    }
}

pub fn report(cancel: &query::Cancellation) -> String {
    let memory = fs::read_to_string("/proc/meminfo").ok()
        .and_then(|text| memory_headroom(&text))
        .map(|(available, total)| format!(
            "Memory available: {} / {} MiB{}", available / 1024, total / 1024,
            if u128::from(available) * 100 <= u128::from(total) * 10 {
                "\nLow memory headroom (10% or less). Inspect /resources and /tasks before starting more work."
            } else { "" }
        )).unwrap_or_else(|| "Memory available: unavailable".into());
    let disk = monitor::disk_usage().ok().filter(|(_, total, available)| *total > 0 && available <= total)
        .map(|(_, total, available)| format!(
            "Root storage available: {} / {} MiB{}", available / 1024 / 1024, total / 1024 / 1024,
            if u128::from(available) * 100 <= u128::from(total) * 5 {
                "\nLow root space (5% or less). Review data and backups before an update; no files were removed."
            } else { "" }
        )).unwrap_or_else(|| "Root storage available: unavailable".into());
    let services = failed_services(query::run(
        "/usr/bin/systemctl",
        &[
            "--failed",
            "--type=service",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
        cancel,
        query::Policy::default(),
    ));
    format!(
        "Status snapshot, refresh by reopening /status\n\n{memory}\n{disk}\n\n{}\n\n{services}\n\nThese checks do not certify hardware, workload safety or future reliability.\nMissing readings are unknown. No automatic changes were made.",
        pressure_report()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn unknown_or_impossible_readings_are_not_healthy() {
        assert_eq!(
            memory_headroom("MemTotal: 100 kB\nMemAvailable: 10 kB"),
            Some((10, 100))
        );
        for text in [
            "",
            "MemTotal: 0\nMemAvailable: 0",
            "MemTotal: 10\nMemAvailable: 11",
            "MemTotal: 10",
        ] {
            assert_eq!(memory_headroom(text), None);
        }
        assert_eq!(pressure("some avg10=12.50 avg60=1.0 total=10"), Some(12.5));
        for text in [
            "",
            "full avg10=1",
            "some avg10=NaN",
            "some avg10=-1",
            "some avg10=101",
        ] {
            assert_eq!(pressure(text), None);
        }
    }

    #[test]
    fn a_failed_or_cancelled_query_cannot_report_no_failed_services() {
        let output = |code, text: &str| {
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(code),
                stdout: text.as_bytes().to_vec(),
                stderr: Vec::new(),
            })
        };
        assert!(failed_services(output(0, "")).contains("none reported by systemd"));
        assert!(failed_services(output(256, "")).contains("unavailable"));
        assert!(failed_services(Err(std::io::Error::other("cancelled"))).contains("unavailable"));
        let failed = failed_services(output(0, "backup.service loaded failed failed Backup\n"));
        assert!(failed.contains("Failed system services: 1"));
        assert!(failed.contains("journalctl -u UNIT"));
    }
}
