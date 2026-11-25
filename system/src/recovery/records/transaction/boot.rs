//! Layout 2 Type 1 entries, kernel selectors and native loader observations.
//! Read only preflight, never a blessing or permission to mount persistent data.

use super::{BootDecision, BootEvidence, Entry, Phase, Transaction};
use crate::recovery::{
    records::{Layout, invalid, state::positive, uuid},
    update::journal::open_directory,
};
use rustix::fs::{self, Mode, OFlags};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read},
    os::{fd::AsRawFd, unix::fs::MetadataExt},
};

const MAX_TEXT: usize = 16 * 1024;
const MAX_ENTRIES: usize = 128;
const EFIVARS: &str = "/sys/firmware/efi/efivars/";
const VENDOR: &str = "4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";
const ENTRIES: &str = "/efi/loader/entries";
const MASK: &str = "systemd.mask=systemd-bless-boot.service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selectors {
    pub volume_partuuid: String,
    pub entry: Entry,
    pub blessing_masked: bool,
}

impl Selectors {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.is_empty()
            || bytes.len() > MAX_TEXT
            || !bytes
                .iter()
                .all(|b| (b' '..=b'~').contains(b) || *b == b'\n')
        {
            return Err(invalid("Kernel selectors require bounded ASCII"));
        }
        let text = std::str::from_utf8(bytes).map_err(invalid)?;
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.contains(['\n', '"', '\'']) {
            return Err(invalid("Ambiguous kernel command line quoting"));
        }
        let mut fields = BTreeMap::new();
        let mut readonly = false;
        let mut masked = false;
        for word in text.split(' ').filter(|word| !word.is_empty()) {
            if word.contains('\\') && !word.starts_with("initrd=") {
                return Err(invalid("Backslash outside the firmware initramfs selector"));
            }
            if word == "ro" {
                if readonly {
                    return Err(invalid("Duplicate root access mode"));
                }
                readonly = true;
            } else if word == "rw" {
                return Err(invalid("Managed root must remain read only"));
            } else if word == MASK {
                if masked {
                    return Err(invalid("Duplicate blessing mask"));
                }
                masked = true;
            } else {
                let (key, value) = word.split_once('=').unwrap_or((word, ""));
                if !(key.starts_with("linos.")
                    || matches!(key, "root" | "rootflags" | "panic" | "initrd"))
                {
                    continue;
                }
                if !matches!(
                    key,
                    "linos.layout"
                        | "linos.volume"
                        | "linos.deployment"
                        | "linos.transaction"
                        | "root"
                        | "rootflags"
                        | "panic"
                        | "initrd"
                ) || value.is_empty()
                    || fields.insert(key, value).is_some()
                {
                    return Err(invalid("Unknown, empty or duplicate managed selector"));
                }
            }
        }
        let get = |key| {
            fields
                .get(key)
                .copied()
                .ok_or_else(|| invalid("Missing managed selector"))
        };
        if fields.get("panic").is_some_and(|value| *value != "10") {
            return Err(invalid("Managed kernel panic delay must be ten seconds"));
        }
        if !readonly
            || get("root")? != "/dev/linos-system"
            || get("rootflags")? != "noload"
            || get("linos.layout")? != "2"
        {
            return Err(invalid("Invalid managed loop root selectors"));
        }
        let entry = Entry::new(
            get("linos.deployment")?,
            positive(get("linos.transaction")?)?,
        )?;
        if let Some(initrd) = fields.get("initrd") {
            let expected = format!("/EFI/Linos/deployments/{}/initramfs", entry.deployment_id());
            if **initrd != expected && initrd.replace('\\', "/") != expected {
                return Err(invalid(
                    "Firmware initramfs selector differs from the deployment",
                ));
            }
        }
        Ok(Self {
            volume_partuuid: uuid(get("linos.volume")?)?,
            entry,
            blessing_masked: masked,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counter {
    Stable,
    Untried,
    First,
    Last,
}

impl Counter {
    fn suffix(self) -> &'static str {
        match self {
            Self::Stable => "",
            Self::Untried => "+2",
            Self::First => "+1-1",
            Self::Last => "+0-2",
        }
    }
}

pub fn filename(entry: &Entry, counter: Counter) -> String {
    format!(
        "linos-{}-{}{}.conf",
        entry.transaction(),
        entry.deployment_id(),
        counter.suffix()
    )
}

pub(super) fn parse_filename(name: &str) -> io::Result<(Entry, Counter)> {
    let body = name
        .strip_prefix("linos-")
        .and_then(|value| value.strip_suffix(".conf"))
        .ok_or_else(|| invalid("Invalid managed entry filename"))?;
    let (body, counter) = if let Some((body, count)) = body.split_once('+') {
        (
            body,
            match count {
                "2" => Counter::Untried,
                "1-1" => Counter::First,
                "0-2" => Counter::Last,
                _ => return Err(invalid("Unknown managed boot counter")),
            },
        )
    } else {
        (body, Counter::Stable)
    };
    let (transaction, id) = body
        .split_once('-')
        .ok_or_else(|| invalid("Missing entry transaction"))?;
    Ok((Entry::new(id, positive(transaction)?)?, counter))
}

/// Canonical entry bytes for publication; the caller syncs verified boot files
/// first and persists Trial intent before exposing a counted entry.
pub fn render(layout: &Layout, entry: &Entry, trial: bool) -> String {
    let asset = format!("/EFI/Linos/deployments/{}/", entry.deployment_id());
    format!(
        "title Linos Server\nsort-key linos\nversion {}\nlinux {asset}kernel\ninitrd {asset}initramfs\noptions root=/dev/linos-system ro rootflags=noload panic=10 linos.layout=2 linos.volume={} linos.deployment={} linos.transaction={}{}\n",
        entry.transaction(),
        layout.linos_partuuid(),
        entry.deployment_id(),
        entry.transaction(),
        if trial {
            " systemd.mask=systemd-bless-boot.service"
        } else {
            ""
        }
    )
}

fn entry_contents(
    bytes: &[u8],
    layout: &Layout,
    expected: &Entry,
    trial: bool,
) -> io::Result<Selectors> {
    if bytes.is_empty()
        || bytes.len() > MAX_TEXT
        || !bytes.ends_with(b"\n")
        || !bytes
            .iter()
            .all(|b| *b == b'\n' || (b' '..=b'~').contains(b))
    {
        return Err(invalid("Invalid boot entry text"));
    }
    let text = std::str::from_utf8(bytes).map_err(invalid)?;
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line
            .split_once(' ')
            .ok_or_else(|| invalid("Malformed boot entry line"))?;
        if !matches!(
            key,
            "title" | "sort-key" | "version" | "linux" | "initrd" | "options"
        ) || value.is_empty()
            || fields.insert(key, value).is_some()
        {
            return Err(invalid("Unknown or duplicate boot entry field"));
        }
    }
    let get = |key| {
        fields
            .get(key)
            .copied()
            .ok_or_else(|| invalid("Missing boot entry field"))
    };
    let asset = format!("/EFI/Linos/deployments/{}/", expected.deployment_id());
    if get("sort-key")? != "linos"
        || get("version")? != expected.transaction().to_string()
        || get("linux")? != format!("{asset}kernel")
        || get("initrd")? != format!("{asset}initramfs")
    {
        return Err(invalid(
            "Boot assets or ordering disagree with the entry identity",
        ));
    }
    get("title")?;
    let selectors = Selectors::parse(get("options")?.as_bytes())?;
    if selectors.entry != *expected
        || selectors.volume_partuuid != layout.linos_partuuid()
        || trial && !selectors.blessing_masked
    {
        return Err(invalid("Boot entry selectors or blessing mask disagree"));
    }
    Ok(selectors)
}

fn efi_text(bytes: &[u8]) -> io::Result<String> {
    if bytes.len() < 8 || bytes.len() > 1024 || !(bytes.len() - 4).is_multiple_of(2) {
        return Err(invalid("Malformed loader EFI variable"));
    }
    let mut words: Vec<_> = bytes[4..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    if words.pop() != Some(0) || words.contains(&0) {
        return Err(invalid("EFI text must have one final terminator"));
    }
    let text = String::from_utf16(&words).map_err(invalid)?;
    if text.is_empty() || !text.bytes().all(|b| (b' '..=b'~').contains(&b)) {
        return Err(invalid("Invalid loader identifier text"));
    }
    Ok(text)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Firmware {
    selected: Vec<u8>,
    counted: Option<Vec<u8>>,
}

fn bounded(mut file: File, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("Native boot input exceeds its bound"));
    }
    Ok(bytes)
}

fn native_file(path: &str, limit: usize) -> io::Result<Vec<u8>> {
    let fd = fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let file = File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(invalid("Native boot input is not a regular file"));
    }
    bounded(file, limit)
}

