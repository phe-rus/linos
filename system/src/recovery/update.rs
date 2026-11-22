//! Fail-closed A/B update transition model for the recovery environment.
//! No partition writing, package installation, or bootloader changes happen here.
//! Journal persistence is separate from the pure transition rules.

mod artifact;
pub mod health;
pub mod image;
pub mod journal;

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
    Healthy {
        active: Slot,
    },
    Staged {
        active: Slot,
        candidate: Slot,
    },
    Trial {
        active: Slot,
        candidate: Slot,
        attempts_left: u8,
    },
    RollbackRequired {
        active: Slot,
        failed: Slot,
    },
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
    if !valid_phase(state) {
        return Err("Update operation refused: inconsistent current state");
    }
    match (state, event) {
        (
            Phase::Healthy { active },
            Event::Stage {
                candidate,
                verified: true,
            },
        ) if candidate == active.other() => Ok(Phase::Staged { active, candidate }),
        (Phase::Staged { active, candidate }, Event::BeginTrial) if active != candidate => {
            Ok(Phase::Trial {
                active,
                candidate,
                attempts_left: MAX_TRIAL_BOOTS,
            })
        }
        (Phase::Trial { candidate, .. }, Event::BootHealth { passed: true }) => {
            Ok(Phase::Healthy { active: candidate })
        }
        (
            Phase::Trial {
                active,
                candidate,
                attempts_left,
            },
            Event::BootHealth { passed: false },
        ) if attempts_left > 1 => Ok(Phase::Trial {
            active,
            candidate,
            attempts_left: attempts_left - 1,
        }),
        (
            Phase::Trial {
                active,
                candidate,
                attempts_left: 1,
            },
            Event::BootHealth { passed: false },
        ) => Ok(Phase::RollbackRequired {
            active,
            failed: candidate,
        }),
        (Phase::RollbackRequired { active, .. }, Event::ConfirmRollback) => {
            Ok(Phase::Healthy { active })
        }
        _ => Err("Update operation refused: invalid state, unverified payload or active slot"),
    }
}

fn valid_phase(phase: Phase) -> bool {
    match phase {
        Phase::Healthy { .. } => true,
        Phase::Staged { active, candidate } => active != candidate,
        Phase::Trial {
            active,
            candidate,
            attempts_left,
        } => active != candidate && (1..=MAX_TRIAL_BOOTS).contains(&attempts_left),
        Phase::RollbackRequired { active, failed } => active != failed,
    }
}

/// Inspect a privileged update-state journal. No writes or boot changes.
pub fn inspect_journal(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(snapshot) = journal::inspect(path)? {
        println!("Linos A/B update state (read-only): {:?}", snapshot.phase);
        println!("Journal generation: {}", snapshot.generation);
    }
    println!("{}", describe_initial_state());
    Ok(())
}

#[cfg(test)]
fn decode_journal(input: &[u8]) -> Result<Phase, &'static str> {
    journal::decode_snapshot(input).map(|snapshot| snapshot.phase)
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
        let staged = transition(
            stable,
            Event::Stage {
                candidate: Slot::B,
                verified: true,
            },
        )
        .unwrap();
        let trial = transition(staged, Event::BeginTrial).unwrap();
        assert_eq!(
            transition(trial, Event::BootHealth { passed: true }).unwrap(),
            Phase::Healthy { active: Slot::B }
        );
    }

    #[test]
    fn unverified_updates_and_overwriting_running_slot_are_refused() {
        let state = Phase::Healthy { active: Slot::A };
        assert!(
            transition(
                state,
                Event::Stage {
                    candidate: Slot::B,
                    verified: false
                }
            )
            .is_err()
        );
        assert!(
            transition(
                state,
                Event::Stage {
                    candidate: Slot::A,
                    verified: true
                }
            )
            .is_err()
        );
        assert!(transition(state, Event::BeginTrial).is_err());
    }

    #[test]
    fn two_failed_boots_require_rollback_and_preserve_good_slot() {
        let stable = Phase::Healthy { active: Slot::A };
        let staged = transition(
            stable,
            Event::Stage {
                candidate: Slot::B,
                verified: true,
            },
        )
        .unwrap();
        let trial = transition(staged, Event::BeginTrial).unwrap();
        let retry = transition(trial, Event::BootHealth { passed: false }).unwrap();
        assert_eq!(
            retry,
            Phase::Trial {
                active: Slot::A,
                candidate: Slot::B,
                attempts_left: 1
            }
        );
        let rollback = transition(retry, Event::BootHealth { passed: false }).unwrap();
        assert_eq!(
            rollback,
            Phase::RollbackRequired {
                active: Slot::A,
                failed: Slot::B
            }
        );
        assert!(transition(rollback, Event::BeginTrial).is_err());
        assert_eq!(
            transition(rollback, Event::ConfirmRollback).unwrap(),
            stable
        );
    }

    #[test]
    fn journal_accepts_only_consistent_state() {
        assert_eq!(
            decode_journal(b"format=1\nphase=healthy\nactive=A\n").unwrap(),
            Phase::Healthy { active: Slot::A }
        );
        assert_eq!(
            decode_journal(b"format=1\nphase=trial\nactive=A\ncandidate=B\nattempts_left=2\n")
                .unwrap(),
            Phase::Trial {
                active: Slot::A,
                candidate: Slot::B,
                attempts_left: 2
            }
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
        let path =
            std::env::temp_dir().join(format!("linos-update-journal-{}", std::process::id()));
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

    #[test]
    fn inconsistent_in_memory_states_cannot_be_committed() {
        for phase in [
            Phase::Trial {
                active: Slot::A,
                candidate: Slot::A,
                attempts_left: 1,
            },
            Phase::Trial {
                active: Slot::A,
                candidate: Slot::B,
                attempts_left: 0,
            },
            Phase::Trial {
                active: Slot::A,
                candidate: Slot::B,
                attempts_left: 3,
            },
        ] {
            assert!(transition(phase, Event::BootHealth { passed: true }).is_err());
        }
        assert!(
            transition(
                Phase::RollbackRequired {
                    active: Slot::A,
                    failed: Slot::A
                },
                Event::ConfirmRollback,
            )
            .is_err()
        );
    }
}
