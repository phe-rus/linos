//! Pure, fail-closed A/B update transition model for the recovery environment.
//! No partition writing, package installation, or bootloader changes happen here.
//! Disk-backed transactions must be built and separately qualified before use.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Healthy { active: Slot },
    Staged { active: Slot, candidate: Slot },
    Trial { active: Slot, candidate: Slot, attempts_left: u8 },
    RollbackRequired { active: Slot, failed: Slot },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Stage { candidate: Slot, verified: bool },
    BeginTrial,
    BootHealth { passed: bool },
    ConfirmRollback,
}

pub const MAX_TRIAL_BOOTS: u8 = 2;

pub fn transition(state: Phase, event: Event) -> Result<Phase, &'static str> {
    match (state, event) {
        (Phase::Healthy { active }, Event::Stage { candidate, verified: true })
            if candidate == active.other() =>
        {
            Ok(Phase::Staged { active, candidate })
        }
        (Phase::Staged { active, candidate }, Event::BeginTrial)
            if active != candidate =>
        {
            Ok(Phase::Trial {
                active,
                candidate,
                attempts_left: MAX_TRIAL_BOOTS,
            })
        }
        (Phase::Trial { candidate, .. }, Event::BootHealth { passed: true }) => {
            Ok(Phase::Healthy { active: candidate })
        }
        (Phase::Trial { active, candidate, attempts_left }, Event::BootHealth { passed: false })
            if attempts_left > 1 =>
        {
            Ok(Phase::Trial { active, candidate, attempts_left: attempts_left - 1 })
        }
        (Phase::Trial { active, candidate, attempts_left: 1 }, Event::BootHealth { passed: false }) => {
            Ok(Phase::RollbackRequired { active, failed: candidate })
        }
        (Phase::RollbackRequired { active, .. }, Event::ConfirmRollback) => {
            Ok(Phase::Healthy { active })
        }
        _ => Err("Update operation refused: invalid state, unverified payload or active slot"),
    }
}


/// Inspect a privileged update-state journal. No writes or boot changes.
pub fn inspect_journal(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("{}", describe_initial_state());
            Ok(())
        }
        Err(error) => Err(error),
        Ok(metadata) if !metadata.file_type().is_file() => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Update journal must be a regular file, never a symlink",
        )),
        Ok(_) => {
            let bytes = std::fs::read(path)?;
            let phase = decode_journal(&bytes)
                .map_err(|reason| std::io::Error::new(std::io::ErrorKind::InvalidData, reason))?;
            println!("Linos A/B update state (read-only): {phase:?}");
            println!("{}", describe_initial_state());
            Ok(())
        }
    }
}

/// Format v1 is intentionally narrow and fails closed on unknown/duplicate
/// fields. It is not a signed transaction journal or a bootloader protocol.
fn decode_journal(input: &[u8]) -> Result<Phase, &'static str> {
    if input.len() > 256 {
        return Err("Update journal exceeds size limit");
    }
    let text = std::str::from_utf8(input).map_err(|_| "Journal is not UTF-8")?;
    let mut fields = std::collections::BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').ok_or("Malformed update journal")?;
        if key.is_empty() || value.is_empty() || fields.insert(key, value).is_some() {
            return Err("Duplicate or empty update journal field");
        }
    }
    if fields.get("format") != Some(&"1") {
        return Err("Unsupported update journal format");
    }
    let slot = |key| match fields.get(key).copied() {
        Some("A") => Ok(Slot::A),
        Some("B") => Ok(Slot::B),
        _ => Err("Invalid or missing slot"),
    };
    let active = slot("active")?;
    let result = match fields.get("phase").copied() {
        Some("healthy") if fields.len() == 3 => Phase::Healthy { active },
        Some("staged") if fields.len() == 4 => {
            let candidate = slot("candidate")?;
            if candidate == active {
                return Err("Update cannot stage the active slot");
            }
            Phase::Staged { active, candidate }
        }
        Some("trial") if fields.len() == 5 => {
            let candidate = slot("candidate")?;
            if candidate == active {
                return Err("Update cannot trial the active slot");
            }
            let attempts_left = fields
                .get("attempts_left")
                .ok_or("Missing boot attempt count")?
                .parse::<u8>()
                .map_err(|_| "Invalid boot attempt count")?;
            if !(1..=MAX_TRIAL_BOOTS).contains(&attempts_left) {
                return Err("Boot attempt count outside safe bounds");
            }
            Phase::Trial { active, candidate, attempts_left }
        }
        Some("rollback") if fields.len() == 4 => {
            let failed = slot("candidate")?;
            if failed == active {
                return Err("Cannot roll back to failed active slot");
            }
            Phase::RollbackRequired { active, failed }
        }
        _ => return Err("Unexpected update phase or journal fields"),
    };
    Ok(result)
}

