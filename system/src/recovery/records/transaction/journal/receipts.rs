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
        if expected.health_receipt_sha256() == Some(receipt.digest()?.as_str()) {
            receipt.verify_committed(expected, &publication, configuration, target)?;
            return Ok(expected.clone());
        }
        receipt.verify_before_intent(expected, &publication, configuration, target)?;
        let digest = self.store_receipt(expected.transaction(), receipt.encode().as_bytes())?;
        self.apply(expected, Event::FallbackIntent { receipt: &digest })
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
        let observed = boot::inspect_approval(layout, expected, manifest, &publication, &health)?;
        if observed.decision() != BootDecision::ResumeApproval {
            return Err(invalid(
                "Candidate has not been blessed to the approved stable entry",
            ));
        }
        boot::sync_approved_entry(layout, expected, manifest, &publication, &health)?;
        self.apply(
            expected,
            Event::Approve {
                observed: health.candidate(),
                receipt: &health.digest()?,
            },
        )
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
