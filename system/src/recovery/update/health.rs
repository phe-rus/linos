//! Development guest boot health. Physical installation and account migration
//! are not enabled by this protocol.
use super::{
    Event, MAX_TRIAL_BOOTS, Phase, Slot,
    artifact::{bounded, fields, invalid, regular, sync_directory, uuid, write_new},
    journal::{Journal, Snapshot},
};
use std::{
    fs, io,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    process::Command,
};

pub const STATE: &str = "/var/lib/linos/updates";
const ENTRIES: &str = "/efi/loader/entries";
const EFI_COUNT: &str =
    "/sys/firmware/efi/efivars/LoaderBootCountPath-4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";
fn name(slot: Slot) -> &'static str {
    match slot {
        Slot::A => "a",
        Slot::B => "b",
    }
}
fn slot(value: &str) -> io::Result<Slot> {
    match value {
        "a" => Ok(Slot::A),
        "b" => Ok(Slot::B),
        _ => Err(invalid("Invalid deployment slot")),
    }
}
fn number(value: &str) -> io::Result<u64> {
    value.parse().map_err(invalid)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub roots: [String; 2],
    pub data: String,
    pub esp: String,
}
impl Layout {
    pub fn new(a: &str, b: &str, data: &str, esp: &str) -> io::Result<Self> {
        let ids = [a, b, data, esp];
        if ids.iter().any(|id| !uuid(id))
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| ids[..i].iter().any(|other| id.eq_ignore_ascii_case(other)))
        {
            return Err(invalid("Guest partition UUIDs must be valid and distinct"));
        }
        Ok(Self {
            roots: [a.to_ascii_lowercase(), b.to_ascii_lowercase()],
            data: data.to_ascii_lowercase(),
            esp: esp.to_ascii_lowercase(),
        })
    }
    pub fn encode(&self) -> String {
        format!(
            "format=1\npolicy=development-boot\nroot_a={}\nroot_b={}\ndata={}\nesp={}\n",
            self.roots[0], self.roots[1], self.data, self.esp
        )
    }
    fn decode(bytes: &[u8]) -> io::Result<Self> {
        let f = fields(bytes)?;
        if f.len() != 6
            || f.get("format") != Some(&"1")
            || f.get("policy") != Some(&"development-boot")
        {
            return Err(invalid("Unsupported guest health policy"));
        }
        let get = |key| {
            f.get(key)
                .copied()
                .ok_or_else(|| invalid("Missing guest layout field"))
        };
        Self::new(get("root_a")?, get("root_b")?, get("data")?, get("esp")?)
    }
    fn root(&self, slot: Slot) -> &str {
        &self.roots[usize::from(slot == Slot::B)]
    }
}

