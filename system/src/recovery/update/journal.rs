//! Durable journal storage only. Callers supply verified update events; this
//! module never authenticates payloads, selects boot targets, or writes slots.

use super::{Event, MAX_TRIAL_BOOTS, Phase, Slot, transition, valid_phase};
use rustix::fs::{self, AtFlags, FlockOperation, Mode, OFlags};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path},
};

const NAME: &str = "update-state";
const PENDING: &str = "update-state.pending";
const LIMIT: u64 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub generation: u64,
    pub phase: Phase,
}

/// Holds an exclusive directory lock through read, validation and replacement.
/// Every writer must use this protocol. The caller owns directory provisioning.
pub struct Journal {
    directory: File,
}

impl Drop for Journal {
    fn drop(&mut self) {
        // Explicit unlock also releases copies briefly inherited by a child
        // between fork and exec. Closing only this descriptor would not.
        let _ = fs::flock(&self.directory, FlockOperation::Unlock);
    }
}

impl Journal {
    pub fn open(directory: &Path) -> io::Result<Self> {
        let directory = open_directory(directory)?;
        fs::flock(&directory, FlockOperation::NonBlockingLockExclusive)?;
        Ok(Self { directory })
    }

    pub fn read(&self) -> io::Result<Option<Snapshot>> {
        read_at(&self.directory, NAME)
    }

    /// Initialization never infers a slot or overwrites an existing journal.
    pub fn initialize(&self, active: Slot) -> io::Result<Snapshot> {
        if self.read()?.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Journal already initialized",
            ));
        }
        let snapshot = Snapshot {
            generation: 0,
            phase: Phase::Healthy { active },
        };
        self.replace(snapshot)?;
        Ok(snapshot)
    }

    /// Compare generation AND phase, so a stale writer cannot reuse a state
    /// that returned to the same phase after another completed transaction.
    pub fn apply(&self, expected: Snapshot, event: Event) -> io::Result<Snapshot> {
        if self.read()? != Some(expected) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Stale or missing update journal",
            ));
        }
        let next = Snapshot {
            generation: expected
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("Journal generation exhausted"))?,
            phase: transition(expected.phase, event).map_err(invalid)?,
        };
        self.replace(next)?;
        Ok(next)
    }

    fn replace(&self, snapshot: Snapshot) -> io::Result<()> {
        let bytes = encode(snapshot)?;
        // A pending file from an interrupted write is never authoritative.
        // Unlink the directory entry, never follow it or open its target.
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
            // A successful rename alone does not make the directory entry durable.
            self.directory.sync_all()
        })();
        if result.is_err() {
            // A failure after rename has an uncertain commit outcome. Callers
            // must re-read before retrying; never assume the old state survived.
            let _ = fs::unlinkat(&self.directory, PENDING, AtFlags::empty());
        }
        result
    }
}

