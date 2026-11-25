//! Immutable, digest named receipts beneath the locked format 3 directory.
use super::*;
use crate::recovery::records::{
    Layout,
    deployment::{OfficialManifest, sha256},
    hash,
    transaction::{
        BootDecision, boot,
        receipts::{FallbackReceipt, HealthIntent, Publication},
    },
};

fn child(parent: &File, name: &str, create: bool) -> io::Result<File> {
    if create {
        match fs::mkdirat(parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
            Ok(()) => {}
            Err(rustix::io::Errno::EXIST) => {}
            Err(error) => return Err(error.into()),
        }
        parent.sync_all()?;
    }
    let directory = File::from(fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let metadata = directory.metadata()?;
    if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o077 != 0 {
        return Err(invalid(
            "Receipt directories must be caller owned and private",
        ));
    }
    Ok(directory)
}

fn read(directory: &File, name: &str) -> io::Result<Vec<u8>> {
    let file = File::from(fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
        || metadata.len() > MAX_RECORD_BYTES as u64
    {
        return Err(invalid(
            "Receipt must be a bounded private standalone regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(invalid("Receipt exceeds its bound"));
    }
    Ok(bytes)
}

impl Journal {
    /// The installer has verified and synced all target artifacts and Recovery.
    /// Store publication evidence before making InstallPending authoritative.
    pub fn initialize_install(
        &self,
        layout: &Layout,
        manifest: &OfficialManifest,
        configuration: &crate::recovery::records::state::Configuration,
        recovery: &crate::recovery::records::recovery::OfficialRecovery,
        transaction: u64,
    ) -> io::Result<Transaction> {
        if self.read()?.is_some() {
            return Err(invalid("Installation journal already exists"));
        }
        let receipt = super::super::install::Installation::new(
            layout,
            manifest,
            configuration,
            recovery,
            transaction,
        )?;
        let digest = self.store_receipt(transaction, receipt.encode().as_bytes())?;
        self.initialize(manifest.id(), transaction, &digest)
    }

    pub fn initial_publication(
        &self,
        expected: &Transaction,
    ) -> io::Result<super::super::install::Installation> {
        self.require_current(expected)?;
        let digest = expected
            .publication_receipt_sha256()
            .ok_or_else(|| invalid("Missing initial publication receipt"))?;
        super::super::install::Installation::parse(
            &self.load_receipt(expected.transaction(), digest)?,
        )
    }

    /// Native identity/setup readiness and mount checks must precede this call.
    /// Initial publication uses a stable entry, so no counted blessing is needed.
    pub fn approve_initial(
        &self,
        expected: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
        configuration: &crate::recovery::records::state::Configuration,
        observed: &boot::Observation,
    ) -> io::Result<Transaction> {
        let publication = self.initial_publication(expected)?;
        publication.verify(expected, layout, manifest, configuration)?;
        if expected.phase() != super::super::Phase::InstallPending
            || observed.decision() != BootDecision::InitialSetup
            || observed.journal_generation() != expected.generation()
            || observed.selectors().entry.deployment_id() != publication.deployment_id()
            || observed.selectors().entry.transaction() != publication.transaction()
            || observed.entry_sha256() != publication.entry_sha256()
        {
            return Err(invalid(
                "Initial health observation differs from installation",
            ));
        }
        let digest = self.store_receipt(
            expected.transaction(),
            publication.health_receipt()?.as_bytes(),
        )?;
        self.apply(
            expected,
            Event::Approve {
                observed: &observed.selectors().entry,
                receipt: &digest,
            },
        )
    }

    pub fn verify_initial_health(
        &self,
        expected: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
        configuration: &crate::recovery::records::state::Configuration,
    ) -> io::Result<()> {
        let publication = self.initial_publication(expected)?;
        publication.verify(expected, layout, manifest, configuration)?;
        if expected.phase() != super::super::Phase::Healthy {
            return Err(invalid("Initial baseline is not healthy"));
        }
        let digest = expected
            .health_receipt_sha256()
            .ok_or_else(|| invalid("Missing initial health receipt"))?;
        if self.load_receipt(expected.transaction(), digest)?
            != publication.health_receipt()?.as_bytes()
        {
            return Err(invalid("Initial health receipt differs from installation"));
        }
        Ok(())
    }

    pub(crate) fn persist_receipt(
        &self,
        expected: &Transaction,
        bytes: &[u8],
    ) -> io::Result<String> {
        self.require_current(expected)?;
        self.store_receipt(expected.transaction(), bytes)
    }
    /// The new configuration tree and merge receipt are already synced and
    /// compatible. Failure leaves the authoritative source selection untouched.
    pub fn record_fallback_intent(
        &self,
        expected: &Transaction,
        layout: &Layout,
        failed: &OfficialManifest,
        target: &OfficialManifest,
        configuration: &crate::recovery::records::state::Configuration,
        receipt: &FallbackReceipt,
    ) -> io::Result<Transaction> {
        self.require_current(expected)?;
        let publication = self.publication(expected)?;
        publication.verify(expected, layout, failed)?;
        if let Some(existing) = self.fallback_intent(expected)? {
            if existing != *receipt {
                return Err(invalid("Conflicting durable fallback intent"));
            }
            receipt.verify_committed(expected, &publication, configuration, target)?;
            return Ok(expected.clone());
        }
        receipt.verify_before_intent(expected, &publication, configuration, target)?;
        let digest = self.store_receipt(expected.transaction(), receipt.encode().as_bytes())?;
        self.apply(expected, Event::FallbackIntent { receipt: &digest })
    }

    /// Distinguish a prior approval from an actual fallback decision. Corrupt,
    /// missing or unknown pointed receipts never mean absence of intent.
    pub fn fallback_intent(&self, expected: &Transaction) -> io::Result<Option<FallbackReceipt>> {
        self.require_current(expected)?;
        let Some(digest) = expected.health_receipt_sha256() else {
            return Ok(None);
        };
        let bytes = self.load_receipt(expected.transaction(), digest)?;
        if let Ok(receipt) = FallbackReceipt::parse(&bytes) {
            return Ok(Some(receipt));
        }
        let approval = HealthIntent::parse(&bytes)?;
        match expected.phase() {
            super::super::Phase::Trial => {
                approval.verify(expected, &self.publication(expected)?)?
            }
            super::super::Phase::RollbackRequired => {
                approval.verify_exhausted(expected, &self.publication(expected)?)?
            }
            super::super::Phase::Healthy => {}
            _ => return Err(invalid("Unexpected approval receipt phase")),
        }
        Ok(None)
    }

    pub fn resume_fallback(
        &self,
        expected: &Transaction,
        layout: &Layout,
        failed: &OfficialManifest,
        target: &OfficialManifest,
        configuration: &crate::recovery::records::state::Configuration,
    ) -> io::Result<Transaction> {
        self.require_current(expected)?;
        let publication = self.publication(expected)?;
        let digest = expected
            .health_receipt_sha256()
            .ok_or_else(|| invalid("Missing durable fallback intent"))?;
        let receipt = FallbackReceipt::parse(&self.load_receipt(expected.transaction(), digest)?)?;
        if expected.phase() == super::super::Phase::Healthy {
            boot::retire_failed(
                layout,
                expected,
                failed,
                target,
                &publication,
                &receipt,
                configuration,
            )?;
            return Ok(expected.clone());
        }
        boot::inspect_fallback(
            layout,
            expected,
            failed,
            target,
            &publication,
            &receipt,
            configuration,
        )?;
        boot::sync_entries()?;
        boot::inspect_fallback(
            layout,
            expected,
            failed,
            target,
            &publication,
            &receipt,
            configuration,
        )?;
        let completed = self.apply(
            expected,
            Event::Fallback {
                configuration,
                receipt: digest,
            },
        )?;
        boot::retire_failed(
            layout,
            &completed,
            failed,
            target,
            &publication,
            &receipt,
            configuration,
        )?;
        Ok(completed)
    }
    fn receipt_directory(&self, transaction: u64, create: bool) -> io::Result<File> {
        let receipts = child(&self.directory, "receipts", create)?;
        child(&receipts, &transaction.to_string(), create)
    }

    fn store_receipt(&self, transaction: u64, bytes: &[u8]) -> io::Result<String> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(invalid("Receipt exceeds its bound"));
        }
        let digest = sha256(bytes)?;
        let directory = self.receipt_directory(transaction, true)?;
        let name = format!("{digest}.record");
        match read(&directory, &name) {
            Ok(existing) if existing == bytes => {
                directory.sync_all()?;
                return Ok(digest);
            }
            Ok(_) => return Err(invalid("Existing receipt disagrees with its digest")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let pending = "receipt.pending";
        match fs::unlinkat(&directory, pending, AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => {}
            Err(error) => return Err(error.into()),
        }
        let mut file = File::from(fs::openat(
            &directory,
            pending,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )?);
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::renameat_with(
            &directory,
            pending,
            &directory,
            name.as_str(),
            fs::RenameFlags::NOREPLACE,
        )?;
        directory.sync_all()?;
        Ok(digest)
    }

    fn load_receipt(&self, transaction: u64, digest: &str) -> io::Result<Vec<u8>> {
        hash(digest)?;
        let directory = self.receipt_directory(transaction, false)?;
        let bytes = read(&directory, &format!("{digest}.record"))?;
        if sha256(&bytes)? != digest {
            return Err(invalid("Receipt bytes disagree with journal digest"));
        }
        Ok(bytes)
    }

    /// Caller verifies and syncs the published EFI entry and boot artifacts first.
    /// A receipt alone never establishes that publication succeeded.
    pub fn record_publication(
        &self,
        expected: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
        receipt: &Publication,
    ) -> io::Result<Transaction> {
        self.require_current(expected)?;
        receipt.verify(expected, layout, manifest)?;
        let digest = self.store_receipt(expected.transaction(), receipt.encode().as_bytes())?;
        self.apply(expected, Event::Published { receipt: &digest })
    }

    pub fn publication(&self, expected: &Transaction) -> io::Result<Publication> {
        self.require_current(expected)?;
        let digest = expected
            .publication_receipt_sha256()
            .ok_or_else(|| invalid("Missing publication receipt pointer"))?;
        Publication::parse(&self.load_receipt(expected.transaction(), digest)?)
    }

    pub fn health_intent(&self, expected: &Transaction) -> io::Result<HealthIntent> {
        self.require_current(expected)?;
        let digest = expected
            .health_receipt_sha256()
            .ok_or_else(|| invalid("Missing health intent pointer"))?;
        HealthIntent::parse(&self.load_receipt(expected.transaction(), digest)?)
    }

    /// Mandatory native health checks precede this call. Persist record, sync it,
    /// then commit its pointer before the caller may invoke native blessing.
    pub fn record_health_intent(
        &self,
        expected: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
        health: &HealthIntent,
    ) -> io::Result<Transaction> {
        self.require_current(expected)?;
        let publication = self.publication(expected)?;
        publication.verify(expected, layout, manifest)?;
        health.verify(expected, &publication)?;
        let digest = self.store_receipt(expected.transaction(), health.encode().as_bytes())?;
        self.apply(expected, Event::HealthIntent { receipt: &digest })
    }

    /// Finish a matching already blessed candidate, including after interruption.
    /// Does not invoke blessing or substitute for current native health checks.
    pub fn resume_approval(
        &self,
        expected: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
    ) -> io::Result<Transaction> {
        self.require_current(expected)?;
        let publication = self.publication(expected)?;
        let health = self.health_intent(expected)?;
        if expected.phase() == super::super::Phase::Healthy {
            boot::retire_previous(layout, expected, manifest, &publication, &health)?;
            return Ok(expected.clone());
        }
        let observed = boot::inspect_approval(layout, expected, manifest, &publication, &health)?;
        if observed.decision() != BootDecision::ResumeApproval {
            return Err(invalid(
                "Candidate has not been blessed to the approved stable entry",
            ));
        }
        boot::sync_approved_entry(layout, expected, manifest, &publication, &health)?;
        let completed = self.apply(
            expected,
            Event::Approve {
                observed: health.candidate(),
                receipt: &health.digest()?,
            },
        )?;
        boot::retire_previous(layout, &completed, manifest, &publication, &health)?;
        Ok(completed)
    }

    fn require_current(&self, expected: &Transaction) -> io::Result<()> {
        if self.read()?.as_ref() != Some(expected) {
            return Err(invalid("Stale or missing format 3 journal"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };

    #[test]
    fn pointed_receipts_distinguish_approval_exhaustion_and_fallback_without_ignoring_damage() {
        use crate::recovery::records::{
            state::Configuration,
            transaction::{Entry, Operation},
        };
        let path = std::env::current_dir()
            .unwrap()
            .join("target/transaction-receipt-tests")
            .join(format!("fallback-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let journal = Journal::open(&path).unwrap();
        let old = "a".repeat(64);
        let new = "c".repeat(64);
        let hash = "b".repeat(64);
        let initial = journal.initialize(&old, 1, &hash).unwrap();
        let healthy = journal
            .apply(
                &initial,
                Event::Approve {
                    observed: &Entry::new(&old, 1).unwrap(),
                    receipt: &hash,
                },
            )
            .unwrap();
        let preparing = journal
            .apply(
                &healthy,
                Event::Prepare {
                    operation: Operation::Update,
                    deployment_id: &new,
                },
            )
            .unwrap();
        let staged = journal.apply(&preparing, Event::Staged).unwrap();
        let trial = journal.apply(&staged, Event::BeginTrial).unwrap();
        let publication = Publication::parse(format!(
            "kind=linos-publication\nformat=1\ntransaction=2\nsource_journal_generation={}\nsource_config_generation=1\nhealthy_id={old}\nhealthy_entry_transaction=1\ncandidate_id={new}\ncandidate_entry_transaction=2\nentry_sha256={hash}\nkernel_sha256={hash}\ninitramfs_sha256={hash}\n", trial.generation()
        ).as_bytes()).unwrap();
        let digest = journal
            .persist_receipt(&trial, publication.encode().as_bytes())
            .unwrap();
        let published = journal
            .apply(&trial, Event::Published { receipt: &digest })
            .unwrap();
        let config = Configuration::selected(2, &new, Some(1), 1, &hash).unwrap();
        let selected = journal
            .apply(
                &published,
                Event::SelectCandidate {
                    configuration: &config,
                },
            )
            .unwrap();
        let approval = HealthIntent::parse(format!(
            "kind=linos-health\nformat=1\ntransaction=2\nsource_journal_generation={}\nsource_config_generation=1\nselected_config_generation=2\nhealthy_id={old}\nhealthy_entry_transaction=1\ncandidate_id={new}\ncandidate_entry_transaction=2\npublication_receipt_sha256={digest}\nentry_sha256={hash}\nkernel_sha256={hash}\ninitramfs_sha256={hash}\nobserved_entry_path=/loader/entries/linos-2-{new}+1-1.conf\ndecision=approve\n", selected.generation()
        ).as_bytes()).unwrap();
        let approval_digest = journal
            .persist_receipt(&selected, approval.encode().as_bytes())
            .unwrap();
        let intent = journal
            .apply(
                &selected,
                Event::HealthIntent {
                    receipt: &approval_digest,
                },
            )
            .unwrap();
        assert!(journal.fallback_intent(&intent).unwrap().is_none());
        let exhausted = journal.apply(&intent, Event::Exhausted).unwrap();
        assert!(journal.fallback_intent(&exhausted).unwrap().is_none());
        assert!(journal.fallback_intent(&intent).is_err());
        let fallback = FallbackReceipt::parse(format!(
            "kind=linos-health\nformat=1\ntransaction=2\nsource_journal_generation={}\nsource_config_generation=2\nselected_config_generation=3\nhealthy_id={old}\nhealthy_entry_transaction=1\ncandidate_id={new}\ncandidate_entry_transaction=2\npublication_receipt_sha256={digest}\nentry_sha256={hash}\nkernel_sha256={hash}\ninitramfs_sha256={hash}\nobserved_entry_path=/loader/entries/linos-1-{old}.conf\ndecision=fallback\n", exhausted.generation()
        ).as_bytes()).unwrap();
        let fallback_digest = journal
            .persist_receipt(&exhausted, fallback.encode().as_bytes())
            .unwrap();
        let recorded = journal
            .apply(
                &exhausted,
                Event::FallbackIntent {
                    receipt: &fallback_digest,
                },
            )
            .unwrap();
        assert_eq!(journal.fallback_intent(&recorded).unwrap(), Some(fallback));
        let record = path.join(format!("receipts/2/{fallback_digest}.record"));
        std::fs::write(&record, b"tampered fallback").unwrap();
        assert!(journal.fallback_intent(&recorded).is_err());
        std::fs::remove_file(&record).unwrap();
        assert!(journal.fallback_intent(&recorded).is_err());
        let unknown = journal
            .persist_receipt(&recorded, b"kind=unknown\nformat=1\n")
            .unwrap();
        let unknown_intent = journal
            .apply(&recorded, Event::FallbackIntent { receipt: &unknown })
            .unwrap();
        assert!(journal.fallback_intent(&unknown_intent).is_err());
        drop(journal);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn durable_receipts_are_immutable_digest_checked_and_private() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::current_dir()
            .unwrap()
            .join("target/transaction-receipt-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let journal = Journal::open(&path).unwrap();
        let bytes = b"kind=fixture\nformat=1\n";
        let digest = journal.store_receipt(7, bytes).unwrap();
        assert_eq!(journal.store_receipt(7, bytes).unwrap(), digest);
        assert_eq!(journal.load_receipt(7, &digest).unwrap(), bytes);
        let directory = path.join("receipts/7");
        std::fs::write(directory.join("receipt.pending"), b"interrupted").unwrap();
        assert_eq!(journal.load_receipt(7, &digest).unwrap(), bytes);
        let other = journal
            .store_receipt(7, b"kind=another\nformat=1\n")
            .unwrap();
        assert!(!directory.join("receipt.pending").exists());
        assert_ne!(digest, other);
        let record = directory.join(format!("{digest}.record"));
        std::fs::write(&record, b"tampered").unwrap();
        assert!(journal.load_receipt(7, &digest).is_err());
        assert!(journal.store_receipt(7, bytes).is_err());
        std::fs::write(&record, bytes).unwrap();
        std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(journal.load_receipt(7, &digest).is_err());
        drop(journal);
        std::fs::remove_dir_all(path).unwrap();
    }
}
