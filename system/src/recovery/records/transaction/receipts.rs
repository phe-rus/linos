//! Canonical trial publication and approval intent receipts. These are local
//! consistency records, not signatures or a replacement for native health checks.

use super::{
    Entry, Phase, Transaction,
    boot::{self, Counter, Observation},
    optional_entry,
};
use crate::recovery::records::{
    Layout,
    deployment::{Deployment, OfficialManifest, sha256},
    hash, invalid, ordered,
    state::{decimal, positive},
};
use std::io;

const PUBLICATION_KEYS: [&str; 12] = [
    "kind",
    "format",
    "transaction",
    "source_journal_generation",
    "source_config_generation",
    "healthy_id",
    "healthy_entry_transaction",
    "candidate_id",
    "candidate_entry_transaction",
    "entry_sha256",
    "kernel_sha256",
    "initramfs_sha256",
];
const HEALTH_KEYS: [&str; 16] = [
    "kind",
    "format",
    "transaction",
    "source_journal_generation",
    "source_config_generation",
    "selected_config_generation",
    "healthy_id",
    "healthy_entry_transaction",
    "candidate_id",
    "candidate_entry_transaction",
    "publication_receipt_sha256",
    "entry_sha256",
    "kernel_sha256",
    "initramfs_sha256",
    "observed_entry_path",
    "decision",
];

fn record<const N: usize>(keys: [&str; N], values: [String; N]) -> String {
    keys.into_iter()
        .zip(values)
        .map(|(key, value)| format!("{key}={value}\n"))
        .collect()
}