/// Inspection uses the same bounded decoder and descriptor-based file access,
/// but does not create files. A missing directory or journal means no record.
pub fn inspect(path: &Path) -> io::Result<Option<Snapshot>> {
    let name = path
        .file_name()
        .ok_or_else(|| invalid("Missing journal filename"))?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Missing journal directory"))?;
    let directory = match open_directory(parent) {
        Ok(directory) => directory,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    fs::flock(&directory, FlockOperation::NonBlockingLockShared)?;
    let journal = Journal { directory };
    read_at(&journal.directory, name)
}

pub(super) fn open_directory(path: &Path) -> io::Result<File> {
    if !path.is_absolute() {
        return Err(invalid("Journal directory must be absolute"));
    }
    let mut directory = File::from(fs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    for component in path.components() {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                let metadata = directory.metadata()?;
                // Root-owned sticky /tmp is allowed as an ancestor, never as
                // the journal directory itself. Other writable ancestors fail.
                if metadata.uid() != 0 && metadata.uid() != rustix::process::geteuid().as_raw()
                    || metadata.mode() & 0o022 != 0
                        && !(metadata.uid() == 0 && metadata.mode() & 0o1000 != 0)
                {
                    return Err(invalid("Untrusted journal ancestor directory"));
                }
                directory = File::from(fs::openat(
                    &directory,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?);
            }
            _ => {
                return Err(invalid(
                    "Journal directory cannot contain traversal components",
                ));
            }
        }
    }
    let metadata = directory.metadata()?;
    if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o022 != 0 {
        return Err(invalid(
            "Journal directory must be owned by the caller and not writable by others",
        ));
    }
    Ok(directory)
}

fn read_at(directory: &File, name: impl rustix::path::Arg) -> io::Result<Option<Snapshot>> {
    let fd = match fs::openat(
        directory,
        name,
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
    {
        return Err(invalid(
            "Journal must be a caller-owned regular file without hard links or other writers",
        ));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    decode_snapshot(&bytes).map(Some).map_err(invalid)
}

fn encode(snapshot: Snapshot) -> io::Result<String> {
    if !valid_phase(snapshot.phase) {
        return Err(invalid("Cannot persist an inconsistent update phase"));
    }
    let slot = |value| match value {
        Slot::A => "A",
        Slot::B => "B",
    };
    let (phase, active, candidate, attempts) = match snapshot.phase {
        Phase::Healthy { active } => ("healthy", active, None, None),
        Phase::Staged { active, candidate } => ("staged", active, Some(candidate), None),
        Phase::Trial {
            active,
            candidate,
            attempts_left,
        } => ("trial", active, Some(candidate), Some(attempts_left)),
        Phase::RollbackRequired { active, failed } => ("rollback", active, Some(failed), None),
    };
    let mut text = format!(
        "format=2\ngeneration={}\nphase={phase}\nactive={}\n",
        snapshot.generation,
        slot(active)
    );
    if let Some(candidate) = candidate {
        text.push_str(&format!("candidate={}\n", slot(candidate)));
    }
    if let Some(attempts) = attempts {
        text.push_str(&format!("attempts_left={attempts}\n"));
    }
    Ok(text)
}

fn invalid(reason: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

/// Format v1 is intentionally narrow and fails closed on unknown/duplicate
/// fields. It is not a signed transaction journal or a bootloader protocol.
pub(super) fn decode_snapshot(input: &[u8]) -> Result<Snapshot, &'static str> {
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
    let generation = match fields.get("format") {
        Some(&"1") => 0,
        Some(&"2") => fields
            .remove("generation")
            .ok_or("Missing journal generation")?
            .parse::<u64>()
            .map_err(|_| "Invalid journal generation")?,
        _ => return Err("Unsupported update journal format"),
    };
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
            Phase::Trial {
                active,
                candidate,
                attempts_left,
            }
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
    Ok(Snapshot {
        generation,
        phase: result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "linos-journal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn open(&self) -> Journal {
            Journal::open(&self.0).unwrap()
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn persists_update_and_rollback_across_reopens() {
        let directory = Directory::new();
        let initial = directory.open().initialize(Slot::A).unwrap();
        let staged = directory
            .open()
            .apply(
                initial,
                Event::Stage {
                    candidate: Slot::B,
                    verified: true,
                },
            )
            .unwrap();
        let trial = directory.open().apply(staged, Event::BeginTrial).unwrap();
        let retry = directory
            .open()
            .apply(trial, Event::BootHealth { passed: false })
            .unwrap();
        let rollback = directory
            .open()
            .apply(retry, Event::BootHealth { passed: false })
            .unwrap();
        let restored = directory
            .open()
            .apply(rollback, Event::ConfirmRollback)
            .unwrap();
        assert_eq!(restored.phase, initial.phase);
        assert_eq!(restored.generation, 5);
        assert_eq!(inspect(&directory.0.join(NAME)).unwrap(), Some(restored));
        assert!(
            directory
                .open()
                .apply(
                    initial,
                    Event::Stage {
                        candidate: Slot::B,
                        verified: true
                    }
                )
                .is_err()
        );
        let staged = directory
            .open()
            .apply(
                restored,
                Event::Stage {
                    candidate: Slot::B,
                    verified: true,
                },
            )
            .unwrap();
        let trial = directory.open().apply(staged, Event::BeginTrial).unwrap();
        let healthy = directory
            .open()
            .apply(trial, Event::BootHealth { passed: true })
            .unwrap();
        assert_eq!(directory.open().read().unwrap(), Some(healthy));
        assert_eq!(healthy.phase, Phase::Healthy { active: Slot::B });
        assert_eq!(
            fs::metadata(directory.0.join(NAME)).unwrap().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn refuses_competing_writers_and_preserves_state_after_rejected_events() {
        let directory = Directory::new();
        let journal = directory.open();
        let initial = journal.initialize(Slot::A).unwrap();
        assert!(Journal::open(&directory.0).is_err());
        assert!(journal.initialize(Slot::B).is_err());
        assert!(
            journal
                .apply(
                    initial,
                    Event::Stage {
                        candidate: Slot::B,
                        verified: false
                    }
                )
                .is_err()
        );
        assert!(
            journal
                .apply(
                    initial,
                    Event::Stage {
                        candidate: Slot::A,
                        verified: true
                    }
                )
                .is_err()
        );
        assert_eq!(journal.read().unwrap(), Some(initial));
    }

    #[test]
    fn rejects_corrupt_unsafe_and_oversized_files_without_reinitializing() {
        let directory = Directory::new();
        let journal = directory.open();
        for data in [
            b"partial".as_slice(),
            &[b'x'; 257],
            b"format=2\ngeneration=0\ngeneration=1\nphase=healthy\nactive=A\n",
        ] {
            fs::write(directory.0.join(NAME), data).unwrap();
            assert!(journal.read().is_err());
            assert!(journal.initialize(Slot::A).is_err());
            assert_eq!(fs::read(directory.0.join(NAME)).unwrap(), data);
        }
        fs::remove_file(directory.0.join(NAME)).unwrap();
        let target = directory.0.join("target");
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, directory.0.join(NAME)).unwrap();
        assert!(journal.read().is_err());
        assert!(journal.initialize(Slot::A).is_err());
        fs::remove_file(directory.0.join(NAME)).unwrap();
        fs::hard_link(&target, directory.0.join(NAME)).unwrap();
        assert!(journal.read().is_err());
        fs::remove_file(directory.0.join(NAME)).unwrap();
        fs::write(
            directory.0.join(NAME),
            b"format=1\nphase=healthy\nactive=A\n",
        )
        .unwrap();
        fs::set_permissions(directory.0.join(NAME), fs::Permissions::from_mode(0o666)).unwrap();
        assert!(journal.read().is_err());
        assert_eq!(fs::read(&target).unwrap(), b"untouched");
    }

    #[test]
    fn rejects_symlinked_or_shared_directories() {
        let directory = Directory::new();
        let alias = directory.0.join("alias");
        symlink(&directory.0, &alias).unwrap();
        assert!(Journal::open(&alias).is_err());
        assert!(Journal::open(&alias.join("child")).is_err());
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(Journal::open(&directory.0).is_err());
    }

    #[test]
    fn legacy_records_are_readable_and_advance_to_version_two() {
        let directory = Directory::new();
        fs::write(
            directory.0.join(NAME),
            b"format=1\nphase=healthy\nactive=A\n",
        )
        .unwrap();
        let journal = directory.open();
        let original = journal.read().unwrap().unwrap();
        let next = journal
            .apply(
                original,
                Event::Stage {
                    candidate: Slot::B,
                    verified: true,
                },
            )
            .unwrap();
        assert_eq!(next.generation, 1);
        assert!(
            fs::read_to_string(directory.0.join(NAME))
                .unwrap()
                .starts_with("format=2\n")
        );
        assert_eq!(journal.read().unwrap(), Some(next));
    }

    #[test]
    fn generation_overflow_is_refused_without_writing() {
        let directory = Directory::new();
        let snapshot = Snapshot {
            generation: u64::MAX,
            phase: Phase::Healthy { active: Slot::A },
        };
        fs::write(directory.0.join(NAME), encode(snapshot).unwrap()).unwrap();
        let journal = directory.open();
        assert!(
            journal
                .apply(
                    snapshot,
                    Event::Stage {
                        candidate: Slot::B,
                        verified: true
                    }
                )
                .is_err()
        );
        assert_eq!(journal.read().unwrap(), Some(snapshot));
    }

    #[test]
    fn interrupted_writer_child() {
        let Some(path) = std::env::var_os("LINOS_JOURNAL_CRASH_TEST") else {
            return;
        };
        let directory = std::path::PathBuf::from(path);
        let _lock = Journal::open(&directory).unwrap();
        let mut pending = File::create(directory.join(PENDING)).unwrap();
        pending.write_all(b"format=2\ngeneration=").unwrap();
        pending.sync_all().unwrap();
        // Exit without running Rust destructors, as when the writer is killed.
        std::process::exit(73);
    }

    #[test]
    fn process_exit_preserves_last_commit_and_retry_discards_partial_pending_file() {
        let directory = Directory::new();
        let initial = directory.open().initialize(Slot::A).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "recovery::update::journal::tests::interrupted_writer_child",
            ])
            .env("LINOS_JOURNAL_CRASH_TEST", &directory.0)
            .output()
            .unwrap()
            .status;
        assert_eq!(status.code(), Some(73));
        let journal = directory.open();
        assert_eq!(journal.read().unwrap(), Some(initial));
        assert!(directory.0.join(PENDING).exists());
        let next = journal
            .apply(
                initial,
                Event::Stage {
                    candidate: Slot::B,
                    verified: true,
                },
            )
            .unwrap();
        assert_eq!(journal.read().unwrap(), Some(next));
        assert!(!directory.0.join(PENDING).exists());
    }
}
