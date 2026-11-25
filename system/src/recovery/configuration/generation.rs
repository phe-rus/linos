//! Prepare from the latest journal selection. No journal pointer or live mount
//! changes here; publication is a separate generation checked operation.
use super::{Content, Tree, invalid, tree};
use crate::recovery::records::{
    deployment::{sha256, source::Loaded},
    metadata::accounts::NativeAccounts,
    transaction::{Phase, Transaction},
};
use std::{
    io,
    path::{Path, PathBuf},
};

pub struct Inputs<'a> {
    pub volume: &'a Path,
    pub journal: &'a Transaction,
    pub configuration: &'a crate::recovery::records::state::Configuration,
    pub source: &'a Loaded,
    pub target: &'a Loaded,
    pub base_vendor: &'a Path,
    pub current: &'a Path,
    pub target_vendor: &'a Path,
    pub workspace: &'a Path,
}

/// Keep every published or reserved generation, including an unselected failed
/// trial. Reuse only this exact source/target intent; otherwise allocate above
/// all retained records. The caller holds the volume journal lock.
fn allocation(
    volume: &Path,
    snapshot: &Transaction,
    target_id: &str,
    schema: u32,
) -> io::Result<u64> {
    use crate::recovery::{
        records::{deployment::source, state::Configuration},
        update::journal::open_directory,
    };
    use std::os::fd::AsRawFd;
    let records = open_directory(&volume.join("State/var/lib/linos/configuration"))?;
    let mut maximum = snapshot.selected_config_generation();
    let mut matching = None;
    for (count, entry) in
        std::fs::read_dir(format!("/proc/self/fd/{}", records.as_raw_fd()))?.enumerate()
    {
        if count >= 4096 {
            return Err(invalid("Configuration record inventory exceeds its bound"));
        }
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| invalid("Invalid configuration record filename"))?;
        let Some(stem) = name.strip_suffix(".record") else {
            continue;
        };
        let generation = if let Some(prepared) = stem.strip_prefix("prepared-") {
            let Some((transaction, generation)) = prepared.split_once('-') else {
                return Err(invalid("Invalid prepared generation filename"));
            };
            super::super::records::state::positive(transaction)?;
            super::super::records::state::positive(generation)?
        } else {
            super::super::records::state::positive(stem)?
        };
        maximum = maximum.max(generation);
        if generation <= snapshot.selected_config_generation() || !stem.starts_with("prepared-") {
            continue;
        }
        let bytes = source::read_at(&records, &name, 64 * 1024)?;
        let text = std::str::from_utf8(&bytes).map_err(invalid)?;
        // Old and abandoned intents reserve their number but cannot supply the
        // current selection. Matching fields still undergo the full parser.
        let fields = [
            format!("transaction={}", snapshot.transaction()),
            format!("source_journal_generation={}", snapshot.generation()),
            format!(
                "source_config_generation={}",
                snapshot.selected_config_generation()
            ),
            format!("source_deployment_id={}", snapshot.selected_deployment_id()),
            format!("target_deployment_id={target_id}"),
        ];
        if !fields
            .iter()
            .all(|field| text.lines().any(|line| line == field))
        {
            continue;
        }
        if name != format!("prepared-{}-{generation}.record", snapshot.transaction()) {
            return Err(invalid(
                "Prepared generation filename disagrees with its receipt",
            ));
        }
        let configuration = Configuration::selected(
            generation,
            target_id,
            Some(snapshot.selected_config_generation()),
            schema,
            &sha256(&bytes)?,
        )?;
        configuration.verify_prepared_merge(&bytes, snapshot)?;
        if matching.replace(generation).is_some() {
            return Err(invalid("Multiple matching configuration preparations"));
        }
    }
    matching.map(Ok).unwrap_or_else(|| {
        maximum
            .checked_add(1)
            .ok_or_else(|| invalid("Configuration generation exhausted"))
    })
}

pub struct Prepared {
    directory: PathBuf,
    generation: u64,
    receipt: String,
    snapshot: Transaction,
    configuration: crate::recovery::records::state::Configuration,
    source_digest: String,
    target_digest: String,
}

