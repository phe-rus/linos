//! Separate format 3 durable journal. No layout 1 migration or EFI writes.
mod receipts;

use super::{Event, Transaction};
use crate::recovery::{
    records::{MAX_RECORD_BYTES, invalid},
    update::journal::open_directory,
};
use rustix::fs::{self, AtFlags, FlockOperation, Mode, OFlags};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::unix::fs::MetadataExt,
    path::Path,
};

const NAME: &str = "journal";
const PENDING: &str = "journal.pending";

/// The supplied directory is the dedicated updates/v3 directory, independently
/// bound to the observed State volume by the caller. Never pass a layout 1 path.
pub struct Journal {
    directory: File,
}

impl Drop for Journal {
    fn drop(&mut self) {
        let _ = fs::flock(&self.directory, FlockOperation::Unlock);
    }
}

impl Journal {
    pub fn open(path: &Path) -> io::Result<Self> {
        let directory = open_directory(path)?;
        fs::flock(&directory, FlockOperation::NonBlockingLockExclusive)?;
        Ok(Self { directory })
    }

    pub fn read(&self) -> io::Result<Option<Transaction>> {
        let fd = match fs::openat(
            &self.directory,
            NAME,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o022 != 0
            || metadata.len() > MAX_RECORD_BYTES as u64
        {
            return Err(invalid(
                "Journal must be a bounded private standalone regular file",
            ));
        }
        let mut bytes = Vec::new();
        file.take(MAX_RECORD_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        Transaction::parse(&bytes).map(Some)
    }

    /// Collect live boot evidence while the volume journal remains locked. This
    /// is preflight only; it never changes selection, mounts data or blesses boot.
    pub fn inspect_boot(
        &self,
        layout: &crate::recovery::records::Layout,
    ) -> io::Result<super::boot::Observation> {
        let current = self
            .read()?
            .ok_or_else(|| invalid("Missing format 3 journal"))?;
        let approval = current.health_receipt_sha256().is_some()
            && (current.phase() == super::Phase::Trial
                || current.phase() == super::Phase::Healthy
                    && current.operation() != super::Operation::Install
                    && self.fallback_intent(&current)?.is_none());
        let observation = if approval {
            super::boot::inspect_intent(
                layout,
                &current,
                &self.publication(&current)?,
                &self.health_intent(&current)?,
            )?
        } else {
            super::boot::inspect(layout, &current)?
        };
        if self.read()?.as_ref() != Some(&current) {
            return Err(invalid("Journal changed during native boot inspection"));
        }
        Ok(observation)
    }

    /// Does not inspect installation evidence. The installer must verify and
    /// sync all target artifacts and independent Recovery before this call.
    pub fn initialize(
        &self,
        deployment_id: &str,
        transaction: u64,
        publication_receipt: &str,
    ) -> io::Result<Transaction> {
        if self.read()?.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Journal already initialized",
            ));
        }
        let initial = Transaction::installed(deployment_id, transaction, publication_receipt)?;
        self.replace(&initial)?;
        Ok(initial)
    }

    /// Compare the entire snapshot, including generation, before applying a
    /// typed transition. A parsed arbitrary replacement cannot bypass the model.
    pub fn apply(&self, expected: &Transaction, event: Event<'_>) -> io::Result<Transaction> {
        if self.read()?.as_ref() != Some(expected) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Stale or missing format 3 journal",
            ));
        }
        let next = expected.transition(event)?;
        if next != *expected {
            self.replace(&next)?;
        }
        Ok(next)
    }

    fn replace(&self, next: &Transaction) -> io::Result<()> {
        next.validate()?;
        let bytes = next.encode();
        // Defensive round trip before mutation. The pending path never supplies
        // authority, including after a crash between file sync and rename.
        if Transaction::parse(bytes.as_bytes())? != *next {
            return Err(invalid("Journal encoding mismatch"));
        }
        match fs::unlinkat(&self.directory, PENDING, AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => {}
            Err(error) => return Err(error.into()),
        }
        let fd = fs::openat(
            &self.directory,
            PENDING,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )?;
        let mut pending = File::from(fd);
        let result = (|| {
            pending.write_all(bytes.as_bytes())?;
            pending.sync_all()?;
            fs::renameat(&self.directory, PENDING, &self.directory, NAME)?;
            self.directory.sync_all()
        })();
        if result.is_err() {
            // A post-rename sync failure is an uncertain outcome. Re-read the
            // journal before retrying; never infer that the old state survived.
            let _ = fs::unlinkat(&self.directory, PENDING, AtFlags::empty());
        }
        result
    }
}

/// Inspection takes a shared lock and never creates a file or repairs state.
pub fn inspect(directory: &Path) -> io::Result<Option<Transaction>> {
    let directory = open_directory(directory)?;
    fs::flock(&directory, FlockOperation::NonBlockingLockShared)?;
    let journal = Journal { directory };
    journal.read()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::records::transaction::{Entry, Phase};
    use std::{
        os::unix::fs::{PermissionsExt, symlink},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[test]
    fn durable_transitions_refuse_stale_writers_and_ignore_pending_files() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::current_dir()
            .unwrap()
            .join("target/transaction-v3-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let id = "a".repeat(64);
        let receipt = "b".repeat(64);
        let journal = Journal::open(&path).unwrap();
        assert!(Journal::open(&path).is_err());
        assert!(journal.read().unwrap().is_none());
        let initial = journal.initialize(&id, 1, &receipt).unwrap();
        assert!(journal.initialize(&id, 1, &receipt).is_err());
        let entry = Entry::new(&id, 1).unwrap();
        let healthy = journal
            .apply(
                &initial,
                Event::Approve {
                    observed: &entry,
                    receipt: &receipt,
                },
            )
            .unwrap();
        assert_eq!(healthy.phase(), Phase::Healthy);
        assert!(journal.apply(&initial, Event::RequireRecovery).is_err());
        std::fs::write(path.join(PENDING), b"interrupted invalid bytes").unwrap();
        assert_eq!(journal.read().unwrap(), Some(healthy.clone()));
        let prepared = journal
            .apply(
                &healthy,
                Event::Prepare {
                    operation: super::super::Operation::Update,
                    deployment_id: &"c".repeat(64),
                },
            )
            .unwrap();
        assert!(!path.join(PENDING).exists());
        drop(journal);
        assert_eq!(inspect(&path).unwrap(), Some(prepared));
        let original = path.join("retained");
        std::fs::rename(path.join(NAME), &original).unwrap();
        symlink(&original, path.join(NAME)).unwrap();
        assert!(inspect(&path).is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
}
