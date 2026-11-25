//! Layout 2 transaction records and boot selection preflight.
//! These checks do not authenticate firmware, receipts or deployment contents.

pub mod boot;
pub mod journal;
pub mod receipts;

use super::{
    hash, invalid, ordered,
    state::{decimal, positive},
};
use std::io;

const KEYS: [&str; 18] = [
    "kind",
    "format",
    "generation",
    "transaction",
    "operation",
    "phase",
    "healthy_id",
    "healthy_entry_transaction",
    "previous_healthy_id",
    "previous_entry_transaction",
    "candidate_id",
    "candidate_entry_transaction",
    "selected_deployment_id",
    "source_config_generation",
    "selected_config_generation",
    "trial_budget",
    "publication_receipt_sha256",
    "health_receipt_sha256",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Install,
    Update,
    Packages,
    Rollback,
    Repair,
}

impl Operation {
    fn parse(value: &str) -> io::Result<Self> {
        match value {
            "install" => Ok(Self::Install),
            "update" => Ok(Self::Update),
            "packages" => Ok(Self::Packages),
            "rollback" => Ok(Self::Rollback),
            "repair" => Ok(Self::Repair),
            _ => Err(invalid("Unknown transaction operation")),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Update => "update",
            Self::Packages => "packages",
            Self::Rollback => "rollback",
            Self::Repair => "repair",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    InstallPending,
    Healthy,
    Preparing,
    Staged,
    Trial,
    RollbackRequired,
    RecoveryRequired,
    Aborted,
}

impl Phase {
    fn parse(value: &str) -> io::Result<Self> {
        match value {
            "InstallPending" => Ok(Self::InstallPending),
            "Healthy" => Ok(Self::Healthy),
            "Preparing" => Ok(Self::Preparing),
            "Staged" => Ok(Self::Staged),
            "Trial" => Ok(Self::Trial),
            "RollbackRequired" => Ok(Self::RollbackRequired),
            "RecoveryRequired" => Ok(Self::RecoveryRequired),
            "Aborted" => Ok(Self::Aborted),
            _ => Err(invalid("Unknown transaction phase")),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::InstallPending => "InstallPending",
            Self::Healthy => "Healthy",
            Self::Preparing => "Preparing",
            Self::Staged => "Staged",
            Self::Trial => "Trial",
            Self::RollbackRequired => "RollbackRequired",
            Self::RecoveryRequired => "RecoveryRequired",
            Self::Aborted => "Aborted",
        }
    }
}

/// An entry is a deployment plus the transaction which published it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    deployment_id: String,
    transaction: u64,
}

impl Entry {
    pub fn new(deployment_id: &str, transaction: u64) -> io::Result<Self> {
        if transaction == 0 {
            return Err(invalid("Entry transaction must be positive"));
        }
        Ok(Self {
            deployment_id: hash(deployment_id)?,
            transaction,
        })
    }
    pub fn deployment_id(&self) -> &str {
        &self.deployment_id
    }
    pub fn transaction(&self) -> u64 {
        self.transaction
    }
}

fn optional_entry(id: &str, transaction: &str) -> io::Result<Option<Entry>> {
    match (id, transaction) {
        ("none", "none") => Ok(None),
        ("none", _) | (_, "none") => Err(invalid("Entry identity fields must be present together")),
        _ => Ok(Some(Entry::new(id, positive(transaction)?)?)),
    }
}

fn optional_hash(value: &str) -> io::Result<Option<String>> {
    if value == "none" {
        Ok(None)
    } else {
        hash(value).map(Some)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    generation: u64,
    transaction: u64,
    operation: Operation,
    phase: Phase,
    healthy: Option<Entry>,
    previous: Option<Entry>,
    candidate: Option<Entry>,
    selected_deployment_id: String,
    source_config_generation: u64,
    selected_config_generation: u64,
    publication_receipt_sha256: Option<String>,
    health_receipt_sha256: Option<String>,
}

impl Transaction {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(bytes, KEYS)?;
        if f[0] != "linos-transaction" || f[1] != "3" || f[15] != "2" {
            return Err(invalid(
                "Unsupported transaction kind, format or trial budget",
            ));
        }
        let record = Self {
            generation: decimal(f[2])?,
            transaction: positive(f[3])?,
            operation: Operation::parse(f[4])?,
            phase: Phase::parse(f[5])?,
            healthy: optional_entry(f[6], f[7])?,
            previous: optional_entry(f[8], f[9])?,
            candidate: optional_entry(f[10], f[11])?,
            selected_deployment_id: hash(f[12])?,
            source_config_generation: positive(f[13])?,
            selected_config_generation: positive(f[14])?,
            publication_receipt_sha256: optional_hash(f[16])?,
            health_receipt_sha256: optional_hash(f[17])?,
        };
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> io::Result<()> {
        if self.selected_config_generation < self.source_config_generation {
            return Err(invalid(
                "Selected configuration cannot precede transaction source",
            ));
        }
        if self.healthy.is_none() {
            if self.operation != Operation::Install
                || !matches!(self.phase, Phase::InstallPending | Phase::RecoveryRequired)
                || self.previous.is_some()
                || self.candidate.is_some()
                || self.source_config_generation != 1
                || self.selected_config_generation != 1
            {
                return Err(invalid(
                    "Missing healthy entry outside initial installation",
                ));
            }
        } else if self.phase == Phase::InstallPending {
            return Err(invalid("InstallPending cannot replace a healthy baseline"));
        }
        for entry in [&self.healthy, &self.previous, &self.candidate]
            .into_iter()
            .flatten()
        {
            if entry.transaction > self.transaction {
                return Err(invalid("Entry transaction is in the future"));
            }
        }
        if self.previous.is_some() && self.previous == self.healthy {
            return Err(invalid("Previous entry duplicates current healthy entry"));
        }
        if let Some(candidate) = &self.candidate
            && (candidate.transaction != self.transaction
                || Some(candidate) == self.healthy.as_ref()
                || Some(candidate) == self.previous.as_ref())
        {
            return Err(invalid("Candidate entry must belong to this transaction"));
        }
        match self.phase {
            Phase::Healthy | Phase::Aborted => {
                if self.candidate.is_some()
                    || self
                        .healthy
                        .as_ref()
                        .is_none_or(|entry| entry.deployment_id != self.selected_deployment_id)
                {
                    return Err(invalid(
                        "Stable selection must name healthy deployment without a candidate",
                    ));
                }
            }
            Phase::Preparing | Phase::Staged => {
                if self.candidate.is_none()
                    || self
                        .healthy
                        .as_ref()
                        .is_none_or(|entry| entry.deployment_id != self.selected_deployment_id)
                    || self.source_config_generation != self.selected_config_generation
                    || self.publication_receipt_sha256.is_some()
                    || self.health_receipt_sha256.is_some()
                {
                    return Err(invalid(
                        "Unpublished transaction changed selection or has published receipts",
                    ));
                }
            }
            Phase::Trial | Phase::RollbackRequired => {
                let candidate = self
                    .candidate
                    .as_ref()
                    .ok_or_else(|| invalid("Trial requires a candidate"))?;
                let healthy = self
                    .healthy
                    .as_ref()
                    .ok_or_else(|| invalid("Trial requires healthy fallback"))?;
                if self.selected_deployment_id != healthy.deployment_id
                    && self.selected_deployment_id != candidate.deployment_id
                {
                    return Err(invalid(
                        "Trial selection names neither healthy nor candidate",
                    ));
                }
                if self.selected_deployment_id != healthy.deployment_id
                    && self.selected_config_generation == self.source_config_generation
                {
                    return Err(invalid("Candidate requires a new configuration generation"));
                }
            }
            Phase::InstallPending | Phase::RecoveryRequired => {}
        }
        Ok(())
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn transaction(&self) -> u64 {
        self.transaction
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn healthy(&self) -> Option<&Entry> {
        self.healthy.as_ref()
    }
    pub fn previous(&self) -> Option<&Entry> {
        self.previous.as_ref()
    }
    pub fn candidate(&self) -> Option<&Entry> {
        self.candidate.as_ref()
    }
    pub fn selected_deployment_id(&self) -> &str {
        &self.selected_deployment_id
    }
    pub fn source_config_generation(&self) -> u64 {
        self.source_config_generation
    }
    pub fn selected_config_generation(&self) -> u64 {
        self.selected_config_generation
    }
    pub fn publication_receipt_sha256(&self) -> Option<&str> {
        self.publication_receipt_sha256.as_deref()
    }
    pub fn health_receipt_sha256(&self) -> Option<&str> {
        self.health_receipt_sha256.as_deref()
    }

    /// Bind the selected configuration to an authenticated official manifest,
    /// preventing an unrelated deployment's schema range from authorizing it.
    /// Artifact contents, receipt and actual configuration files remain separate
    /// checks. Local compositions need their own provenance verifier.
    pub fn require_selected_configuration(
        &self,
        configuration: &super::state::Configuration,
        manifest: &super::deployment::OfficialManifest,
    ) -> io::Result<()> {
        if manifest.id() != self.selected_deployment_id {
            return Err(invalid(
                "Authenticated deployment is not the journal selection",
            ));
        }
        configuration.require_selection(
            self.selected_config_generation,
            manifest.id(),
            manifest.deployment(),
        )
    }

    /// Construct an initial record only after all install artifacts, configuration
    /// generation 1 and independent Recovery have been durably verified.
    pub fn installed(
        deployment_id: &str,
        transaction: u64,
        publication_receipt: &str,
    ) -> io::Result<Self> {
        Entry::new(deployment_id, transaction)?;
        Ok(Self {
            generation: 0,
            transaction,
            operation: Operation::Install,
            phase: Phase::InstallPending,
            healthy: None,
            previous: None,
            candidate: None,
            selected_deployment_id: hash(deployment_id)?,
            source_config_generation: 1,
            selected_config_generation: 1,
            publication_receipt_sha256: Some(hash(publication_receipt)?),
            health_receipt_sha256: None,
        })
    }

    fn next(&self, phase: Phase) -> io::Result<Self> {
        let mut next = self.clone();
        next.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("Journal generation exhausted"))?;
        next.phase = phase;
        Ok(next)
    }

    /// Caller supplies the authenticated and admitted target. No artifacts are
    /// staged by this pure transition. Competing operations require Healthy.
    pub fn prepare(&self, operation: Operation, deployment_id: &str) -> io::Result<Self> {
        if self.phase != Phase::Healthy || operation == Operation::Install {
            return Err(invalid(
                "Preparing requires Healthy and a non-install operation",
            ));
        }
        let mut next = self.next(Phase::Preparing)?;
        next.transaction = self
            .transaction
            .checked_add(1)
            .ok_or_else(|| invalid("Transaction counter exhausted"))?;
        next.operation = operation;
        next.candidate = Some(Entry::new(deployment_id, next.transaction)?);
        next.source_config_generation = self.selected_config_generation;
        next.publication_receipt_sha256 = None;
        next.health_receipt_sha256 = None;
        next.validate()?;
        Ok(next)
    }

    /// Caller has copied, verified and synced all candidate artifacts.
    pub fn staged(&self) -> io::Result<Self> {
        if self.phase != Phase::Preparing {
            return Err(invalid("Staging requires Preparing"));
        }
        self.next(Phase::Staged)
    }

    /// Persist this intent before publishing the counted entry. This cannot be
    /// called on Trial, so retry never grants another two boot attempts.
    pub fn begin_trial(&self) -> io::Result<Self> {
        if self.phase != Phase::Staged {
            return Err(invalid("Trial requires Staged"));
        }
        self.next(Phase::Trial)
    }

    /// Attach an independently verified publication receipt without changing the
    /// entry or trial budget. Conflicting receipts require inspection.
    pub fn published(&self, receipt: &str) -> io::Result<Self> {
        if self.phase != Phase::Trial {
            return Err(invalid("Publication receipt requires Trial"));
        }
        let receipt = hash(receipt)?;
        if let Some(existing) = &self.publication_receipt_sha256 {
            return if *existing == receipt {
                Ok(self.clone())
            } else {
                Err(invalid("Conflicting publication receipt"))
            };
        }
        let mut next = self.next(self.phase)?;
        next.publication_receipt_sha256 = Some(receipt);
        Ok(next)
    }

    /// Select the compatible configuration prepared from the latest source.
    /// Caller verifies its record, merge receipt, account data and schema first.
    pub fn select_candidate(
        &self,
        configuration: &super::state::Configuration,
    ) -> io::Result<Self> {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or_else(|| invalid("Missing candidate"))?;
        if self.phase != Phase::Trial
            || self.publication_receipt_sha256.is_none()
            || configuration.deployment_id() != candidate.deployment_id
            || configuration.phase() != super::state::ConfigurationPhase::Selected
        {
            return Err(invalid(
                "Candidate selection requires published Trial and matching configuration",
            ));
        }
        if self.selected_deployment_id == candidate.deployment_id
            && self.selected_config_generation == configuration.generation()
        {
            return Ok(self.clone());
        }
        if self.selected_config_generation != self.source_config_generation
            || configuration.parent_generation() != Some(self.selected_config_generation)
        {
            return Err(invalid(
                "Candidate selection must merge from current authoritative configuration",
            ));
        }
        let mut next = self.next(self.phase)?;
        next.selected_deployment_id = candidate.deployment_id.clone();
        next.selected_config_generation = configuration.generation();
        next.validate()?;
        Ok(next)
    }

    /// Commit only after the exact observed entry was blessed, its stable file
    /// synced, and the binding health receipt independently checked.
    pub fn approve(&self, observed: &Entry, health_receipt: &str) -> io::Result<Self> {
        let health_receipt = hash(health_receipt)?;
        if self.phase == Phase::Trial
            && self
                .health_receipt_sha256
                .as_ref()
                .is_some_and(|intent| *intent != health_receipt)
        {
            return Err(invalid("Approval conflicts with durable health intent"));
        }
        let initial = self.phase == Phase::InstallPending
            && observed.deployment_id == self.selected_deployment_id
            && observed.transaction == self.transaction;
        let trial = self.phase == Phase::Trial
            && self.candidate.as_ref() == Some(observed)
            && self.selected_deployment_id == observed.deployment_id
            && self.selected_config_generation > self.source_config_generation;
        if (!initial && !trial) || self.publication_receipt_sha256.is_none() {
            return Err(invalid(
                "Approval requires exact installed or selected published candidate entry",
            ));
        }
        let mut next = self.next(Phase::Healthy)?;
        next.previous = self.healthy.clone();
        next.healthy = Some(observed.clone());
        next.candidate = None;
        next.health_receipt_sha256 = Some(health_receipt);
        next.validate()?;
        Ok(next)
    }

    /// The caller persists and verifies the exact health intent record before
    /// this transition. Native blessing must happen only after this commit.
    pub fn health_intent(&self, receipt: &str) -> io::Result<Self> {
        if self.phase != Phase::Trial
            || self.publication_receipt_sha256.is_none()
            || self
                .candidate
                .as_ref()
                .is_none_or(|entry| entry.deployment_id != self.selected_deployment_id)
            || self.selected_config_generation <= self.source_config_generation
        {
            return Err(invalid("Health intent requires selected published trial"));
        }
        let receipt = hash(receipt)?;
        if let Some(existing) = &self.health_receipt_sha256 {
            return if *existing == receipt {
                Ok(self.clone())
            } else {
                Err(invalid("Conflicting health intent"))
            };
        }
        let mut next = self.next(self.phase)?;
        next.health_receipt_sha256 = Some(receipt);
        Ok(next)
    }

    /// Caller has reconciled the exact exhausted firmware candidate entry.
    pub fn exhausted(&self) -> io::Result<Self> {
        if self.phase != Phase::Trial {
            return Err(invalid("Exhaustion requires Trial"));
        }
        self.next(Phase::RollbackRequired)
    }

    /// Preserve healthy and its previous fallback while selecting a newly merged
    /// current configuration. No old configuration or service data is restored.
    pub fn fallback(
        &self,
        configuration: &super::state::Configuration,
        receipt: &str,
    ) -> io::Result<Self> {
        let healthy = self
            .healthy
            .as_ref()
            .ok_or_else(|| invalid("Missing healthy fallback"))?;
        if self.phase != Phase::RollbackRequired
            || configuration.deployment_id() != healthy.deployment_id
            || configuration.phase() != super::state::ConfigurationPhase::Selected
            || configuration.parent_generation() != Some(self.selected_config_generation)
        {
            return Err(invalid(
                "Fallback requires exhausted trial and newly merged current configuration",
            ));
        }
        let mut next = self.next(Phase::Healthy)?;
        next.selected_deployment_id = healthy.deployment_id.clone();
        next.selected_config_generation = configuration.generation();
        next.candidate = None;
        next.health_receipt_sha256 = Some(hash(receipt)?);
        next.validate()?;
        Ok(next)
    }

    pub fn abort(&self) -> io::Result<Self> {
        if !matches!(self.phase, Phase::Preparing | Phase::Staged) {
            return Err(invalid("Only unpublished transactions can be aborted"));
        }
        let mut next = self.next(Phase::Aborted)?;
        next.candidate = None;
        Ok(next)
    }

    pub fn finish_abort(&self) -> io::Result<Self> {
        if self.phase != Phase::Aborted {
            return Err(invalid("Expected Aborted"));
        }
        self.next(Phase::Healthy)
    }

    pub fn require_recovery(&self) -> io::Result<Self> {
        self.next(Phase::RecoveryRequired)
    }

    pub fn encode(&self) -> String {
        let pair = |entry: &Option<Entry>| match entry {
            Some(entry) => (entry.deployment_id.clone(), entry.transaction.to_string()),
            None => ("none".into(), "none".into()),
        };
        let healthy = pair(&self.healthy);
        let previous = pair(&self.previous);
        let candidate = pair(&self.candidate);
        let values = [
            "linos-transaction".into(),
            "3".into(),
            self.generation.to_string(),
            self.transaction.to_string(),
            self.operation.as_str().into(),
            self.phase.as_str().into(),
            healthy.0,
            healthy.1,
            previous.0,
            previous.1,
            candidate.0,
            candidate.1,
            self.selected_deployment_id.clone(),
            self.source_config_generation.to_string(),
            self.selected_config_generation.to_string(),
            "2".into(),
            self.publication_receipt_sha256
                .clone()
                .unwrap_or("none".into()),
            self.health_receipt_sha256.clone().unwrap_or("none".into()),
        ];
        KEYS.iter()
            .zip(values)
            .map(|(key, value)| format!("{key}={value}\n"))
            .collect()
    }

    /// Gate before mounting writable configuration, Users or service data.
    /// Counter evidence must come from exact firmware entry reconciliation, not
    /// a caller override. A returned decision still requires configuration/data
    /// compatibility, authenticated artifacts and observed ancestry checks.
    pub fn check_boot(&self, observed: &Entry, evidence: BootEvidence) -> io::Result<BootDecision> {
        match self.phase {
            Phase::InstallPending
                if observed.deployment_id == self.selected_deployment_id
                    && observed.transaction == self.transaction
                    && evidence == BootEvidence::Stable =>
            {
                Ok(BootDecision::InitialSetup)
            }
            Phase::Healthy | Phase::Preparing | Phase::Staged | Phase::Aborted
                if self.healthy.as_ref() == Some(observed) && evidence == BootEvidence::Stable =>
            {
                Ok(BootDecision::Healthy)
            }
            Phase::Trial
                if self.candidate.as_ref() == Some(observed)
                    && self.publication_receipt_sha256.is_some()
                    && matches!(
                        evidence,
                        BootEvidence::Counted { left: 1, done: 1 }
                            | BootEvidence::Counted { left: 0, done: 2 }
                    ) =>
            {
                Ok(BootDecision::Candidate)
            }
            Phase::Trial | Phase::RollbackRequired
                if self.healthy.as_ref() == Some(observed)
                    && self.publication_receipt_sha256.is_some()
                    && evidence == BootEvidence::ExhaustedCandidate =>
            {
                Ok(BootDecision::MergeFallback)
            }
            _ => Err(invalid(
                "Boot identity or trial evidence requires Recovery reconciliation",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootEvidence {
    Stable,
    /// Counter after the firmware consumed this boot's attempt.
    Counted {
        left: u8,
        done: u8,
    },
    /// Observed stable healthy entry plus the exact candidate's exhausted entry.
    ExhaustedCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootDecision {
    InitialSetup,
    Healthy,
    Candidate,
    MergeFallback,
    ResumeApproval,
}

/// Evidence is verified by the installer/health coordinator before a transition.
pub enum Event<'a> {
    Prepare {
        operation: Operation,
        deployment_id: &'a str,
    },
    Staged,
    BeginTrial,
    Published {
        receipt: &'a str,
    },
    SelectCandidate {
        configuration: &'a super::state::Configuration,
    },
    HealthIntent {
        receipt: &'a str,
    },
    Approve {
        observed: &'a Entry,
        receipt: &'a str,
    },
    Exhausted,
    Fallback {
        configuration: &'a super::state::Configuration,
        receipt: &'a str,
    },
    Abort,
    FinishAbort,
    RequireRecovery,
}

impl Transaction {
    pub fn transition(&self, event: Event<'_>) -> io::Result<Self> {
        match event {
            Event::Prepare {
                operation,
                deployment_id,
            } => self.prepare(operation, deployment_id),
            Event::Staged => self.staged(),
            Event::BeginTrial => self.begin_trial(),
            Event::Published { receipt } => self.published(receipt),
            Event::SelectCandidate { configuration } => self.select_candidate(configuration),
            Event::HealthIntent { receipt } => self.health_intent(receipt),
            Event::Approve { observed, receipt } => self.approve(observed, receipt),
            Event::Exhausted => self.exhausted(),
            Event::Fallback {
                configuration,
                receipt,
            } => self.fallback(configuration, receipt),
            Event::Abort => self.abort(),
            Event::FinishAbort => self.finish_abort(),
            Event::RequireRecovery => self.require_recovery(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::records::state::Configuration;

    fn configuration(generation: u64, parent: u64, id: &str) -> Configuration {
        Configuration::parse(format!("kind=linos-configuration\nformat=1\ngeneration={generation}\ndeployment_id={id}\nparent_generation={parent}\nschema=1\nmerge_receipt_sha256={}\nphase=selected\n", "d".repeat(64)).as_bytes()).unwrap()
    }

    fn healthy() -> Transaction {
        let id = "a".repeat(64);
        Transaction::installed(&id, 1, &"b".repeat(64))
            .unwrap()
            .approve(&Entry::new(&id, 1).unwrap(), &"c".repeat(64))
            .unwrap()
    }

    fn trial() -> Transaction {
        healthy()
            .prepare(Operation::Update, &"e".repeat(64))
            .unwrap()
            .staged()
            .unwrap()
            .begin_trial()
            .unwrap()
    }

    #[test]
    fn exact_record_roundtrip_and_strict_refusals() {
        let trial = trial();
        let text = trial.encode();
        assert_eq!(Transaction::parse(text.as_bytes()).unwrap(), trial);
        for (from, to) in [
            ("format=3", "format=2"),
            ("generation=4", "generation=04"),
            ("trial_budget=2", "trial_budget=3"),
            ("transaction=2\n", "transaction=0\n"),
            ("phase=Trial", "phase=Unknown"),
            ("operation=update", "operation=unknown"),
            (
                "previous_healthy_id=none",
                &format!("previous_healthy_id={}", "f".repeat(64)),
            ),
            (
                "candidate_entry_transaction=2",
                "candidate_entry_transaction=1",
            ),
            ("source_config_generation=1", "source_config_generation=2"),
        ] {
            assert!(text.contains(from), "fixture must exercise {from}");
            assert!(
                Transaction::parse(text.replace(from, to).as_bytes()).is_err(),
                "accepted {to}"
            );
        }
        assert!(Transaction::parse(text.trim_end().as_bytes()).is_err());
        assert!(Transaction::parse(format!("{text}phase=Trial\n").as_bytes()).is_err());
        assert!(Transaction::parse(b"format=2\ngeneration=0\nphase=healthy\nactive=A\n").is_err());
    }

    #[test]
    fn trial_retries_do_not_reset_budget_or_allow_early_manual_fallback() {
        let trial = trial();
        assert!(
            trial
                .check_boot(
                    trial.candidate().unwrap(),
                    BootEvidence::Counted { left: 1, done: 1 }
                )
                .is_err()
        );
        let trial = trial.published(&"b".repeat(64)).unwrap();
        let healthy = trial.healthy().unwrap();
        let candidate = trial.candidate().unwrap();
        assert!(trial.begin_trial().is_err());
        assert!(
            trial
                .prepare(Operation::Rollback, healthy.deployment_id())
                .is_err()
        );
        assert!(trial.abort().is_err());
        assert!(trial.check_boot(healthy, BootEvidence::Stable).is_err());
        assert_eq!(
            trial
                .check_boot(healthy, BootEvidence::ExhaustedCandidate)
                .unwrap(),
            BootDecision::MergeFallback
        );
        for (left, done) in [(1, 1), (0, 2)] {
            assert_eq!(
                trial
                    .check_boot(candidate, BootEvidence::Counted { left, done })
                    .unwrap(),
                BootDecision::Candidate
            );
        }
        for (left, done) in [(2, 0), (2, 1), (0, 0), (1, 2), (0, 3)] {
            assert!(
                trial
                    .check_boot(candidate, BootEvidence::Counted { left, done })
                    .is_err()
            );
        }
        assert!(trial.check_boot(candidate, BootEvidence::Stable).is_err());
        let wrong_entry =
            Entry::new(candidate.deployment_id(), candidate.transaction() + 1).unwrap();
        assert!(
            trial
                .check_boot(&wrong_entry, BootEvidence::Counted { left: 1, done: 1 })
                .is_err()
        );
        assert!(trial.approve(candidate, &"b".repeat(64)).is_err());
    }

    #[test]
    fn approval_retains_old_healthy_and_fallback_uses_latest_configuration() {
        let receipt = "b".repeat(64);
        let trial = trial().published(&receipt).unwrap();
        assert_eq!(trial.published(&receipt).unwrap(), trial);
        assert!(trial.published(&"c".repeat(64)).is_err());
        let config = configuration(2, 1, trial.candidate().unwrap().deployment_id());
        let selected = trial.select_candidate(&config).unwrap();
        assert_eq!(selected.select_candidate(&config).unwrap(), selected);
        let approved = selected
            .approve(selected.candidate().unwrap(), &receipt)
            .unwrap();
        assert_eq!(approved.previous(), trial.healthy());
        assert_eq!(approved.healthy(), trial.candidate());
        assert_eq!(approved.selected_config_generation(), 2);
        assert!(approved.candidate().is_none());

        let second = approved
            .prepare(Operation::Packages, &"f".repeat(64))
            .unwrap()
            .staged()
            .unwrap()
            .begin_trial()
            .unwrap()
            .published(&receipt)
            .unwrap();
        let config = configuration(3, 2, second.candidate().unwrap().deployment_id());
        let selected = second.select_candidate(&config).unwrap();
        let failed = selected.exhausted().unwrap();
        let stale = configuration(4, 2, failed.healthy().unwrap().deployment_id());
        assert!(failed.fallback(&stale, &receipt).is_err());
        let current = configuration(4, 3, failed.healthy().unwrap().deployment_id());
        let fallback = failed.fallback(&current, &receipt).unwrap();
        assert_eq!(fallback.healthy(), approved.healthy());
        assert_eq!(fallback.previous(), approved.previous());
        assert_eq!(fallback.selected_config_generation(), 4);
        assert!(fallback.candidate().is_none());
        assert_eq!(
            Transaction::parse(fallback.encode().as_bytes()).unwrap(),
            fallback
        );
    }

    #[test]
    fn initial_setup_abort_overflow_and_recovery_are_explicit() {
        let id = "a".repeat(64);
        let receipt = "b".repeat(64);
        let initial = Transaction::installed(&id, 1, &receipt).unwrap();
        let entry = Entry::new(&id, 1).unwrap();
        assert_eq!(
            initial.check_boot(&entry, BootEvidence::Stable).unwrap(),
            BootDecision::InitialSetup
        );
        assert!(initial.prepare(Operation::Update, &id).is_err());
        let prepared = healthy()
            .prepare(Operation::Rollback, &"f".repeat(64))
            .unwrap();
        let aborted = prepared.abort().unwrap();
        assert_eq!(
            aborted.finish_abort().unwrap().healthy(),
            healthy().healthy()
        );
        assert!(aborted.candidate().is_none());
        let recovery = prepared.require_recovery().unwrap();
        assert!(recovery.check_boot(&entry, BootEvidence::Stable).is_err());
        assert!(recovery.begin_trial().is_err());
        let mut overflow = healthy();
        overflow.generation = u64::MAX;
        assert!(overflow.prepare(Operation::Update, &id).is_err());
        overflow.generation = 1;
        overflow.transaction = u64::MAX;
        assert!(overflow.prepare(Operation::Update, &id).is_err());
    }
}