fn record(parent: &std::fs::File, name: &str, bytes: &[u8]) -> io::Result<()> {
    use crate::recovery::records::deployment::source;
    use rustix::fs::{self, Mode, OFlags};
    use std::{fs::File, io::Write};
    match source::read_at(parent, name, 64 * 1024) {
        Ok(existing) if existing == bytes => return parent.sync_all(),
        Ok(_) => {
            return Err(invalid(
                "Prepared generation record conflicts with existing intent",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match fs::unlinkat(parent, "configuration.pending", fs::AtFlags::empty()) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => {}
        Err(error) => return Err(error.into()),
    }
    let mut file = File::from(fs::openat(
        parent,
        "configuration.pending",
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?);
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::renameat_with(
        parent,
        "configuration.pending",
        parent,
        name,
        fs::RenameFlags::NOREPLACE,
    )?;
    parent.sync_all()
}

fn database<'a>(tree: &'a Tree, name: &str) -> io::Result<&'a [u8]> {
    let object = tree
        .get(name)
        .ok_or_else(|| invalid("Missing current native account database"))?;
    match &object.content {
        Content::File(bytes) if object.uid == 0 && object.mode & 0o022 == 0 => Ok(bytes),
        _ => Err(invalid(
            "Native account database must be protected and regular",
        )),
    }
}

pub fn accounts(tree: &Tree) -> io::Result<NativeAccounts> {
    NativeAccounts::parse(
        database(tree, "passwd")?,
        database(tree, "shadow")?,
        database(tree, "group")?,
        database(tree, "gshadow")?,
    )
}

/// Length framed binary hashing avoids ambiguous filename/content separators.
/// Only the digest enters receipts; credentials never enter diagnostics.
pub fn digest(tree: &Tree) -> io::Result<String> {
    let mut bytes = Vec::new();
    for (path, object) in tree {
        bytes.extend_from_slice(&(path.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        for number in [object.uid, object.gid, object.mode] {
            bytes.extend_from_slice(&number.to_le_bytes());
        }
        let (tag, value): (u8, &[u8]) = match &object.content {
            Content::Directory => (0, &[]),
            Content::File(bytes) => (1, bytes),
            Content::Symlink(target) => (2, target.as_bytes()),
        };
        bytes.push(tag);
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value);
    }
    sha256(&bytes)
}

/// Recover publication interrupted before journal selection. Verify both trees
/// while they are quiescent; never use this for an already selected generation.
pub fn published(
    volume: &Path,
    snapshot: &Transaction,
    target: &Loaded,
    schema: u32,
) -> io::Result<Option<crate::recovery::records::state::Configuration>> {
    let expected_target = match snapshot.phase() {
        Phase::Trial => snapshot.candidate(),
        Phase::RollbackRequired => snapshot.healthy(),
        _ => None,
    }
    .ok_or_else(|| invalid("Prepared configuration requires a trial or exhausted fallback"))?;
    if expected_target.deployment_id() != target.manifest().id()
        || snapshot.phase() == Phase::Trial
            && snapshot.selected_deployment_id() == target.manifest().id()
        || schema < target.manifest().deployment().config_schema_min
        || schema > target.manifest().deployment().config_schema_max
    {
        return Err(invalid(
            "Prepared configuration target or schema is incompatible",
        ));
    }
    let configuration = published_configuration(volume, snapshot, target.manifest().id(), schema)?;
    if let Some(configuration) = &configuration {
        configuration.require_selection(
            configuration.generation(),
            target.manifest().id(),
            target.manifest().deployment(),
        )?;
    }
    Ok(configuration)
}

fn published_configuration(
    volume: &Path,
    snapshot: &Transaction,
    target_id: &str,
    schema: u32,
) -> io::Result<Option<crate::recovery::records::state::Configuration>> {
    let next = allocation(volume, snapshot, target_id, schema)?;
    published_configuration_at(
        volume,
        snapshot,
        target_id,
        schema,
        snapshot.generation(),
        next,
    )
}

/// Resume an already committed fallback intent. Its complete configuration
/// record must predate the intent; absence is corruption, not a fresh merge.
pub fn published_fallback(
    volume: &Path,
    snapshot: &Transaction,
    target: &Loaded,
    schema: u32,
    receipt: &crate::recovery::records::transaction::receipts::FallbackReceipt,
    publication: &crate::recovery::records::transaction::receipts::Publication,
) -> io::Result<crate::recovery::records::state::Configuration> {
    use crate::recovery::{
        records::{deployment::source, state::Configuration},
        update::journal::open_directory,
    };
    let next = receipt.configuration_generation();
    let records = open_directory(&volume.join("State/var/lib/linos/configuration"))?;
    let configuration = Configuration::parse(&source::read_at(
        &records,
        &format!("{next}.record"),
        64 * 1024,
    )?)?;
    receipt.verify_committed(snapshot, publication, &configuration, target.manifest())?;
    published_configuration_at(
        volume,
        snapshot,
        target.manifest().id(),
        schema,
        receipt.preparation_generation(),
        next,
    )?
    .ok_or_else(|| invalid("Committed fallback is missing its published configuration"))
}

fn published_configuration_at(
    volume: &Path,
    snapshot: &Transaction,
    target_id: &str,
    schema: u32,
    source_generation: u64,
    next: u64,
) -> io::Result<Option<crate::recovery::records::state::Configuration>> {
    use crate::recovery::records::state::Configuration;
    use crate::recovery::{records::deployment::source, update::journal::open_directory};
    let records = open_directory(&volume.join("State/var/lib/linos/configuration"))?;
    let name = format!("{next}.record");
    let (configuration, complete) = match source::read_at(&records, &name, 64 * 1024) {
        Ok(bytes) => (Configuration::parse(&bytes)?, true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let intent = match source::read_at(
                &records,
                &format!("prepared-{}-{next}.record", snapshot.transaction()),
                64 * 1024,
            ) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
            };
            let configuration = Configuration::selected(
                next,
                target_id,
                Some(snapshot.selected_config_generation()),
                schema,
                &sha256(&intent)?,
            )?;
            configuration.verify_prepared_merge_at(&intent, snapshot, source_generation)?;
            match tree::snapshot(&volume.join(format!("State/config/{next}"))) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
            }
            (configuration, false)
        }
        Err(error) => return Err(error),
    };
    if configuration.generation() != next
        || configuration.deployment_id() != target_id
        || configuration.schema() != schema
    {
        return Err(invalid(
            "Published configuration differs from expected target",
        ));
    }
    let receipts = open_directory(&volume.join("State/var/lib/linos/configuration/receipts"))?;
    let receipt = source::read_at(
        &receipts,
        &format!("{}.record", configuration.merge_receipt_sha256()),
        64 * 1024,
    )?;
    let expected = configuration.verify_prepared_merge_at(&receipt, snapshot, source_generation)?;
    let source_digest = digest(&tree::snapshot(&volume.join(format!(
        "State/config/{}",
        snapshot.selected_config_generation()
    )))?)?;
    if !receipt
        .split(|byte| *byte == b'\n')
        .any(|line| line == format!("source_tree_sha256={source_digest}").as_bytes())
        || digest(&tree::snapshot(
            &volume.join(format!("State/config/{next}")),
        )?)? != expected
    {
        return Err(invalid(
            "Published configuration no longer matches its merge receipt",
        ));
    }
    if !complete {
        record(&records, &name, configuration.encode().as_bytes())?;
    }
    Ok(Some(configuration))
}

impl Prepared {
    pub fn build(input: Inputs<'_>) -> io::Result<Self> {
        let journal = input.journal;
        journal.require_selected_configuration(input.configuration, input.source.manifest())?;
        let schema = input.configuration.schema();
        if schema < input.target.manifest().deployment().config_schema_min
            || schema > input.target.manifest().deployment().config_schema_max
        {
            return Err(invalid(
                "Target cannot read the current configuration schema",
            ));
        }
        let expected_target = match journal.phase() {
            Phase::Trial => journal.candidate(),
            Phase::RollbackRequired => journal.healthy(),
            _ => None,
        }
        .ok_or_else(|| {
            invalid("Configuration preparation requires an authorized trial or exhausted fallback")
        })?;
        if journal.selected_deployment_id() != input.source.manifest().id()
            || expected_target.deployment_id() != input.target.manifest().id()
            || journal.phase() == Phase::Trial
                && journal.selected_deployment_id() == expected_target.deployment_id()
        {
            return Err(invalid(
                "Configuration source/target disagrees with journal, or selected trial must be reused",
            ));
        }
        let generation = allocation(input.volume, journal, input.target.manifest().id(), schema)?;
        let current_tree = tree::snapshot(input.current)?;
        let current_accounts = accounts(&current_tree)?;
        input
            .target
            .requirements()
            .accounts
            .plan(&input.source.requirements().accounts, &current_accounts)?;
        let destination = input.workspace.join("etc");
        tree::prepare(
            input.base_vendor,
            input.current,
            input.target_vendor,
            &destination,
        )?;
        input
            .target
            .requirements()
            .accounts
            .create_required(&input.source.requirements().accounts, input.workspace)?;
        let prepared = tree::snapshot(&destination)?;
        let prepared_accounts = accounts(&prepared)?;
        if !input
            .target
            .requirements()
            .accounts
            .plan(&input.source.requirements().accounts, &prepared_accounts)?
            .is_empty()
        {
            return Err(invalid(
                "Prepared configuration still has missing required accounts",
            ));
        }
        for service in input.target.requirements().services.services() {
            if service
                .paths()
                .iter()
                .any(|path| !prepared_accounts.owns(path.uid, path.gid))
            {
                return Err(invalid(
                    "Service data requires an unregistered numeric account owner",
                ));
            }
        }
        if tree::snapshot(input.current)? != current_tree {
            return Err(invalid("Current configuration changed during preparation"));
        }
        let receipt = format!(
            "kind=linos-configuration-merge\nformat=1\ntransaction={}\nsource_journal_generation={}\nsource_config_generation={}\nselected_config_generation={}\nschema={}\nsource_deployment_id={}\ntarget_deployment_id={}\nsource_tree_sha256={}\ntarget_tree_sha256={}\n",
            journal.transaction(),
            journal.generation(),
            journal.selected_config_generation(),
            generation,
            schema,
            input.source.manifest().id(),
            input.target.manifest().id(),
            digest(&current_tree)?,
            digest(&prepared)?
        );
        let configuration = crate::recovery::records::state::Configuration::selected(
            generation,
            input.target.manifest().id(),
            Some(journal.selected_config_generation()),
            schema,
            &sha256(receipt.as_bytes())?,
        )?;
        Ok(Self {
            directory: destination,
            generation,
            receipt,
            snapshot: journal.clone(),
            configuration,
            source_digest: digest(&current_tree)?,
            target_digest: digest(&prepared)?,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn receipt(&self) -> &str {
        &self.receipt
    }
    pub fn receipt_sha256(&self) -> io::Result<String> {
        sha256(self.receipt.as_bytes())
    }
    /// Publish only the prepared tree and its records. The journal remains the
    /// sole selector; callers then use SelectCandidate or fallback reconciliation.
    pub fn publish(
        &self,
        volume: &Path,
        journal: &crate::recovery::records::transaction::journal::Journal,
    ) -> io::Result<crate::recovery::records::state::Configuration> {
        use crate::recovery::update::journal::open_directory;
        use rustix::fs::{self, Mode, OFlags};
        use std::fs::File;
        if journal.read()?.as_ref() != Some(&self.snapshot) {
            return Err(invalid("Stale configuration publication"));
        }
        self.configuration
            .verify_prepared_merge(self.receipt.as_bytes(), &self.snapshot)?;
        let current = volume.join(format!(
            "State/config/{}",
            self.snapshot.selected_config_generation()
        ));
        if digest(&tree::snapshot(&current)?)? != self.source_digest {
            return Err(invalid("Source configuration changed before publication"));
        }
        let parent = open_directory(&volume.join("State/config"))?;
        let records = open_directory(&volume.join("State/var/lib/linos/configuration"))?;
        let receipt_digest = journal.persist_receipt(&self.snapshot, self.receipt.as_bytes())?;
        if receipt_digest != self.configuration.merge_receipt_sha256() {
            return Err(invalid("Prepared receipt identity mismatch"));
        }
        record(
            &records,
            &format!(
                "prepared-{}-{}.record",
                self.snapshot.transaction(),
                self.generation
            ),
            self.receipt.as_bytes(),
        )?;
        match fs::mkdirat(&records, "receipts", Mode::RUSR | Mode::WUSR | Mode::XUSR) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(error) => return Err(error.into()),
        }
        records.sync_all()?;
        let receipts = File::from(fs::openat(
            &records,
            "receipts",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        record(
            &receipts,
            &format!("{receipt_digest}.record"),
            self.receipt.as_bytes(),
        )?;
        let destination = volume.join(format!("State/config/{}", self.generation));
        if destination.try_exists()? {
            if digest(&tree::snapshot(&destination)?)? != self.target_digest {
                return Err(invalid(
                    "Existing prepared generation differs from its intent",
                ));
            }
        } else {
            if digest(&tree::snapshot(&self.directory)?)? != self.target_digest {
                return Err(invalid("Prepared configuration changed before publication"));
            }
            let source_parent = open_directory(
                self.directory
                    .parent()
                    .ok_or_else(|| invalid("Missing prepared parent"))?,
            )?;
            fs::renameat_with(
                &source_parent,
                "etc",
                &parent,
                self.generation.to_string(),
                fs::RenameFlags::NOREPLACE,
            )?;
            parent.sync_all()?;
        }
        record(
            &records,
            &format!("{}.record", self.generation),
            self.configuration.encode().as_bytes(),
        )?;
        if journal.read()?.as_ref() != Some(&self.snapshot) {
            return Err(invalid("Journal changed before configuration selection"));
        }
        Ok(self.configuration.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::records::{
        state::Configuration,
        transaction::{Entry, Event, Operation, journal::Journal},
    };
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    #[test]
    fn durable_generation_publication_preserves_source_and_waits_for_journal_selection() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let volume = std::env::current_dir()
            .unwrap()
            .join("target/configuration-generation-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let journal_path = volume.join("State/var/lib/linos/updates/v3");
        let current = volume.join("State/config/1");
        let work = volume.join("State/config/work/etc");
        for path in [
            &journal_path,
            &current,
            &work,
            &volume.join("State/var/lib/linos/configuration"),
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        std::fs::set_permissions(&volume, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(current.join("shadow"), b"synthetic current credential").unwrap();
        std::fs::write(work.join("shadow"), b"synthetic merged credential").unwrap();
        let journal = Journal::open(&journal_path).unwrap();
        let id = "a".repeat(64);
        let receipt_id = "b".repeat(64);
        let initial = journal.initialize(&id, 1, &receipt_id).unwrap();
        let healthy = journal
            .apply(
                &initial,
                Event::Approve {
                    observed: &Entry::new(&id, 1).unwrap(),
                    receipt: &receipt_id,
                },
            )
            .unwrap();
        let candidate = healthy
            .prepare(Operation::Update, &"c".repeat(64))
            .unwrap()
            .staged()
            .unwrap()
            .begin_trial()
            .unwrap()
            .published(&receipt_id)
            .unwrap();
        // Exercise the real durable typed transitions rather than replacing state.
        let preparing = journal
            .apply(
                &healthy,
                Event::Prepare {
                    operation: Operation::Update,
                    deployment_id: &"c".repeat(64),
                },
            )
            .unwrap();
        let staged = journal.apply(&preparing, Event::Staged).unwrap();
        let trial = journal.apply(&staged, Event::BeginTrial).unwrap();
        let snapshot = journal
            .apply(
                &trial,
                Event::Published {
                    receipt: &receipt_id,
                },
            )
            .unwrap();
        assert_eq!(candidate, snapshot);
        let source_digest = digest(&tree::snapshot(&current).unwrap()).unwrap();
        let target_digest = digest(&tree::snapshot(&work).unwrap()).unwrap();
        let receipt = format!(
            "kind=linos-configuration-merge\nformat=1\ntransaction={}\nsource_journal_generation={}\nsource_config_generation=1\nselected_config_generation=2\nschema=1\nsource_deployment_id={}\ntarget_deployment_id={}\nsource_tree_sha256={source_digest}\ntarget_tree_sha256={target_digest}\n",
            snapshot.transaction(),
            snapshot.generation(),
            id,
            "c".repeat(64),
        );
        let configuration = Configuration::selected(
            2,
            &"c".repeat(64),
            Some(1),
            1,
            &sha256(receipt.as_bytes()).unwrap(),
        )
        .unwrap();
        let prepared = Prepared {
            directory: work,
            generation: 2,
            receipt,
            snapshot: snapshot.clone(),
            configuration: configuration.clone(),
            source_digest,
            target_digest,
        };
        assert_eq!(prepared.publish(&volume, &journal).unwrap(), configuration);
        // An exhausted trial may have published generation 2 without selecting
        // it. Its fallback reserves generation 3, preserving both source 1 and 2.
        let exhausted = snapshot.exhausted().unwrap();
        assert_eq!(allocation(&volume, &exhausted, &id, 1).unwrap(), 3);
        assert_eq!(
            allocation(&volume, &snapshot, &"c".repeat(64), 1).unwrap(),
            2
        );
        let records = volume.join("State/var/lib/linos/configuration");
        let duplicate = records.join(format!("prepared-{}-4.record", snapshot.transaction()));
        std::fs::write(
            &duplicate,
            prepared.receipt.replace(
                "selected_config_generation=2",
                "selected_config_generation=4",
            ),
        )
        .unwrap();
        assert!(allocation(&volume, &snapshot, &"c".repeat(64), 1).is_err());
        std::fs::remove_file(duplicate).unwrap();
        let overflow = records.join(format!("{}.record", u64::MAX));
        std::fs::write(&overflow, b"reserved").unwrap();
        assert!(allocation(&volume, &exhausted, &id, 1).is_err());
        std::fs::remove_file(overflow).unwrap();
        assert_eq!(journal.read().unwrap(), Some(snapshot.clone()));
        assert_eq!(
            std::fs::read(current.join("shadow")).unwrap(),
            b"synthetic current credential"
        );
        assert_eq!(
            std::fs::read(volume.join("State/config/2/shadow")).unwrap(),
            b"synthetic merged credential"
        );
        assert_eq!(prepared.publish(&volume, &journal).unwrap(), configuration);
        let published_record = volume.join("State/var/lib/linos/configuration/2.record");
        std::fs::remove_file(&published_record).unwrap();
        // Power loss after the tree rename but before its final record is
        // recoverable without rerunning account creation or resetting attempts.
        assert_eq!(
            published_configuration(&volume, &snapshot, &"c".repeat(64), 1).unwrap(),
            Some(configuration.clone()),
        );
        assert!(published_record.is_file());
        assert_eq!(journal.read().unwrap(), Some(snapshot.clone()));
        assert_eq!(
            published_configuration(&volume, &snapshot, &"c".repeat(64), 1).unwrap(),
            Some(configuration.clone()),
        );
        let target_file = volume.join("State/config/2/shadow");
        std::fs::write(&target_file, b"tampered prepared credential").unwrap();
        assert!(published_configuration(&volume, &snapshot, &"c".repeat(64), 1).is_err());
        std::fs::write(&target_file, b"synthetic merged credential").unwrap();
        std::fs::write(current.join("shadow"), b"changed source credential").unwrap();
        assert!(published_configuration(&volume, &snapshot, &"c".repeat(64), 1).is_err());
        std::fs::write(current.join("shadow"), b"synthetic current credential").unwrap();
        // Once an intent advances the journal, its verified original merge
        // generation still validates the published tree. Ordinary retry cannot
        // reinterpret that prior receipt as belonging to a new generation.
        assert_eq!(
            published_configuration_at(
                &volume,
                &exhausted,
                &"c".repeat(64),
                1,
                snapshot.generation(),
                2
            )
            .unwrap(),
            Some(configuration.clone()),
        );
        assert!(
            published_configuration_at(
                &volume,
                &exhausted,
                &"c".repeat(64),
                1,
                exhausted.generation(),
                2
            )
            .is_err()
        );
        journal
            .apply(
                &snapshot,
                Event::SelectCandidate {
                    configuration: &configuration,
                },
            )
            .unwrap();
        assert!(prepared.publish(&volume, &journal).is_err());
        drop(journal);
        std::fs::remove_dir_all(volume).unwrap();
    }
}
