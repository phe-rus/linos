//! Development backend for ordinary image files, never physical devices.
use super::{
    Event, MAX_TRIAL_BOOTS, Phase, Slot,
    artifact::{
        self, Payload, bounded, digest, fields, invalid, mkdir, sync_directory, uuid, write_new,
    },
    journal::{Journal, Snapshot, open_directory},
};
use rustix::fs::{RenameFlags, renameat_with};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn slot_name(slot: Slot) -> &'static str {
    match slot {
        Slot::A => "a",
        Slot::B => "b",
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new(root: &Path, name: &str) -> io::Result<Self> {
        let path = root.join(name);
        if path.try_exists()? {
            open_directory(&path)?;
            fs::remove_dir_all(&path)?;
        }
        mkdir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct ImageStore {
    root: PathBuf,
    journal: Journal,
    roots: [String; 2],
}

impl ImageStore {
    /// Initialize only an empty, privately owned image workspace. UUIDs come
    /// from the disposable GPT builder, not from a running host disk.
    pub fn initialize(
        root: &Path,
        payload: &Path,
        key: &Path,
        root_a: &str,
        root_b: &str,
    ) -> io::Result<Self> {
        open_directory(root)?;
        let journal = Journal::open(root)?;
        if fs::read_dir(root)?.next().is_some()
            || !uuid(root_a)
            || !uuid(root_b)
            || root_a.eq_ignore_ascii_case(root_b)
        {
            return Err(invalid(
                "Initialization requires an empty image directory and two distinct partition UUIDs",
            ));
        }
        let store = Self {
            root: root.to_owned(),
            journal,
            roots: [root_a.to_ascii_lowercase(), root_b.to_ascii_lowercase()],
        };
        for path in [
            "slots",
            "efi",
            "efi/linos",
            "efi/loader",
            "efi/loader/entries",
        ] {
            mkdir(&store.root.join(path))?;
        }
        let scratch = Scratch::new(root, ".verify-initial")?;
        let payload = Payload::authenticate(payload, key, &scratch.0)?;
        let slot = store.root.join("slots/a");
        mkdir(&slot)?;
        payload.copy_verified(&slot)?;
        write_new(&slot.join("transaction"), b"0\n")?;
        sync_directory(&slot)?;
        sync_directory(&store.root.join("slots"))?;
        store.publish_assets(Slot::A, payload.release, 0)?;
        let name = format!("linos-{}-a.conf", payload.release);
        let entry = store.entry(Slot::A, payload.release, 0);
        write_new(&store.entries().join(name), entry.as_bytes())?;
        sync_directory(&store.entries())?;
        write_new(
            &root.join("efi/loader/loader.conf"),
            b"default linos-*.conf\ntimeout 1\neditor no\nauto-entries no\nauto-firmware no\n",
        )?;
        sync_directory(&root.join("efi/loader"))?;
        sync_directory(&root.join("efi/linos"))?;
        sync_directory(&root.join("efi"))?;
        write_new(
            &root.join("image-layout"),
            format!(
                "format=1\nroot_a={}\nroot_b={}\n",
                store.roots[0], store.roots[1]
            )
            .as_bytes(),
        )?;
        sync_directory(root)?;
        store.journal.initialize(Slot::A)?;
        Ok(store)
    }

    pub fn open(root: &Path) -> io::Result<Self> {
        let journal = Journal::open(root)?;
        let bytes = bounded(&root.join("image-layout"), 256)?;
        let values = fields(&bytes)?;
        let a = *values
            .get("root_a")
            .ok_or_else(|| invalid("Missing System A UUID"))?;
        let b = *values
            .get("root_b")
            .ok_or_else(|| invalid("Missing System B UUID"))?;
        if values.len() != 3
            || values.get("format") != Some(&"1")
            || !uuid(a)
            || !uuid(b)
            || a.eq_ignore_ascii_case(b)
        {
            return Err(invalid("Invalid image layout"));
        }
        for path in [
            "slots",
            "efi",
            "efi/linos",
            "efi/loader",
            "efi/loader/entries",
        ] {
            open_directory(&root.join(path))?;
        }
        // An altered default can bypass counted fallback. Do not overwrite it.
        if bounded(&root.join("efi/loader/loader.conf"), 256)?
            != b"default linos-*.conf\ntimeout 1\neditor no\nauto-entries no\nauto-firmware no\n"
        {
            return Err(invalid("Image boot selection policy changed"));
        }
        Ok(Self {
            root: root.to_owned(),
            journal,
            roots: [a.to_owned(), b.to_owned()],
        })
    }

    pub fn snapshot(&self) -> io::Result<Snapshot> {
        self.journal
            .read()?
            .ok_or_else(|| invalid("Image journal is missing"))
    }

    /// Export a single staged transaction to a fresh persistent guest state
    /// directory. The disk builder places it inside the Data filesystem.
    pub fn export_guest(
        &self,
        destination: &Path,
        key: &Path,
        data: &str,
        esp: &str,
    ) -> io::Result<()> {
        use super::health::{Deployment, Layout};
        let snapshot = self.snapshot()?;
        let Phase::Trial {
            active, candidate, ..
        } = snapshot.phase
        else {
            return Err(invalid("Only an activated image trial can be exported"));
        };
        let layout = Layout::new(&self.roots[0], &self.roots[1], data, esp)?;
        let transaction = snapshot
            .generation
            .checked_sub(1)
            .ok_or_else(|| invalid("Invalid trial generation"))?;
        if bounded(&self.slot(candidate).join("activated"), 32)?
            != format!("{transaction}\n").as_bytes()
        {
            return Err(invalid("Candidate publication has not completed"));
        }
        let scratch = Scratch::new(&self.root, ".verify-export")?;
        let payload = Payload::authenticate(&self.slot(candidate), key, &scratch.0)?;
        let active_scratch = Scratch::new(&self.root, ".verify-active")?;
        let current = Payload::authenticate(&self.slot(active), key, &active_scratch.0)?;
        let active_transaction =
            std::str::from_utf8(&bounded(&self.slot(active).join("transaction"), 32)?)
                .map_err(invalid)?
                .trim()
                .parse()
                .map_err(invalid)?;
        let deployment = Deployment {
            active,
            active_release: current.release,
            active_transaction,
            candidate,
            release: payload.release,
            transaction,
            trial_generation: snapshot.generation,
        };
        let _lock = Journal::open(destination)?;
        if fs::read_dir(destination)?.next().is_some() {
            return Err(invalid("Guest state export requires an empty directory"));
        }
        write_new(&destination.join("layout"), layout.encode().as_bytes())?;
        write_new(
            &destination.join("deployment"),
            deployment.encode().as_bytes(),
        )?;
        write_new(
            &destination.join("update-state"),
            &bounded(&self.root.join("update-state"), 256)?,
        )?;
        sync_directory(destination)
    }

    /// Copies and authenticates the entire candidate before publishing Staged.
    /// Failure may leave an incomplete inactive slot, never an active boot entry.
    pub fn stage(&self, expected: Snapshot, payload: &Path, key: &Path) -> io::Result<Snapshot> {
        if self.snapshot()? != expected {
            return Err(invalid("Stale update request"));
        }
        let Phase::Healthy { active } = expected.phase else {
            return Err(invalid("Only a healthy image can stage an update"));
        };
        let candidate = active.other();
        let next_generation = expected
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("Generation exhausted"))?;
        let scratch = Scratch::new(&self.root, ".verify-candidate")?;
        let payload = Payload::authenticate(payload, key, &scratch.0)?;
        let active_scratch = Scratch::new(&self.root, ".verify-active")?;
        let current = Payload::authenticate(&self.slot(active), key, &active_scratch.0)?;
        if payload.release <= current.release {
            return Err(invalid(
                "Update release must be newer than the healthy release",
            ));
        }
        self.require_healthy_entry(active, current.release)?;
        let healthy_name = format!("linos-{}-{}.conf", current.release, slot_name(active));
        for entry in fs::read_dir(self.entries())? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if name.starts_with("linos-") && name.ends_with(".conf") && name != healthy_name {
                return Err(invalid("Unexpected boot entry in a healthy image"));
            }
        }
        let slots = self.root.join("slots");
        let pending_name = format!(".pending-{}", slot_name(candidate));
        let pending = Scratch::new(&slots, &pending_name)?;
        payload.copy_verified(&pending.0)?;
        write_new(
            &pending.0.join("transaction"),
            format!("{next_generation}\n").as_bytes(),
        )?;
        sync_directory(&pending.0)?;
        let destination = self.slot(candidate);
        if destination.try_exists()? {
            open_directory(&destination)?;
            let directory = open_directory(&slots)?;
            renameat_with(
                &directory,
                pending_name.as_str(),
                &directory,
                slot_name(candidate),
                RenameFlags::EXCHANGE,
            )?;
        } else {
            fs::rename(&pending.0, &destination)?;
        }
        sync_directory(&slots)?;
        self.publish_assets(candidate, payload.release, next_generation)?;
        self.journal.apply(
            expected,
            Event::Stage {
                candidate,
                verified: true,
            },
        )
    }

    /// Retry-safe publication. Trial is durable before a counted entry appears.
    /// A repeated call cannot reset firmware-maintained counters.
    pub fn activate(&self, expected: Snapshot, key: &Path) -> io::Result<Snapshot> {
        if self.snapshot()? != expected {
            return Err(invalid("Stale activation request"));
        }
        let (active, candidate, transaction) = match expected.phase {
            Phase::Staged { active, candidate } => (active, candidate, expected.generation),
            Phase::Trial {
                active, candidate, ..
            } => (
                active,
                candidate,
                expected
                    .generation
                    .checked_sub(1)
                    .ok_or_else(|| invalid("Invalid trial generation"))?,
            ),
            _ => return Err(invalid("Image has no staged or trial candidate")),
        };
        let scratch = Scratch::new(&self.root, ".verify-activation")?;
        let payload = Payload::authenticate(&self.slot(candidate), key, &scratch.0)?;
        if bounded(&self.slot(candidate).join("transaction"), 32)?
            != format!("{transaction}\n").as_bytes()
        {
            return Err(invalid(
                "Candidate is not bound to this journal transaction",
            ));
        }
        let active_scratch = Scratch::new(&self.root, ".verify-active")?;
        let current = Payload::authenticate(&self.slot(active), key, &active_scratch.0)?;
        if payload.release <= current.release {
            return Err(invalid(
                "Candidate release no longer advances the active release",
            ));
        }
        self.require_healthy_entry(active, current.release)?;
        payload.verify_content()?;
        self.publish_assets(candidate, payload.release, transaction)?;
        let base = format!("linos-{}-{}", payload.release, slot_name(candidate));
        let expected_entry = self.entry(candidate, payload.release, transaction);
        let mut found = false;
        for entry in fs::read_dir(self.entries())? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("Non UTF-8 boot entry"))?;
            if name.starts_with(&format!("{base}+")) || name == format!("{base}.conf") {
                if found
                    || !trial_name(&name, &base)
                    || bounded(&entry.path(), 2048)? != expected_entry.as_bytes()
                {
                    return Err(invalid("Unexpected or duplicate candidate boot entry"));
                }
                found = true;
            } else if name.starts_with("linos-")
                && name.ends_with(".conf")
                && name != format!("linos-{}-{}.conf", current.release, slot_name(active))
            {
                return Err(invalid(
                    "Unexpected boot entry could bypass candidate selection",
                ));
            }
        }
        if found {
            if !matches!(expected.phase, Phase::Trial { .. }) {
                return Err(invalid("Boot entry exists before journal trial approval"));
            }
            sync_directory(&self.entries())?;
            self.record_publication(candidate, transaction)?;
            return Ok(expected);
        }
        if self.slot(candidate).join("activated").try_exists()? {
            return Err(invalid(
                "Published candidate entry is missing; trial counts cannot be reset",
            ));
        }
        let trial = if matches!(expected.phase, Phase::Staged { .. }) {
            self.journal.apply(expected, Event::BeginTrial)?
        } else {
            expected
        };
        let pending = self.entries().join(".linos-entry.pending");
        if pending.try_exists()? {
            artifact::regular(&pending)?;
            fs::remove_file(&pending)?;
        }
        write_new(&pending, expected_entry.as_bytes())?;
        fs::rename(
            &pending,
            self.entries()
                .join(format!("{base}+{MAX_TRIAL_BOOTS}.conf")),
        )?;
        sync_directory(&self.entries())?;
        self.record_publication(candidate, transaction)?;
        Ok(trial)
    }

    fn record_publication(&self, slot: Slot, transaction: u64) -> io::Result<()> {
        let path = self.slot(slot).join("activated");
        let expected = format!("{transaction}\n");
        if path.try_exists()? {
            if bounded(&path, 32)? != expected.as_bytes() {
                return Err(invalid(
                    "Boot publication receipt has the wrong transaction",
                ));
            }
            // A previous directory flush may have failed after the receipt write.
            artifact::regular(&path)?.sync_all()?;
        } else {
            write_new(&path, expected.as_bytes())?;
        }
        sync_directory(&self.slot(slot))
    }

    fn slot(&self, slot: Slot) -> PathBuf {
        self.root.join("slots").join(slot_name(slot))
    }
    fn entries(&self) -> PathBuf {
        self.root.join("efi/loader/entries")
    }

    fn require_healthy_entry(&self, slot: Slot, release: u64) -> io::Result<()> {
        let bytes = bounded(
            &self
                .entries()
                .join(format!("linos-{release}-{}.conf", slot_name(slot))),
            2048,
        )?;
        let transaction: u64 =
            std::str::from_utf8(&bounded(&self.slot(slot).join("transaction"), 32)?)
                .map_err(invalid)?
                .trim()
                .parse()
                .map_err(invalid)?;
        if bytes != self.entry(slot, release, transaction).as_bytes() {
            return Err(invalid("Healthy fallback entry changed"));
        }
        for name in ["kernel", "initramfs"] {
            if digest(&self.slot(slot).join(name))?
                != digest(&self.root.join(format!(
                    "efi/linos/{release}-{transaction}-{}/{name}",
                    slot_name(slot)
                )))?
            {
                return Err(invalid("Healthy fallback boot assets changed"));
            }
        }
        Ok(())
    }

    fn publish_assets(&self, slot: Slot, release: u64, transaction: u64) -> io::Result<()> {
        let path = self.root.join(format!(
            "efi/linos/{release}-{transaction}-{}",
            slot_name(slot)
        ));
        mkdir(&path)?;
        for name in ["kernel", "initramfs"] {
            let source = self.slot(slot).join(name);
            let destination = path.join(name);
            if destination.try_exists()? {
                if digest(&destination)? != digest(&source)? {
                    return Err(invalid("Immutable boot artifact differs from candidate"));
                }
            } else {
                let mut source = artifact::regular(&source)?;
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)?;
                io::copy(&mut source, &mut output)?;
                output.sync_all()?;
                if digest(&destination)? != digest(&self.slot(slot).join(name))? {
                    return Err(invalid("Boot artifact copy mismatch"));
                }
            }
        }
        sync_directory(&path)?;
        sync_directory(&self.root.join("efi/linos"))
    }

    fn entry(&self, slot: Slot, release: u64, transaction: u64) -> String {
        let name = slot_name(slot);
        let root = &self.roots[usize::from(slot == Slot::B)];
        format!(
            "title Linos 13 Server image trial\nversion {release}\nsort-key linos\nlinux /linos/{release}-{transaction}-{name}/kernel\ninitrd /linos/{release}-{transaction}-{name}/initramfs\noptions root=PARTUUID={root} rootwait rw console=ttyS0,115200n8 linos.slot={name} linos.release={release} linos.transaction={transaction} systemd.mask=systemd-bless-boot.service systemd.mask=linos-enroll.service systemd.unit=multi-user.target\n"
        )
    }
}

fn trial_name(name: &str, base: &str) -> bool {
    let Some(counts) = name
        .strip_prefix(base)
        .and_then(|s| s.strip_prefix('+'))
        .and_then(|s| s.strip_suffix(".conf"))
    else {
        return false;
    };
    let (left, done) = counts.split_once('-').unwrap_or((counts, "0"));
    match (left.parse::<u8>(), done.parse::<u8>()) {
        (Ok(left), Ok(done)) => {
            left <= MAX_TRIAL_BOOTS && done <= MAX_TRIAL_BOOTS && left + done == MAX_TRIAL_BOOTS
        }
        _ => false,
    }
}
