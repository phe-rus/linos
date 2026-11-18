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
    fn out_of_order_approval_is_rejected() {
        let stable = Phase::Healthy { active: Slot::B };
        assert!(transition(stable, Event::BootHealth { passed: true }).is_err());
        assert!(transition(stable, Event::ConfirmRollback).is_err());
    }
}