fn firmware() -> io::Result<Firmware> {
    let selected = native_file(&format!("{EFIVARS}LoaderEntrySelected-{VENDOR}"), 1024)?;
    efi_text(&selected)?;
    let counted = match native_file(&format!("{EFIVARS}LoaderBootCountPath-{VENDOR}"), 1024) {
        Ok(bytes) => {
            efi_text(&bytes)?;
            Some(bytes)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    Ok(Firmware { selected, counted })
}

fn inventory(directory: &File) -> io::Result<BTreeMap<String, Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut total = 0;
    for item in std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        total += 1;
        if total > MAX_ENTRIES {
            return Err(invalid("Boot entry inventory exceeds its bound"));
        }
        let item = item?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| invalid("Non ASCII boot entry filename"))?;
        if !name.starts_with("linos-") {
            continue;
        }
        parse_filename(&name)?;
        let fd = fs::openat(
            directory,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
        {
            return Err(invalid(
                "Managed entry must be a protected standalone regular file",
            ));
        }
        entries.insert(name, bounded(file, MAX_TEXT)?);
    }
    Ok(entries)
}

fn reconcile(
    layout: &Layout,
    journal: &Transaction,
    cmdline: &[u8],
    firmware: &Firmware,
    entries: &BTreeMap<String, Vec<u8>>,
) -> io::Result<Observation> {
    reconcile_impl(layout, journal, cmdline, firmware, entries, None)
}

