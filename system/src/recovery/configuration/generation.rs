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
    pub journal: &'a Transaction,
    pub source: &'a Loaded,
    pub target: &'a Loaded,
    pub base_vendor: &'a Path,
    pub current: &'a Path,
    pub target_vendor: &'a Path,
    pub workspace: &'a Path,
}

pub struct Prepared {
    directory: PathBuf,
    generation: u64,
    receipt: String,
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

impl Prepared {
    pub fn build(input: Inputs<'_>) -> io::Result<Self> {
        let journal = input.journal;
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
        let generation = journal
            .selected_config_generation()
            .checked_add(1)
            .ok_or_else(|| invalid("Configuration generation exhausted"))?;
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
            "kind=linos-configuration-merge\nformat=1\ntransaction={}\nsource_journal_generation={}\nsource_config_generation={}\nselected_config_generation={}\nsource_deployment_id={}\ntarget_deployment_id={}\nsource_tree_sha256={}\ntarget_tree_sha256={}\n",
            journal.transaction(),
            journal.generation(),
            journal.selected_config_generation(),
            generation,
            input.source.manifest().id(),
            input.target.manifest().id(),
            digest(&current_tree)?,
            digest(&prepared)?
        );
        Ok(Self {
            directory: destination,
            generation,
            receipt,
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
}