pub fn describe_initial_state() -> &'static str {
    "A/B design: inactive system slot receives only verified updates; \
     boot trials keep the last healthy slot available. Automatic slot \
     writing, bootloader switching and rollback are NOT enabled."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_update_commits_only_after_health_passes() {
        let stable = Phase::Healthy { active: Slot::A };
        let staged = transition(stable, Event::Stage { candidate: Slot::B, verified: true }).unwrap();
        let trial = transition(staged, Event::BeginTrial).unwrap();
        assert_eq!(transition(trial, Event::BootHealth { passed: true }).unwrap(), Phase::Healthy { active: Slot::B });
    }

    #[test]
    fn unverified_updates_and_overwriting_running_slot_are_refused() {
        let state = Phase::Healthy { active: Slot::A };
        assert!(transition(state, Event::Stage { candidate: Slot::B, verified: false }).is_err());
        assert!(transition(state, Event::Stage { candidate: Slot::A, verified: true }).is_err());
        assert!(transition(state, Event::BeginTrial).is_err());
    }

    #[test]
    fn two_failed_boots_require_rollback_and_preserve_good_slot() {
        let stable = Phase::Healthy { active: Slot::A };
        let staged = transition(stable, Event::Stage { candidate: Slot::B, verified: true }).unwrap();
        let trial = transition(staged, Event::BeginTrial).unwrap();
        let retry = transition(trial, Event::BootHealth { passed: false }).unwrap();
        assert_eq!(retry, Phase::Trial { active: Slot::A, candidate: Slot::B, attempts_left: 1 });
        let rollback = transition(retry, Event::BootHealth { passed: false }).unwrap();
        assert_eq!(rollback, Phase::RollbackRequired { active: Slot::A, failed: Slot::B });
        assert!(transition(rollback, Event::BeginTrial).is_err());
        assert_eq!(transition(rollback, Event::ConfirmRollback).unwrap(), stable);
    }

    #[test]
    fn journal_accepts_only_consistent_state() {
        assert_eq!(
            decode_journal(b"format=1\nphase=healthy\nactive=A\n").unwrap(),
            Phase::Healthy { active: Slot::A }
        );
        assert_eq!(
            decode_journal(b"format=1\nphase=trial\nactive=A\ncandidate=B\nattempts_left=2\n").unwrap(),
            Phase::Trial { active: Slot::A, candidate: Slot::B, attempts_left: 2 }
        );
        for data in [
            "format=1\nphase=trial\nactive=A\ncandidate=A\nattempts_left=1\n",
            "format=1\nphase=trial\nactive=A\ncandidate=B\nattempts_left=0\n",
            "format=1\nphase=trial\nactive=A\ncandidate=B\nattempts_left=3\n",
            "format=1\nphase=healthy\nactive=A\nactive=B\n",
            "format=1\nphase=healthy\nactive=A\nextra=field\n",
            "format=0\nphase=healthy\nactive=A\n",
        ] {
            assert!(decode_journal(data.as_bytes()).is_err());
        }
    }

    #[test]
    fn journal_refuses_symlinks() {
        use std::os::unix::fs::symlink;
        let path = std::env::temp_dir().join(format!(
            "linos-update-journal-{}",
            std::process::id()
        ));
        // File is not used as a journal; only its symlink type is examined.
        symlink("/etc/passwd", &path).unwrap();
        assert!(inspect_journal(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn out_of_order_approval_is_rejected() {
        let stable = Phase::Healthy { active: Slot::B };
        assert!(transition(stable, Event::BootHealth { passed: true }).is_err());
        assert!(transition(stable, Event::ConfirmRollback).is_err());
    }
}