#[derive(Debug, Clone)]
pub(super) struct Deployment {
    pub active: Slot,
    pub active_release: u64,
    pub active_transaction: u64,
    pub candidate: Slot,
    pub release: u64,
    pub transaction: u64,
    pub trial_generation: u64,
}
impl Deployment {
    pub fn encode(&self) -> String {
        format!(
            "format=1\nactive={}\nactive_release={}\nactive_transaction={}\ncandidate={}\nrelease={}\ntransaction={}\ntrial_generation={}\n",
            name(self.active),
            self.active_release,
            self.active_transaction,
            name(self.candidate),
            self.release,
            self.transaction,
            self.trial_generation
        )
    }
    fn decode(bytes: &[u8]) -> io::Result<Self> {
        let f = fields(bytes)?;
        let get = |key| {
            f.get(key)
                .copied()
                .ok_or_else(|| invalid("Missing deployment field"))
        };
        if f.len() != 8 || get("format")? != "1" {
            return Err(invalid("Unsupported deployment record"));
        }
        let d = Self {
            active: slot(get("active")?)?,
            active_release: number(get("active_release")?)?,
            active_transaction: number(get("active_transaction")?)?,
            candidate: slot(get("candidate")?)?,
            release: number(get("release")?)?,
            transaction: number(get("transaction")?)?,
            trial_generation: number(get("trial_generation")?)?,
        };
        if d.active == d.candidate
            || d.active_release == 0
            || d.release <= d.active_release
            || d.active_transaction >= d.transaction
            || d.trial_generation > u64::MAX - 3
            || d.transaction.checked_add(1) != Some(d.trial_generation)
        {
            return Err(invalid("Inconsistent deployment record"));
        }
        Ok(d)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Boot {
    slot: Slot,
    release: u64,
    transaction: u64,
    root: String,
}
impl Boot {
    fn parse(cmdline: &str) -> io::Result<Self> {
        let mut f = std::collections::BTreeMap::new();
        for item in cmdline.split_whitespace() {
            if let Some((key, value)) = item.split_once('=')
                && matches!(
                    key,
                    "root" | "linos.slot" | "linos.release" | "linos.transaction"
                )
                && f.insert(key, value).is_some()
            {
                return Err(invalid("Duplicate boot identity parameter"));
            }
        }
        let get = |key| {
            f.get(key)
                .copied()
                .ok_or_else(|| invalid("Missing boot identity parameter"))
        };
        let root = get("root")?
            .strip_prefix("PARTUUID=")
            .ok_or_else(|| invalid("Managed boot requires a partition UUID"))?;
        if !uuid(root) {
            return Err(invalid("Invalid boot partition UUID"));
        }
        Ok(Self {
            slot: slot(get("linos.slot")?)?,
            release: number(get("linos.release")?)?,
            transaction: number(get("linos.transaction")?)?,
            root: root.to_ascii_lowercase(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mount {
    device: String,
    filesystem: String,
}
fn mount(mountinfo: &str, point: &str, filesystem: &str) -> io::Result<Mount> {
    let mut found = None;
    for line in mountinfo.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.get(4) != Some(&point) {
            continue;
        }
        let sep = f
            .iter()
            .position(|s| *s == "-")
            .ok_or_else(|| invalid("Malformed mount record"))?;
        if found.is_some()
            || f.len() < 10
            || f.len() <= sep + 3
            || f.get(3) != Some(&"/")
            || f[sep + 1] != filesystem
            || !f[5].split(',').any(|v| v == "rw")
            || !f[sep + 3].split(',').any(|v| v == "rw")
        {
            return Err(invalid(
                "Guest requires distinct writable whole-filesystem mounts",
            ));
        }
        found = Some(Mount {
            device: f[2].to_owned(),
            filesystem: filesystem.to_owned(),
        });
    }
    found.ok_or_else(|| invalid("Required guest mount is missing"))
}
fn validate_mounts(layout: &Layout, boot: &Boot, mountinfo: &str) -> io::Result<()> {
    let points = [
        ("/", layout.root(boot.slot), "ext4"),
        ("/var/lib/linos", layout.data.as_str(), "ext4"),
        ("/efi", layout.esp.as_str(), "vfat"),
    ];
    let mut parent = None;
    let mut devices = std::collections::BTreeSet::new();
    for (point, uuid, filesystem) in points {
        let mounted = mount(mountinfo, point, filesystem)?;
        let metadata = fs::metadata(format!("/dev/disk/by-partuuid/{uuid}"))?;
        if !metadata.file_type().is_block_device() {
            return Err(invalid("Partition UUID does not identify a block device"));
        }
        let actual = format!(
            "{}:{}",
            rustix::fs::major(metadata.rdev()),
            rustix::fs::minor(metadata.rdev())
        );
        let location = fs::canonicalize(format!("/sys/dev/block/{actual}"))?;
        if !location.join("partition").is_file()
            || actual != mounted.device
            || !devices.insert(actual.clone())
            || fs::metadata(point)?.dev() != metadata.rdev()
        {
            return Err(invalid(
                "Mounted filesystem does not match the managed partition",
            ));
        }
        let this_parent = location
            .parent()
            .ok_or_else(|| invalid("Missing partition parent"))?
            .to_owned();
        if parent
            .as_ref()
            .is_some_and(|previous| previous != &this_parent)
        {
            return Err(invalid("Guest partitions belong to different disks"));
        }
        parent = Some(this_parent);
    }
    Ok(())
}

fn expected_boot(layout: &Layout, d: &Deployment, candidate: bool) -> Boot {
    let (slot, release, transaction) = if candidate {
        (d.candidate, d.release, d.transaction)
    } else {
        (d.active, d.active_release, d.active_transaction)
    };
    Boot {
        slot,
        release,
        transaction,
        root: layout.root(slot).to_owned(),
    }
}
fn entry(path: &Path, expected: &Boot) -> io::Result<()> {
    let bytes = bounded(path, 2048)?;
    let text = std::str::from_utf8(&bytes).map_err(invalid)?;
    let options: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("options "))
        .collect();
    let asset = format!(
        "/linos/{}-{}-{}/",
        expected.release,
        expected.transaction,
        name(expected.slot)
    );
    if options.len() != 1
        || Boot::parse(options[0])? != *expected
        || !text
            .lines()
            .any(|line| line == format!("linux {asset}kernel"))
        || !text
            .lines()
            .any(|line| line == format!("initrd {asset}initramfs"))
    {
        return Err(invalid(
            "Boot entry is not bound to the deployed transaction",
        ));
    }
    Ok(())
}
fn counter(entries: &Path, base: &str) -> io::Result<Option<(PathBuf, u8, u8)>> {
    let mut found = None;
    for item in fs::read_dir(entries)? {
        let item = item?;
        let filename = item
            .file_name()
            .into_string()
            .map_err(|_| invalid("Invalid boot filename"))?;
        let Some(counts) = filename
            .strip_prefix(&format!("{base}+"))
            .and_then(|v| v.strip_suffix(".conf"))
        else {
            continue;
        };
        let (left, done) = counts.split_once('-').unwrap_or((counts, "0"));
        let left: u8 = left.parse().map_err(invalid)?;
        let done: u8 = done.parse().map_err(invalid)?;
        if found.is_some()
            || left > MAX_TRIAL_BOOTS
            || done > MAX_TRIAL_BOOTS
            || left + done != MAX_TRIAL_BOOTS
        {
            return Err(invalid("Ambiguous or invalid firmware boot counters"));
        }
        found = Some((item.path(), left, done));
    }
    Ok(found)
}
fn receipt(d: &Deployment, action: &str) -> String {
    format!(
        "format=1\naction={action}\ntransaction={}\ntrial_generation={}\nrelease={}\nslot={}\n",
        d.transaction,
        d.trial_generation,
        d.release,
        name(d.candidate)
    )
}
fn record(state: &Path, d: &Deployment, action: &str) -> io::Result<()> {
    let path = state.join("health-intent");
    let expected = receipt(d, action);
    if path.try_exists()? {
        let previous = bounded(&path, 256)?;
        if previous == expected.as_bytes() {
            regular(&path)?.sync_all()?;
            return sync_directory(state);
        }
        // Only verified exhausted fallback may supersede an uncompleted
        // approval for this exact deployment. Other conflicting intents fail.
        if action != "rollback" || previous != receipt(d, "good").as_bytes() {
            return Err(invalid("Conflicting or stale health intent"));
        }
    }
    let pending = state.join(".health-intent.pending");
    match fs::remove_file(&pending) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    write_new(&pending, expected.as_bytes())?;
    fs::rename(&pending, &path)?;
    sync_directory(state)
}
fn has_receipt(state: &Path, d: &Deployment, action: &str) -> io::Result<bool> {
    let path = state.join("health-intent");
    if !path.try_exists()? {
        return Ok(false);
    }
    if bounded(&path, 256)? != receipt(d, action).as_bytes() {
        return Err(invalid("Health intent does not match this deployment"));
    }
    Ok(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Committed,
    Healthy,
    RolledBack,
    Pending,
}

fn settle(
    state: &Path,
    entries: &Path,
    layout: &Layout,
    d: &Deployment,
    boot: &Boot,
    firmware_entry: Option<&str>,
    bless: impl FnOnce() -> io::Result<()>,
) -> io::Result<Outcome> {
    let journal = Journal::open(state)?;
    let mut snapshot = journal
        .read()?
        .ok_or_else(|| invalid("Guest update journal is missing"))?;
    let base = format!("linos-{}-{}", d.release, name(d.candidate));
    let stable = entries.join(format!("{base}.conf"));
    let counted = counter(entries, &base)?;
    let good = expected_boot(layout, d, true);
    let fallback = expected_boot(layout, d, false);
    entry(
        &entries.join(format!(
            "linos-{}-{}.conf",
            d.active_release,
            name(d.active)
        )),
        &fallback,
    )?;
    if *boot == good {
        if let Phase::Healthy { active } = snapshot.phase {
            if active != d.candidate
                || snapshot.generation
                    != d.trial_generation
                        .checked_add(1)
                        .ok_or_else(|| invalid("Generation exhausted"))?
                || !has_receipt(state, d, "good")?
                || counted.is_some()
            {
                return Err(invalid("Healthy journal does not match candidate approval"));
            }
            entry(&stable, &good)?;
            return Ok(Outcome::Healthy);
        }
        if snapshot
            != (Snapshot {
                generation: d.trial_generation,
                phase: Phase::Trial {
                    active: d.active,
                    candidate: d.candidate,
                    attempts_left: MAX_TRIAL_BOOTS,
                },
            })
        {
            return Err(invalid("Candidate boot has a stale trial journal"));
        }
        if let Some((path, _, done)) = &counted {
            if stable.try_exists()?
                || *done == 0
                || firmware_entry != path.file_name().and_then(|s| s.to_str())
            {
                return Err(invalid(
                    "EFI boot identity does not match the current candidate",
                ));
            }
            entry(path, &good)?;
            record(state, d, "good")?;
            bless()?;
            if path.try_exists()? {
                return Err(invalid(
                    "Native boot blessing did not remove the current counter",
                ));
            }
        } else if !has_receipt(state, d, "good")? {
            return Err(invalid(
                "Uncounted candidate has no durable health approval",
            ));
        }
        entry(&stable, &good)?;
        sync_directory(entries)?;
        journal.apply(snapshot, Event::BootHealth { passed: true })?;
        Ok(Outcome::Committed)
    } else if *boot == fallback {
        if let Phase::Healthy { active } = snapshot.phase {
            if active != d.active
                || snapshot.generation
                    != d.trial_generation
                        .checked_add(3)
                        .ok_or_else(|| invalid("Generation exhausted"))?
                || !has_receipt(state, d, "rollback")?
            {
                return Err(invalid(
                    "Fallback journal has no matching rollback decision",
                ));
            }
        } else {
            let Some((path, left, done)) = &counted else {
                return Err(invalid("Fallback has no exhausted trial entry"));
            };
            entry(path, &good)?;
            if *left != 0 || *done != MAX_TRIAL_BOOTS {
                return Ok(Outcome::Pending);
            }
            if stable.try_exists()? {
                return Err(invalid("Candidate is both blessed and exhausted"));
            }
            let valid = match snapshot.phase {
                Phase::Trial {
                    active,
                    candidate,
                    attempts_left,
                } => {
                    active == d.active
                        && candidate == d.candidate
                        && d.trial_generation
                            .checked_add(u64::from(MAX_TRIAL_BOOTS - attempts_left))
                            == Some(snapshot.generation)
                }
                Phase::RollbackRequired { active, failed } => {
                    active == d.active
                        && failed == d.candidate
                        && d.trial_generation.checked_add(2) == Some(snapshot.generation)
                }
                _ => false,
            };
            if !valid {
                return Err(invalid("Fallback journal does not match this deployment"));
            }
            record(state, d, "rollback")?;
            loop {
                let event = match snapshot.phase {
                    Phase::Trial {
                        active,
                        candidate,
                        attempts_left,
                    } if active == d.active
                        && candidate == d.candidate
                        && snapshot.generation
                            == d.trial_generation + u64::from(MAX_TRIAL_BOOTS - attempts_left) =>
                    {
                        Event::BootHealth { passed: false }
                    }
                    Phase::RollbackRequired { active, failed }
                        if active == d.active
                            && failed == d.candidate
                            && snapshot.generation
                                == d.trial_generation
                                    .checked_add(2)
                                    .ok_or_else(|| invalid("Generation exhausted"))? =>
                    {
                        Event::ConfirmRollback
                    }
                    Phase::Healthy { active } if active == d.active => break,
                    _ => return Err(invalid("Fallback cannot reconcile this journal")),
                };
                snapshot = journal.apply(snapshot, event)?;
            }
        }
        if let Some((path, left, done)) = counted {
            if left != 0 || done != MAX_TRIAL_BOOTS {
                return Err(invalid("Fallback counter changed"));
            }
            fs::remove_file(path)?;
            sync_directory(entries)?;
        }
        Ok(Outcome::RolledBack)
    } else {
        Err(invalid(
            "Running root, release or transaction is not part of this deployment",
        ))
    }
}

fn efi_filename() -> io::Result<Option<String>> {
    let bytes = match bounded(Path::new(EFI_COUNT), 4096) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if bytes.len() < 6 || (bytes.len() - 4) % 2 != 0 {
        return Err(invalid("Malformed EFI boot count path"));
    }
    let words: Vec<_> = bytes[4..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let path = String::from_utf16(&words).map_err(invalid)?;
    let path = path.trim_end_matches('\0').replace('\\', "/");
    let name = path
        .strip_prefix("/loader/entries/")
        .ok_or_else(|| invalid("EFI boot count path is outside Linos entries"))?;
    if name.contains('/') {
        return Err(invalid("Invalid EFI boot filename"));
    }
    Ok(Some(name.to_owned()))
}

/// Only the live wrapper can collect actual root, mount and service evidence.
/// The policy is explicitly for managed development QEMU images.
pub fn approve_running_boot() -> io::Result<Outcome> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Guest health approval requires root",
        ));
    }
    let config = Path::new("/usr/lib/linos/update-layout");
    let metadata = regular(config)?.metadata()?;
    if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(invalid("Guest update layout is not protected"));
    }
    let layout = Layout::decode(&bounded(config, 512)?)?;
    let state = Path::new(STATE);
    let entries = Path::new(ENTRIES);
    if Layout::decode(&bounded(&state.join("layout"), 512)?)? != layout {
        return Err(invalid("Data layout does not match the booted image"));
    }
    let deployment = Deployment::decode(&bounded(&state.join("deployment"), 512)?)?;
    let boot = Boot::parse(&fs::read_to_string("/proc/cmdline")?)?;
    if boot.root != layout.root(boot.slot) {
        return Err(invalid("Boot slot does not match root UUID"));
    }
    validate_mounts(&layout, &boot, &fs::read_to_string("/proc/self/mountinfo")?)?;
    for service in ["linos-first-boot.service", "serial-getty@ttyS0.service"] {
        if !Command::new("/usr/bin/systemctl")
            .args(["is-active", "--quiet", service])
            .status()?
            .success()
        {
            return Err(invalid("Required guest boot service is not active"));
        }
    }
    let hostname = bounded(Path::new("/var/lib/linos/hostname"), 128)?;
    let hostname = std::str::from_utf8(&hostname).map_err(invalid)?.trim();
    let device_id = bounded(Path::new("/var/lib/linos/device-id"), 64)?;
    let device_id = std::str::from_utf8(&device_id).map_err(invalid)?.trim();
    if hostname.is_empty()
        || !uuid(device_id)
        || fs::read_to_string("/etc/hostname")?.trim() != hostname
        || rustix::system::uname()
            .nodename()
            .to_str()
            .map_err(invalid)?
            != hostname
    {
        return Err(invalid(
            "Running device identity does not match persistent Data",
        ));
    }
    let kernel_state = Path::new("/sys/kernel/linos/boot_state");
    if kernel_state.try_exists()? && fs::read_to_string(kernel_state)?.trim() != "ready" {
        return Err(invalid("Linos kernel readiness has not been acknowledged"));
    }
    let firmware = efi_filename()?;
    settle(
        state,
        entries,
        &layout,
        &deployment,
        &boot,
        firmware.as_deref(),
        || {
            if !Command::new("/usr/lib/systemd/systemd-bless-boot")
                .arg("good")
                .status()?
                .success()
            {
                return Err(invalid("Native systemd boot blessing failed"));
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";
    const DATA: &str = "33333333-3333-4333-8333-333333333333";
    const ESP: &str = "44444444-4444-4444-8444-444444444444";
    struct Fixture {
        root: PathBuf,
        state: PathBuf,
        entries: PathBuf,
        layout: Layout,
        deployment: Deployment,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "linos-health-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let state = root.join("state");
            let entries = root.join("entries");
            fs::create_dir(&state).unwrap();
            fs::create_dir(&entries).unwrap();
            let journal = Journal::open(&state).unwrap();
            let healthy = journal.initialize(Slot::A).unwrap();
            let staged = journal
                .apply(
                    healthy,
                    Event::Stage {
                        candidate: Slot::B,
                        verified: true,
                    },
                )
                .unwrap();
            journal.apply(staged, Event::BeginTrial).unwrap();
            drop(journal);
            let layout = Layout::new(A, B, DATA, ESP).unwrap();
            let deployment = Deployment {
                active: Slot::A,
                active_release: 1,
                active_transaction: 0,
                candidate: Slot::B,
                release: 2,
                transaction: 1,
                trial_generation: 2,
            };
            let result = Self {
                root,
                state,
                entries,
                layout,
                deployment,
            };
            result.write_entry("linos-1-a.conf", false);
            result.write_entry("linos-2-b+1-1.conf", true);
            result
        }
        fn write_entry(&self, filename: &str, candidate: bool) {
            let b = expected_boot(&self.layout, &self.deployment, candidate);
            fs::write(self.entries.join(filename),format!("linux /linos/{}-{}-{}/kernel\ninitrd /linos/{}-{}-{}/initramfs\noptions root=PARTUUID={} linos.slot={} linos.release={} linos.transaction={}\n",b.release,b.transaction,name(b.slot),b.release,b.transaction,name(b.slot),b.root,name(b.slot),b.release,b.transaction)).unwrap();
        }
        fn boot(&self, candidate: bool) -> Boot {
            expected_boot(&self.layout, &self.deployment, candidate)
        }
        fn settle(
            &self,
            boot: &Boot,
            bless: impl FnOnce() -> io::Result<()>,
        ) -> io::Result<Outcome> {
            settle(
                &self.state,
                &self.entries,
                &self.layout,
                &self.deployment,
                boot,
                Some("linos-2-b+1-1.conf"),
                bless,
            )
        }
        fn bless(&self) -> io::Result<()> {
            fs::rename(
                self.entries.join("linos-2-b+1-1.conf"),
                self.entries.join("linos-2-b.conf"),
            )
        }
        fn snapshot(&self) -> Snapshot {
            Journal::open(&self.state).unwrap().read().unwrap().unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn commits_only_current_candidate_and_repeated_health_is_idempotent() {
        let f = Fixture::new();
        assert_eq!(
            f.settle(&f.boot(true), || f.bless()).unwrap(),
            Outcome::Committed
        );
        assert_eq!(
            f.snapshot(),
            Snapshot {
                generation: 3,
                phase: Phase::Healthy { active: Slot::B }
            }
        );
        assert_eq!(
            f.settle(&f.boot(true), || panic!("already blessed"))
                .unwrap(),
            Outcome::Healthy
        );
    }
    #[test]
    fn resumes_blessed_entry_after_interrupted_journal_commit() {
        let f = Fixture::new();
        assert!(
            f.settle(&f.boot(true), || {
                f.bless()?;
                Err(io::Error::other("interrupted after native blessing"))
            })
            .is_err()
        );
        assert!(matches!(f.snapshot().phase, Phase::Trial { .. }));
        assert_eq!(
            f.settle(&f.boot(true), || panic!("must not bless twice"))
                .unwrap(),
            Outcome::Committed
        );
    }
    #[test]
    fn failed_health_cannot_commit_a_trial_and_native_failure_can_retry() {
        let f = Fixture::new();
        let original = f.snapshot();
        let mut wrong = f.boot(true);
        wrong.transaction = 99;
        assert!(f.settle(&wrong, || panic!("wrong transaction")).is_err());
        assert!(!f.state.join("health-intent").exists());
        assert!(
            f.settle(&f.boot(true), || Err(io::Error::other("native failure")))
                .is_err()
        );
        assert_eq!(f.snapshot(), original);
        assert_eq!(
            f.settle(&f.boot(true), || f.bless()).unwrap(),
            Outcome::Committed
        );
    }
    #[test]
    fn failed_native_blessing_does_not_block_exhausted_fallback() {
        let f = Fixture::new();
        assert!(
            f.settle(&f.boot(true), || Err(io::Error::other("native failure")))
                .is_err()
        );
        fs::rename(
            f.entries.join("linos-2-b+1-1.conf"),
            f.entries.join("linos-2-b+0-2.conf"),
        )
        .unwrap();
        assert_eq!(
            f.settle(&f.boot(false), || panic!("fallback must not bless"))
                .unwrap(),
            Outcome::RolledBack
        );
        assert_eq!(
            f.snapshot(),
            Snapshot {
                generation: 5,
                phase: Phase::Healthy { active: Slot::A }
            }
        );
        assert!(has_receipt(&f.state, &f.deployment, "rollback").unwrap());
    }
    #[test]
    fn interrupted_pending_intent_is_not_authoritative() {
        let f = Fixture::new();
        fs::write(f.state.join(".health-intent.pending"), b"format=").unwrap();
        assert!(!has_receipt(&f.state, &f.deployment, "good").unwrap());
        assert_eq!(
            f.settle(&f.boot(true), || f.bless()).unwrap(),
            Outcome::Committed
        );
        assert!(!f.state.join(".health-intent.pending").exists());
    }
    #[test]
    fn interrupted_rollback_intent_preserves_approval_until_retry() {
        let f = Fixture::new();
        record(&f.state, &f.deployment, "good").unwrap();
        fs::write(f.state.join(".health-intent.pending"), b"format=").unwrap();
        assert!(has_receipt(&f.state, &f.deployment, "good").unwrap());
        fs::rename(
            f.entries.join("linos-2-b+1-1.conf"),
            f.entries.join("linos-2-b+0-2.conf"),
        )
        .unwrap();
        assert_eq!(
            f.settle(&f.boot(false), || panic!("fallback must not bless"))
                .unwrap(),
            Outcome::RolledBack
        );
        assert!(has_receipt(&f.state, &f.deployment, "rollback").unwrap());
        assert!(!f.state.join(".health-intent.pending").exists());
    }
    #[test]
    fn rejects_unapproved_stable_entry_and_wrong_efi_identity() {
        let f = Fixture::new();
        let boot = f.boot(true);
        assert!(
            settle(
                &f.state,
                &f.entries,
                &f.layout,
                &f.deployment,
                &boot,
                Some("unrelated.conf"),
                || panic!("wrong EFI identity")
            )
            .is_err()
        );
        f.bless().unwrap();
        assert!(f.settle(&boot, || panic!("no approval")).is_err());
        assert!(matches!(f.snapshot().phase, Phase::Trial { .. }));
    }
    #[test]
    fn exhausted_fallback_is_durable_and_idempotent() {
        let f = Fixture::new();
        assert_eq!(
            f.settle(&f.boot(false), || panic!(
                "fallback does not bless candidate"
            ))
            .unwrap(),
            Outcome::Pending
        );
        fs::rename(
            f.entries.join("linos-2-b+1-1.conf"),
            f.entries.join("linos-2-b+0-2.conf"),
        )
        .unwrap();
        assert_eq!(
            f.settle(&f.boot(false), || panic!("fallback")).unwrap(),
            Outcome::RolledBack
        );
        assert_eq!(
            f.snapshot(),
            Snapshot {
                generation: 5,
                phase: Phase::Healthy { active: Slot::A }
            }
        );
        assert!(!f.entries.join("linos-2-b+0-2.conf").exists());
        assert_eq!(
            f.settle(&f.boot(false), || panic!("fallback retry"))
                .unwrap(),
            Outcome::RolledBack
        );
    }
    #[test]
    fn resumes_partially_recorded_rollback() {
        let f = Fixture::new();
        fs::rename(
            f.entries.join("linos-2-b+1-1.conf"),
            f.entries.join("linos-2-b+0-2.conf"),
        )
        .unwrap();
        let journal = Journal::open(&f.state).unwrap();
        journal
            .apply(
                journal.read().unwrap().unwrap(),
                Event::BootHealth { passed: false },
            )
            .unwrap();
        drop(journal);
        assert_eq!(
            f.settle(&f.boot(false), || panic!("fallback")).unwrap(),
            Outcome::RolledBack
        );
    }
    #[test]
    fn rejects_duplicate_identity_and_unsafe_mount_shapes() {
        let f = Fixture::new();
        assert_eq!(
            Layout::decode(f.layout.encode().as_bytes()).unwrap(),
            f.layout
        );
        assert!(Layout::new(A, A, DATA, ESP).is_err());
        assert!(Boot::parse(&format!("root=PARTUUID={B} root=PARTUUID={A} linos.slot=b linos.release=2 linos.transaction=1")).is_err());
        let good = "12 1 254:5 / /var/lib/linos rw,nosuid - ext4 /dev/vda5 rw\n";
        assert!(mount(good, "/var/lib/linos", "ext4").is_ok());
        for bad in [
            good.replace(" rw,nosuid ", " ro,nosuid "),
            good.replace("254:5 / ", "254:5 /subdir "),
            format!("{good}{good}"),
            good.replace("- ext4 ", "- tmpfs "),
        ] {
            assert!(mount(&bad, "/var/lib/linos", "ext4").is_err());
        }
        assert!(mount("", "/var/lib/linos", "ext4").is_err());
    }
}