fn reconcile_impl(
    layout: &Layout,
    journal: &Transaction,
    cmdline: &[u8],
    firmware: &Firmware,
    entries: &BTreeMap<String, Vec<u8>>,
    approval: Option<&super::receipts::HealthIntent>,
) -> io::Result<Observation> {
    let selectors = Selectors::parse(cmdline)?;
    if selectors.volume_partuuid != layout.linos_partuuid() {
        return Err(invalid("Kernel volume selector disagrees with layout"));
    }
    if efi_text(&firmware.selected)? != filename(&selectors.entry, Counter::Stable) {
        return Err(invalid(
            "Loader selection disagrees with running kernel selectors",
        ));
    }
    let mut pairs = BTreeSet::new();
    let mut candidate_counter = None;
    let mut running_counter = None;
    for (name, bytes) in entries {
        let (entry, counter) = parse_filename(name)?;
        let is_candidate = journal.candidate() == Some(&entry);
        let initial = journal.phase() == Phase::InstallPending
            && entry == selectors.entry
            && entry.transaction() == journal.transaction()
            && entry.deployment_id() == journal.selected_deployment_id();
        if !initial
            && !is_candidate
            && journal.healthy() != Some(&entry)
            && journal.previous() != Some(&entry)
        {
            return Err(invalid("Managed entry is not referenced by the journal"));
        }
        if !pairs.insert((entry.transaction(), entry.deployment_id().to_owned())) {
            return Err(invalid(
                "Duplicate stable or counted entries for one boot transaction",
            ));
        }
        if !is_candidate && counter != Counter::Stable {
            return Err(invalid(
                "Healthy or previous entry still has trial counters",
            ));
        }
        if is_candidate {
            if !matches!(journal.phase(), Phase::Trial | Phase::RollbackRequired) {
                return Err(invalid("Unpublished candidate is exposed in the boot menu"));
            }
            if counter == Counter::Stable && approval.is_none() {
                return Err(invalid(
                    "Stable candidate requires durable blessing reconciliation",
                ));
            }
            candidate_counter = Some(counter);
        }
        let trial = counter != Counter::Stable || is_candidate;
        let expected = entry_contents(bytes, layout, &entry, trial)?;
        if entry == selectors.entry {
            if trial && !selectors.blessing_masked {
                return Err(invalid("Running trial permits independent blessing"));
            }
            if expected != selectors {
                return Err(invalid(
                    "Running managed selectors differ from the boot entry",
                ));
            }
            running_counter = Some(counter);
        }
    }
    // Both retained healthy paths stay protected while the candidate runs.
    for entry in [journal.healthy(), journal.previous()]
        .into_iter()
        .flatten()
    {
        if !pairs.contains(&(entry.transaction(), entry.deployment_id().to_owned())) {
            return Err(invalid(
                "Recorded healthy or previous boot entry is missing",
            ));
        }
    }
    let running = running_counter.ok_or_else(|| invalid("Running boot entry is absent"))?;
    let evidence = match running {
        Counter::First | Counter::Last => {
            let path = firmware
                .counted
                .as_ref()
                .ok_or_else(|| invalid("Missing firmware counter path"))?;
            let path = efi_text(path)?.replace('\\', "/");
            if path != format!("/loader/entries/{}", filename(&selectors.entry, running)) {
                return Err(invalid(
                    "Firmware counter path disagrees with exact running entry",
                ));
            }
            if running == Counter::First {
                BootEvidence::Counted { left: 1, done: 1 }
            } else {
                BootEvidence::Counted { left: 0, done: 2 }
            }
        }
        Counter::Stable => {
            let resuming = approval.filter(|receipt| receipt.candidate() == &selectors.entry);
            if let Some(receipt) = resuming {
                if let Some(path) = &firmware.counted
                    && efi_text(path)?.replace('\\', "/") != receipt.observed_entry_path()
                {
                    return Err(invalid(
                        "Blessing reconciliation has a different firmware counter path",
                    ));
                }
            } else if firmware.counted.is_some() {
                return Err(invalid(
                    "Stable boot has a conflicting firmware counter path",
                ));
            }
            if resuming.is_some() {
                BootEvidence::Stable
            } else if matches!(journal.phase(), Phase::Trial | Phase::RollbackRequired) {
                if candidate_counter != Some(Counter::Last) {
                    return Err(invalid("Manual fallback precedes candidate exhaustion"));
                }
                BootEvidence::ExhaustedCandidate
            } else {
                BootEvidence::Stable
            }
        }
        Counter::Untried => {
            return Err(invalid(
                "Firmware did not consume the running trial attempt",
            ));
        }
    };
    let observed_bytes = entries
        .get(&filename(&selectors.entry, running))
        .ok_or_else(|| invalid("Missing observed entry bytes"))?;
    let entry_sha256 = crate::recovery::records::deployment::sha256(observed_bytes)?;
    let approved = running == Counter::Stable
        && approval.is_some_and(|receipt| receipt.candidate() == &selectors.entry);
    if approved && approval.is_none_or(|receipt| receipt.entry_sha256() != entry_sha256) {
        return Err(invalid(
            "Blessed entry bytes differ from approved health intent",
        ));
    }
    let decision = if approved && journal.phase() == Phase::Trial {
        BootDecision::ResumeApproval
    } else {
        journal.check_boot(&selectors.entry, evidence)?
    };
    Ok(Observation {
        previous_entry_sha256: journal
            .previous()
            .map(|entry| {
                let bytes = entries
                    .get(&filename(entry, Counter::Stable))
                    .ok_or_else(|| invalid("Missing previous healthy entry"))?;
                crate::recovery::records::deployment::sha256(bytes)
            })
            .transpose()?,
        entry_sha256,
        entry_path: format!("/loader/entries/{}", filename(&selectors.entry, running)),
        selectors,
        decision,
        journal_generation: journal.generation(),
    })
}

