//! Native health approval uses the same locked boot and mount inspection as the
//! init gate. No caller can supply identity, readiness or blessing observations.
use super::*;
use crate::recovery::records::transaction::{Phase, receipts::HealthIntent};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn text(bytes: &[u8]) -> io::Result<&str> {
    let value = std::str::from_utf8(bytes).map_err(|_| invalid("Invalid device health text"))?;
    let value = value.strip_suffix('\n').unwrap_or(value);
    if value.is_empty() || !value.bytes().all(|b| (b' '..=b'~').contains(&b)) {
        return Err(invalid("Device health value is empty or multiline"));
    }
    Ok(value)
}

fn owner<'a>(
    device_id: &str,
    provisioning: &str,
    setup: &str,
    record: &'a [u8],
) -> io::Result<&'a str> {
    if provisioning != "provisioned" || setup != "ready" {
        return Err(invalid("Device enrollment is incomplete"));
    }
    let record = std::str::from_utf8(record).map_err(|_| invalid("Invalid owner record"))?;
    let mut lines = record.lines();
    let username = lines
        .next()
        .and_then(|line| line.strip_prefix("username="))
        .ok_or_else(|| invalid("Missing enrolled owner"))?;
    if username.is_empty()
        || username.len() > 32
        || !username.as_bytes()[0].is_ascii_lowercase()
        || !username
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
        || username == "root"
        || lines.next() != Some(format!("device-id={device_id}").as_str())
        || lines.next().is_some()
    {
        return Err(invalid("Owner does not match the current device"));
    }
    Ok(username)
}

fn identity(
    device_id: &str,
    saved_hostname: &str,
    etc_hostname: &str,
    running_hostname: &str,
) -> io::Result<()> {
    super::super::records::uuid(device_id)?;
    if saved_hostname.len() > 253
        || !saved_hostname.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        || etc_hostname != saved_hostname
        || running_hostname != saved_hostname
    {
        return Err(invalid(
            "Running hostname differs from persistent device identity",
        ));
    }
    Ok(())
}

fn active(service: &str) -> io::Result<bool> {
    Ok(Command::new("/usr/bin/systemctl")
        .args(["is-active", "--quiet", service])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success())
}