fn required_entry(id: &str, transaction: &str) -> io::Result<Entry> {
    optional_entry(id, transaction)?
        .ok_or_else(|| invalid("Trial receipt requires complete healthy and candidate entries"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    transaction: u64,
    source_journal_generation: u64,
    source_config_generation: u64,
    healthy: Entry,
    candidate: Entry,
    entry_sha256: String,
    kernel_sha256: String,
    initramfs_sha256: String,
}

impl Publication {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(bytes, PUBLICATION_KEYS)?;
        if f[0] != "linos-publication" || f[1] != "1" {
            return Err(invalid("Unsupported publication receipt"));
        }
        let receipt = Self {
            transaction: positive(f[2])?,
            source_journal_generation: decimal(f[3])?,
            source_config_generation: positive(f[4])?,
            healthy: required_entry(f[5], f[6])?,
            candidate: required_entry(f[7], f[8])?,
            entry_sha256: hash(f[9])?,
            kernel_sha256: hash(f[10])?,
            initramfs_sha256: hash(f[11])?,
        };
        if receipt.candidate.transaction() != receipt.transaction
            || receipt.healthy.transaction() >= receipt.transaction
        {
            return Err(invalid("Publication receipt entry transactions conflict"));
        }
        Ok(receipt)
    }

    /// Capture the durable Trial intent before publishing the initial counted
    /// entry. Actual boot file length/hash checks and EFI sync remain required.
    pub fn new(
        journal: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
    ) -> io::Result<Self> {
        Self::build(journal, layout, manifest.id(), manifest.deployment())
    }

    fn build(
        journal: &Transaction,
        layout: &Layout,
        id: &str,
        deployment: &Deployment,
    ) -> io::Result<Self> {
        layout.require_plain()?;
        let candidate = journal
            .candidate()
            .ok_or_else(|| invalid("Missing candidate"))?;
        if journal.phase() != Phase::Trial || candidate.deployment_id() != id {
            return Err(invalid(
                "Publication requires matching authenticated trial deployment",
            ));
        }
        Ok(Self {
            transaction: journal.transaction(),
            source_journal_generation: journal.generation(),
            source_config_generation: journal.source_config_generation(),
            healthy: journal
                .healthy()
                .ok_or_else(|| invalid("Missing healthy entry"))?
                .clone(),
            candidate: candidate.clone(),
            entry_sha256: sha256(boot::render(layout, candidate, true).as_bytes())?,
            kernel_sha256: deployment.artifacts[1].sha256.clone(),
            initramfs_sha256: deployment.artifacts[2].sha256.clone(),
        })
    }

    pub fn encode(&self) -> String {
        record(
            PUBLICATION_KEYS,
            [
                "linos-publication".into(),
                "1".into(),
                self.transaction.to_string(),
                self.source_journal_generation.to_string(),
                self.source_config_generation.to_string(),
                self.healthy.deployment_id().into(),
                self.healthy.transaction().to_string(),
                self.candidate.deployment_id().into(),
                self.candidate.transaction().to_string(),
                self.entry_sha256.clone(),
                self.kernel_sha256.clone(),
                self.initramfs_sha256.clone(),
            ],
        )
    }
    pub fn digest(&self) -> io::Result<String> {
        sha256(self.encode().as_bytes())
    }

    /// With no journal pointer, require the exact original intent generation.
    /// Once published, require its exact digest and a later journal generation.
    pub fn verify(
        &self,
        journal: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
    ) -> io::Result<()> {
        self.verify_binding(journal, layout, manifest.id(), manifest.deployment())
    }

    fn verify_binding(
        &self,
        journal: &Transaction,
        layout: &Layout,
        id: &str,
        deployment: &Deployment,
    ) -> io::Result<()> {
        let mut expected = Self::build(journal, layout, id, deployment)?;
        expected.source_journal_generation = self.source_journal_generation;
        if *self != expected {
            return Err(invalid(
                "Publication receipt disagrees with journal, manifest or canonical entry",
            ));
        }
        let valid_generation = match journal.publication_receipt_sha256() {
            None => journal.generation() == self.source_journal_generation,
            Some(digest) => {
                digest == self.digest()? && journal.generation() > self.source_journal_generation
            }
        };
        if !valid_generation {
            return Err(invalid("Publication receipt is stale or not journal bound"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthIntent {
    transaction: u64,
    source_journal_generation: u64,
    source_config_generation: u64,
    selected_config_generation: u64,
    healthy: Entry,
    candidate: Entry,
    publication_receipt_sha256: String,
    entry_sha256: String,
    kernel_sha256: String,
    initramfs_sha256: String,
    observed_entry_path: String,
}

impl HealthIntent {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(bytes, HEALTH_KEYS)?;
        if f[0] != "linos-health" || f[1] != "1" || f[15] != "approve" {
            return Err(invalid("Unsupported health receipt or decision"));
        }
        let receipt = Self {
            transaction: positive(f[2])?,
            source_journal_generation: decimal(f[3])?,
            source_config_generation: positive(f[4])?,
            selected_config_generation: positive(f[5])?,
            healthy: required_entry(f[6], f[7])?,
            candidate: required_entry(f[8], f[9])?,
            publication_receipt_sha256: hash(f[10])?,
            entry_sha256: hash(f[11])?,
            kernel_sha256: hash(f[12])?,
            initramfs_sha256: hash(f[13])?,
            observed_entry_path: f[14].into(),
        };
        let filename = receipt
            .observed_entry_path
            .strip_prefix("/loader/entries/")
            .ok_or_else(|| invalid("Health observation is outside managed EFI entries"))?;
        let (entry, counter) = boot::parse_filename(filename)?;
        if entry != receipt.candidate
            || !matches!(counter, Counter::First | Counter::Last)
            || receipt.candidate.transaction() != receipt.transaction
            || receipt.healthy.transaction() >= receipt.transaction
            || receipt.selected_config_generation <= receipt.source_config_generation
        {
            return Err(invalid(
                "Health intent requires exact counted candidate and current configuration",
            ));
        }
        Ok(receipt)
    }

    /// Called only after all mandatory native health checks have passed. This
    /// captures their decision, but does not perform those checks itself.
    pub fn new(
        journal: &Transaction,
        publication: &Publication,
        observed: &Observation,
    ) -> io::Result<Self> {
        if observed.decision() != super::BootDecision::Candidate
            || observed.journal_generation() != journal.generation()
            || observed.selectors().entry != publication.candidate
            || observed.entry_sha256() != publication.entry_sha256
        {
            return Err(invalid(
                "Health observation does not match canonical published candidate",
            ));
        }
        let receipt = Self {
            transaction: journal.transaction(),
            source_journal_generation: journal.generation(),
            source_config_generation: journal.source_config_generation(),
            selected_config_generation: journal.selected_config_generation(),
            healthy: publication.healthy.clone(),
            candidate: publication.candidate.clone(),
            publication_receipt_sha256: publication.digest()?,
            entry_sha256: observed.entry_sha256().into(),
            kernel_sha256: publication.kernel_sha256.clone(),
            initramfs_sha256: publication.initramfs_sha256.clone(),
            observed_entry_path: observed.entry_path().into(),
        };
        receipt.check_selection(journal, publication)?;
        Self::parse(receipt.encode().as_bytes())
    }

    fn check_selection(&self, journal: &Transaction, publication: &Publication) -> io::Result<()> {
        if journal.phase() != Phase::Trial
            || journal.transaction() != self.transaction
            || journal.healthy() != Some(&self.healthy)
            || journal.candidate() != Some(&self.candidate)
            || journal.selected_deployment_id() != self.candidate.deployment_id()
            || journal.source_config_generation() != self.source_config_generation
            || journal.selected_config_generation() != self.selected_config_generation
            || journal.publication_receipt_sha256()
                != Some(self.publication_receipt_sha256.as_str())
            || self.publication_receipt_sha256 != publication.digest()?
            || self.entry_sha256 != publication.entry_sha256
            || self.kernel_sha256 != publication.kernel_sha256
            || self.initramfs_sha256 != publication.initramfs_sha256
        {
            return Err(invalid(
                "Health intent disagrees with publication or selected transaction",
            ));
        }
        Ok(())
    }

    pub fn verify(&self, journal: &Transaction, publication: &Publication) -> io::Result<()> {
        self.check_selection(journal, publication)?;
        let valid = match journal.health_receipt_sha256() {
            None => journal.generation() == self.source_journal_generation,
            Some(digest) => {
                digest == self.digest()?
                    && self.source_journal_generation.checked_add(1) == Some(journal.generation())
            }
        };
        if !valid {
            return Err(invalid(
                "Health intent is stale or conflicts with journal authority",
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> String {
        record(
            HEALTH_KEYS,
            [
                "linos-health".into(),
                "1".into(),
                self.transaction.to_string(),
                self.source_journal_generation.to_string(),
                self.source_config_generation.to_string(),
                self.selected_config_generation.to_string(),
                self.healthy.deployment_id().into(),
                self.healthy.transaction().to_string(),
                self.candidate.deployment_id().into(),
                self.candidate.transaction().to_string(),
                self.publication_receipt_sha256.clone(),
                self.entry_sha256.clone(),
                self.kernel_sha256.clone(),
                self.initramfs_sha256.clone(),
                self.observed_entry_path.clone(),
                "approve".into(),
            ],
        )
    }
    pub fn digest(&self) -> io::Result<String> {
        sha256(self.encode().as_bytes())
    }
    pub fn observed_entry_path(&self) -> &str {
        &self.observed_entry_path
    }
    pub fn candidate(&self) -> &Entry {
        &self.candidate
    }
    pub fn entry_sha256(&self) -> &str {
        &self.entry_sha256
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::records::{state::Configuration, transaction::Operation};

    fn layout() -> Layout {
        Layout::parse(format!("kind=linos-layout\nformat=2\nvolume_uuid=11111111-1111-1111-1111-111111111111\ndisk_guid=22222222-2222-2222-2222-222222222222\nesp_partuuid=33333333-3333-3333-3333-333333333333\nrecovery_partuuid=44444444-4444-4444-4444-444444444444\nlinos_partuuid=55555555-5555-5555-5555-555555555555\nfilesystem=ext4\nfilesystem_uuid=66666666-6666-6666-6666-666666666666\nencryption=none\nluks_uuid=none\ndevice_key_sha256={}\nbaseline_manifest_sha256={}\n", "a".repeat(64), "b".repeat(64)).as_bytes()).unwrap()
    }

    fn fixture() -> (Transaction, Deployment, Publication, HealthIntent) {
        let old = "a".repeat(64);
        let new = "e".repeat(64);
        let digest = "b".repeat(64);
        let journal = Transaction::installed(&old, 1, &digest)
            .unwrap()
            .approve(&Entry::new(&old, 1).unwrap(), &digest)
            .unwrap()
            .prepare(Operation::Update, &new)
            .unwrap()
            .staged()
            .unwrap()
            .begin_trial()
            .unwrap();
        let deployment = Deployment {
            origin: crate::recovery::records::deployment::Origin::Ci,
            source_sha: "a".repeat(40),
            build_id: "fixture".into(),
            release: 1,
            architecture: "amd64".into(),
            hardware_profile: "qemu-amd64-uefi".into(),
            kernel_release: "test".into(),
            config_schema_min: 1,
            config_schema_max: 1,
            artifacts: std::array::from_fn(|_| crate::recovery::records::deployment::Artifact {
                bytes: 1,
                sha256: "f".repeat(64),
            }),
        };
        let publication = Publication::build(&journal, &layout(), &new, &deployment).unwrap();
        let journal = journal.published(&publication.digest().unwrap()).unwrap();
        let configuration = Configuration::parse(format!("kind=linos-configuration\nformat=1\ngeneration=2\ndeployment_id={new}\nparent_generation=1\nschema=1\nmerge_receipt_sha256={digest}\nphase=selected\n").as_bytes()).unwrap();
        let journal = journal.select_candidate(&configuration).unwrap();
        let health = HealthIntent {
            transaction: journal.transaction(),
            source_journal_generation: journal.generation(),
            source_config_generation: 1,
            selected_config_generation: 2,
            healthy: journal.healthy().unwrap().clone(),
            candidate: journal.candidate().unwrap().clone(),
            publication_receipt_sha256: publication.digest().unwrap(),
            entry_sha256: publication.entry_sha256.clone(),
            kernel_sha256: publication.kernel_sha256.clone(),
            initramfs_sha256: publication.initramfs_sha256.clone(),
            observed_entry_path: format!(
                "/loader/entries/{}",
                boot::filename(journal.candidate().unwrap(), Counter::First)
            ),
        };
        (journal, deployment, publication, health)
    }

    #[test]
    fn canonical_receipts_bind_exact_entry_artifacts_and_journal_authority() {
        let (journal, deployment, publication, health) = fixture();
        assert_eq!(
            Publication::parse(publication.encode().as_bytes()).unwrap(),
            publication
        );
        assert_eq!(
            HealthIntent::parse(health.encode().as_bytes()).unwrap(),
            health
        );
        publication
            .verify_binding(
                &journal,
                &layout(),
                journal.candidate().unwrap().deployment_id(),
                &deployment,
            )
            .unwrap();
        health.verify(&journal, &publication).unwrap();
        let intent = journal.health_intent(&health.digest().unwrap()).unwrap();
        health.verify(&intent, &publication).unwrap();
        assert!(
            intent
                .approve(intent.candidate().unwrap(), &"c".repeat(64))
                .is_err()
        );
        assert_eq!(
            intent.health_intent(&health.digest().unwrap()).unwrap(),
            intent
        );
        assert!(intent.health_intent(&"c".repeat(64)).is_err());
        let mut stale = health.clone();
        stale.source_journal_generation -= 1;
        assert!(stale.verify(&intent, &publication).is_err());
        let mut wrong = publication.clone();
        wrong.kernel_sha256 = "c".repeat(64);
        assert!(
            wrong
                .verify_binding(
                    &journal,
                    &layout(),
                    journal.candidate().unwrap().deployment_id(),
                    &deployment
                )
                .is_err()
        );
        let mut wrong = health.clone();
        wrong.selected_config_generation = 3;
        assert!(wrong.verify(&journal, &publication).is_err());
        let mut wrong = health.clone();
        wrong.candidate = Entry::new(&"c".repeat(64), journal.transaction()).unwrap();
        assert!(wrong.verify(&journal, &publication).is_err());
    }

    #[test]
    fn malformed_receipts_never_become_reconciliation_evidence() {
        let (_, _, publication, health) = fixture();
        let text = health.encode();
        for (from, to) in [
            ("decision=approve", "decision=fallback"),
            ("format=1", "format=01"),
            (
                "selected_config_generation=2",
                "selected_config_generation=1",
            ),
            ("/loader/entries/", "/elsewhere/"),
            ("+1-1.conf", ".conf"),
            ("+1-1.conf", "+2.conf"),
            ("transaction=2\n", "transaction=02\n"),
        ] {
            assert!(text.contains(from));
            assert!(HealthIntent::parse(text.replace(from, to).as_bytes()).is_err());
        }
        assert!(HealthIntent::parse(format!("{text}decision=approve\n").as_bytes()).is_err());
        assert!(Publication::parse(publication.encode().trim_end().as_bytes()).is_err());
        let text = publication.encode().replace(
            "candidate_entry_transaction=2",
            "candidate_entry_transaction=3",
        );
        assert!(Publication::parse(text.as_bytes()).is_err());
    }
}