#[derive(Debug)]
pub struct Observation {
    previous_entry_sha256: Option<String>,
    selectors: Selectors,
    decision: BootDecision,
    journal_generation: u64,
    entry_sha256: String,
    entry_path: String,
}

impl Observation {
    pub fn previous_entry_sha256(&self) -> Option<&str> {
        self.previous_entry_sha256.as_deref()
    }
    pub fn selectors(&self) -> &Selectors {
        &self.selectors
    }
    pub fn decision(&self) -> BootDecision {
        self.decision
    }
    pub fn journal_generation(&self) -> u64 {
        self.journal_generation
    }
    pub fn entry_sha256(&self) -> &str {
        &self.entry_sha256
    }
    pub fn entry_path(&self) -> &str {
        &self.entry_path
    }
}

/// Collect fixed native sources twice and refuse changes. Caller separately
/// verifies that /efi is the observed layout ESP, holds the journal volume lock,
/// and validates signed artifacts, receipts and configuration before mounting data.
/// This function neither probes host block devices nor writes EFI variables.
pub fn inspect(layout: &Layout, journal: &Transaction) -> io::Result<Observation> {
    inspect_native(layout, journal, None)
}

pub(super) fn inspect_intent(
    layout: &Layout,
    journal: &Transaction,
    publication: &super::receipts::Publication,
    health: &super::receipts::HealthIntent,
) -> io::Result<Observation> {
    if journal.phase() == Phase::Healthy {
        health.verify_completed(journal, publication)?;
    } else {
        health.verify(journal, publication)?;
    }
    if journal.health_receipt_sha256() != Some(health.digest()?.as_str()) {
        return Err(invalid("Approval observation has no committed intent"));
    }
    let observed = inspect_native(layout, journal, Some(health))?;
    if journal.phase() == Phase::Trial
        && observed.previous_entry_sha256() != health.previous_entry_sha256()
    {
        return Err(invalid("Previous entry changed after approval intent"));
    }
    Ok(observed)
}