fn enrollment_running() -> io::Result<bool> {
    let output = Command::new("/usr/bin/systemctl")
        .args([
            "show",
            "linos-enroll.service",
            "--property=MainPID",
            "--value",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Ok(false);
    }
    let pid = std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|pid| *pid > 1);
    let Some(pid) = pid else { return Ok(false) };
    for (name, expected) in [
        ("exe", "/usr/lib/linos/linos-enroll"),
        ("fd/0", "/dev/console"),
    ] {
        match std::fs::read_link(format!("/proc/{pid}/{name}")) {
            Ok(path) if path == Path::new(expected) => {}
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

fn ready(initial: bool) -> io::Result<bool> {
    if !active("linos-first-boot.service")? {
        return Ok(false);
    }
    let directory = open_directory(Path::new("/var/lib/linos"))?;
    let id = source::read_at(&directory, "device-id", 64)?;
    let hostname = source::read_at(&directory, "hostname", 256)?;
    let provisioning = source::read_at(&directory, "provisioning-state", 64)?;
    let etc_hostname = source::read_at(&open_directory(Path::new("/etc"))?, "hostname", 256)?;
    let uname = rustix::system::uname();
    identity(
        text(&id)?,
        text(&hostname)?,
        text(&etc_hostname)?,
        uname
            .nodename()
            .to_str()
            .map_err(|_| invalid("Invalid running hostname"))?,
    )?;
    if initial && text(&provisioning)? == "unclaimed" {
        match source::read_at(&directory, "owner", 256) {
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        return enrollment_running();
    }
    if !active("serial-getty@ttyS0.service")? {
        return Ok(false);
    }
    let owner_record = source::read_at(&directory, "owner", 256)?;
    let setup = source::read_at(
        &open_directory(Path::new("/etc/linos"))?,
        "setup-complete",
        32,
    )?;
    let username = owner(
        text(&id)?,
        text(&provisioning)?,
        text(&setup)?,
        &owner_record,
    )?;
    generation::accounts(&super::super::configuration::tree::snapshot(Path::new(
        "/etc",
    ))?)?
    .require_administrator(username)?;
    match std::fs::read_to_string("/sys/kernel/linos/boot_state") {
        Ok(state) => Ok(text(state.as_bytes())? == "ready"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

pub(super) fn approve_running_boot() -> io::Result<()> {
    let context = context(selectors()?, false)?;
    verify_context(&context)?;
    let initial =
        context.snapshot.operation() == crate::recovery::records::transaction::Operation::Install;
    if !matches!(
        context.snapshot.phase(),
        Phase::InstallPending | Phase::Trial | Phase::Healthy
    ) {
        return Err(invalid("Transaction cannot approve native health"));
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    while !ready(initial)? {
        if Instant::now() >= deadline {
            return Err(invalid("Native health readiness deadline expired"));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    verify_context(&context)?;
    let journal = &context.journal;
    let mut snapshot = context.snapshot.clone();
    let observed = journal.inspect_boot(&context.layout)?;
    if snapshot.phase() == Phase::InstallPending {
        journal.approve_initial(
            &snapshot,
            &context.layout,
            context.loaded.manifest(),
            &context.configuration,
            &observed,
        )?;
        return Ok(());
    }
    if snapshot.phase() == Phase::Healthy {
        if initial {
            journal.verify_initial_health(
                &snapshot,
                &context.layout,
                context.loaded.manifest(),
                &context.configuration,
            )?;
            return Ok(());
        }
        configuration::resume_retirement(
            journal,
            &snapshot,
            &context.layout,
            &context.loaded,
            &context.key,
        )?;
        return Ok(());
    }
    if observed.decision() == BootDecision::Candidate {
        let health = if snapshot.health_receipt_sha256().is_some() {
            journal.health_intent(&snapshot)?
        } else {
            HealthIntent::new(&snapshot, &journal.publication(&snapshot)?, &observed)?
        };
        snapshot = journal.record_health_intent(
            &snapshot,
            &context.layout,
            context.loaded.manifest(),
            &health,
        )?;
        // Reinspect the exact counted entry after syncing intent, immediately
        // before invoking the native helper. No independent blessing is enabled.
        let before = journal.inspect_boot(&context.layout)?;
        if before.selectors() != &context.selectors || before.decision() != BootDecision::Candidate
        {
            return Err(invalid("Boot changed before native blessing"));
        }
        if !Command::new("/usr/lib/systemd/systemd-bless-boot")
            .arg("good")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
        {
            return Err(invalid(
                "Native boot blessing failed; durable intent retained",
            ));
        }
    } else if observed.decision() != BootDecision::ResumeApproval {
        return Err(invalid(
            "Native health did not observe an approved candidate",
        ));
    }
    journal.resume_approval(&snapshot, &context.layout, context.loaded.manifest())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "11111111-1111-4111-8111-111111111111";
    #[test]
    fn health_binds_current_owner_and_persistent_identity() {
        let record = format!("username=niina\ndevice-id={ID}\n");
        assert_eq!(
            owner(ID, "provisioned", "ready", record.as_bytes()).unwrap(),
            "niina"
        );
        for bad in [
            record.replace(ID, "other-device"),
            record.replace("niina", "root"),
            format!("{record}extra=value\n"),
        ] {
            assert!(owner(ID, "provisioned", "ready", bad.as_bytes()).is_err());
        }
        assert!(owner(ID, "unclaimed", "ready", record.as_bytes()).is_err());
        assert!(owner(ID, "provisioned", "pending", record.as_bytes()).is_err());
        identity(ID, "linos-server", "linos-server", "linos-server").unwrap();
        assert!(identity(ID, "linos-server", "changed", "linos-server").is_err());
        assert!(identity(ID, "linos-server", "linos-server", "changed").is_err());
        assert!(identity("invalid", "server", "server", "server").is_err());
        assert!(text(b"ready\nextra\n").is_err());
    }
}