/// Resume only the precise health intent already committed in the locked
/// journal, after verifying its publication against an authenticated manifest.
pub fn inspect_approval(
    layout: &Layout,
    journal: &Transaction,
    manifest: &crate::recovery::records::deployment::OfficialManifest,
    publication: &super::receipts::Publication,
    health: &super::receipts::HealthIntent,
) -> io::Result<Observation> {
    publication.verify(journal, layout, manifest)?;
    health.verify(journal, publication)?;
    if journal.health_receipt_sha256() != Some(health.digest()?.as_str()) {
        return Err(invalid(
            "Health intent has not been committed before blessing",
        ));
    }
    verify_efi_artifacts(manifest)?;
    let observation = inspect_native(layout, journal, Some(health))?;
    if observation.previous_entry_sha256() != health.previous_entry_sha256() {
        return Err(invalid(
            "Previous entry differs from approved retirement intent",
        ));
    }
    verify_efi_artifacts(manifest)?;
    Ok(observation)
}

pub(crate) fn verify_efi_artifacts(
    manifest: &crate::recovery::records::deployment::OfficialManifest,
) -> io::Result<()> {
    let directory = open_directory(std::path::Path::new(&format!(
        "/efi/EFI/Linos/deployments/{}",
        manifest.id()
    )))?;
    for (name, expected) in ["kernel", "initramfs"]
        .into_iter()
        .zip(&manifest.deployment().artifacts[1..3])
    {
        let file = File::from(fs::openat(
            &directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
            || metadata.len() != expected.bytes
            || crate::recovery::records::deployment::sha256_file(&file)? != expected.sha256
        {
            return Err(invalid(
                "EFI boot file bytes disagree with authenticated deployment",
            ));
        }
    }
    Ok(())
}

pub fn inspect_fallback(
    layout: &Layout,
    journal: &Transaction,
    failed: &crate::recovery::records::deployment::OfficialManifest,
    target: &crate::recovery::records::deployment::OfficialManifest,
    publication: &super::receipts::Publication,
    receipt: &super::receipts::FallbackReceipt,
    configuration: &crate::recovery::records::state::Configuration,
) -> io::Result<Observation> {
    publication.verify(journal, layout, failed)?;
    receipt.verify_committed(journal, publication, configuration, target)?;
    verify_efi_artifacts(target)?;
    let observed = inspect(layout, journal)?;
    if observed.decision() != BootDecision::MergeFallback
        || &observed.selectors().entry != receipt.healthy()
        || observed.entry_sha256() != receipt.entry_sha256()
    {
        return Err(invalid(
            "Fallback boot differs from durable reconciliation intent",
        ));
    }
    verify_efi_artifacts(target)?;
    Ok(observed)
}

pub(super) fn sync_entries() -> io::Result<()> {
    open_directory(std::path::Path::new(ENTRIES))?.sync_all()
}

struct Retirement {
    name: String,
    bytes: Option<Vec<u8>>,
    remaining: BTreeMap<String, Vec<u8>>,
}

fn retirement_inventory(
    failed: &Entry,
    expected_digest: &str,
    mut entries: BTreeMap<String, Vec<u8>>,
) -> io::Result<Retirement> {
    let name = filename(failed, Counter::Last);
    let bytes = entries.remove(&name);
    if let Some(bytes) = &bytes
        && crate::recovery::records::deployment::sha256(bytes)? != expected_digest
    {
        return Err(invalid("Exhausted entry changed before retirement"));
    }
    Ok(Retirement {
        name,
        bytes,
        remaining: entries,
    })
}

/// Retire only the failed exhausted entry named by a completed fallback receipt.
/// A retry after the journal commit accepts its absence, never recreates trials.
pub(super) fn retire_failed(
    layout: &Layout,
    journal: &Transaction,
    failed: &crate::recovery::records::deployment::OfficialManifest,
    target: &crate::recovery::records::deployment::OfficialManifest,
    publication: &super::receipts::Publication,
    receipt: &super::receipts::FallbackReceipt,
    configuration: &crate::recovery::records::state::Configuration,
) -> io::Result<()> {
    receipt.verify_completed(journal, publication, configuration, target)?;
    publication.verify_software(layout, failed)?;
    verify_efi_artifacts(target)?;
    let directory = open_directory(std::path::Path::new(ENTRIES))?;
    let retirement = retirement_inventory(
        receipt.failed(),
        publication.entry_sha256(),
        inventory(&directory)?,
    )?;
    let cmdline = native_file("/proc/cmdline", MAX_TEXT)?;
    let before = firmware()?;
    let observation = reconcile(layout, journal, &cmdline, &before, &retirement.remaining)?;
    if observation.decision() != BootDecision::Healthy
        || observation.entry_sha256() != receipt.entry_sha256()
    {
        return Err(invalid(
            "Completed fallback no longer boots its approved healthy entry",
        ));
    }
    if retirement.bytes.is_some() {
        let current = inventory(&directory)?;
        if current.get(&retirement.name) != retirement.bytes.as_ref() || firmware()? != before {
            return Err(invalid("EFI changed during exhausted entry retirement"));
        }
        fs::unlinkat(&directory, retirement.name.as_str(), fs::AtFlags::empty())?;
    }
    directory.sync_all()?;
    inspect(layout, journal)?;
    Ok(())
}

pub(super) fn sync_approved_entry(
    layout: &Layout,
    journal: &Transaction,
    manifest: &crate::recovery::records::deployment::OfficialManifest,
    publication: &super::receipts::Publication,
    health: &super::receipts::HealthIntent,
) -> io::Result<()> {
    let directory = open_directory(std::path::Path::new(ENTRIES))?;
    directory.sync_all()?;
    if inspect_approval(layout, journal, manifest, publication, health)?.decision()
        != BootDecision::ResumeApproval
    {
        return Err(invalid("Stable candidate changed during EFI sync"));
    }
    Ok(())
}

/// Approval retains the old healthy pair as the new fallback. Only the older
/// previous entry captured by the committed approval receipt may be retired.
struct ApprovalRetirement {
    remaining: BTreeMap<String, Vec<u8>>,
    name: Option<String>,
}

fn approval_retirement_inventory(
    journal: &Transaction,
    health: &super::receipts::HealthIntent,
    mut remaining: BTreeMap<String, Vec<u8>>,
) -> io::Result<ApprovalRetirement> {
    let mut retired = None;
    if let Some(previous) = health.previous() {
        if journal.healthy() == Some(previous) || journal.previous() == Some(previous) {
            return Err(invalid(
                "Approval retirement still references an active boot entry",
            ));
        }
        let name = filename(previous, Counter::Stable);
        if let Some(bytes) = remaining.remove(&name) {
            if Some(crate::recovery::records::deployment::sha256(&bytes)?.as_str())
                != health.previous_entry_sha256()
            {
                return Err(invalid("Previous entry changed before approval retirement"));
            }
            retired = Some(name);
        }
    }
    Ok(ApprovalRetirement {
        remaining,
        name: retired,
    })
}

pub(super) fn retire_previous(
    layout: &Layout,
    journal: &Transaction,
    manifest: &crate::recovery::records::deployment::OfficialManifest,
    publication: &super::receipts::Publication,
    health: &super::receipts::HealthIntent,
) -> io::Result<()> {
    health.verify_completed(journal, publication)?;
    publication.verify_software(layout, manifest)?;
    verify_efi_artifacts(manifest)?;
    let directory = open_directory(std::path::Path::new(ENTRIES))?;
    let original = inventory(&directory)?;
    let retirement = approval_retirement_inventory(journal, health, original.clone())?;
    let cmdline = native_file("/proc/cmdline", MAX_TEXT)?;
    let before = firmware()?;
    let observed = reconcile_impl(
        layout,
        journal,
        &cmdline,
        &before,
        &retirement.remaining,
        Some(health),
    )?;
    if observed.decision() != BootDecision::Healthy
        || observed.entry_sha256() != health.entry_sha256()
    {
        return Err(invalid("Completed approval boots a different entry"));
    }
    if let Some(name) = retirement.name {
        if inventory(&directory)? != original
            || firmware()? != before
            || native_file("/proc/cmdline", MAX_TEXT)? != cmdline
        {
            return Err(invalid("EFI changed before approval retirement"));
        }
        fs::unlinkat(&directory, name.as_str(), fs::AtFlags::empty())?;
    }
    directory.sync_all()?;
    // A blessing may leave LoaderBootCountPath present until the next reboot.
    // Its exact match was already established by approval inspection.
    inspect_intent(layout, journal, publication, health)?;
    Ok(())
}

fn inspect_native(
    layout: &Layout,
    journal: &Transaction,
    approval: Option<&super::receipts::HealthIntent>,
) -> io::Result<Observation> {
    layout.require_plain()?;
    let cmdline = native_file("/proc/cmdline", MAX_TEXT)?;
    let firmware_before = firmware()?;
    let directory = open_directory(std::path::Path::new(ENTRIES))?;
    let entries = inventory(&directory)?;
    let observation = match approval {
        None => reconcile(layout, journal, &cmdline, &firmware_before, &entries)?,
        Some(receipt) => reconcile_impl(
            layout,
            journal,
            &cmdline,
            &firmware_before,
            &entries,
            Some(receipt),
        )?,
    };
    let reopened = open_directory(std::path::Path::new(ENTRIES))?;
    let before = directory.metadata()?;
    let after = reopened.metadata()?;
    if native_file("/proc/cmdline", MAX_TEXT)? != cmdline
        || firmware()? != firmware_before
        || inventory(&directory)? != entries
        || before.dev() != after.dev()
        || before.ino() != after.ino()
    {
        return Err(invalid("Native boot evidence changed during inspection"));
    }
    Ok(observation)
}

#[cfg(test)]
mod tests;
