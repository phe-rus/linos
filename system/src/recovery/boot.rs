//! Native layout 2 early boot. No caller supplied device, sysfs, trust or runtime
//! overrides. The existing layout remains separate and is never inferred here.
use super::{
    configuration::generation,
    mounts,
    records::{
        Layout,
        deployment::{
            sha256,
            source::{self, Loaded},
        },
        metadata::registry::Registry,
        state::Configuration,
        transaction::{BootDecision, Transaction, boot::Selectors, journal::Journal},
    },
    update::journal::open_directory,
};
use rustix::fs::{self, Mode, OFlags};
use std::{
    ffi::CString,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt},
    },
    path::Path,
};

mod native;
const VOLUME: &str = "/run/linos/volume";
const KEY: &str = "/usr/lib/linos/trust/deployment.pem";
const RECEIPT: &str = "boot-receipt";

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn selectors() -> io::Result<Selectors> {
    let mut bytes = Vec::new();
    File::open("/proc/cmdline")?
        .take(16385)
        .read_to_end(&mut bytes)?;
    let selectors = Selectors::parse(&bytes)?;
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(invalid("Layout 2 native boot requires root"));
    }
    Ok(selectors)
}

fn block_device(file: &File) -> io::Result<mounts::Device> {
    let metadata = file.metadata()?;
    if !metadata.file_type().is_block_device() {
        return Err(invalid("Expected a retained native block descriptor"));
    }
    Ok(mounts::Device {
        major: rustix::fs::major(metadata.rdev()),
        minor: rustix::fs::minor(metadata.rdev()),
    })
}

struct Context {
    layout: Layout,
    journal: Journal,
    snapshot: Transaction,
    loaded: Loaded,
    configuration: Configuration,
    selectors: Selectors,
    key: Vec<u8>,
}

fn context(selectors: Selectors) -> io::Result<Context> {
    let storage = open_directory(Path::new(&format!("{VOLUME}/State/var/lib/linos/storage")))?;
    let layout = Layout::parse(&source::read_at(&storage, "layout", 64 * 1024)?)?;
    layout.require_plain()?;
    if layout.linos_partuuid() != selectors.volume_partuuid {
        return Err(invalid("Boot selector and installed layout disagree"));
    }
    let (volume, esp) = mounts::inspect_volume(&layout)?;
    for (path, expected) in [(VOLUME, volume), ("/efi", esp)] {
        let device = std::fs::metadata(path)?.dev();
        if rustix::fs::major(device) != expected.major
            || rustix::fs::minor(device) != expected.minor
        {
            return Err(invalid(
                "Early boot mounts differ from the native layout identity",
            ));
        }
    }
    let efi = open_directory(Path::new("/efi/EFI/Linos/volume"))?;
    if Layout::parse(&source::read_at(&efi, "layout", 64 * 1024)?)? != layout {
        return Err(invalid("EFI and State layout copies disagree"));
    }
    let journal = Journal::open(Path::new(&format!(
        "{VOLUME}/State/var/lib/linos/updates/v3"
    )))?;
    let snapshot = journal
        .read()?
        .ok_or_else(|| invalid("Missing layout 2 transaction journal"))?;
    let key = source::trusted_key(Path::new(KEY))?;
    let loaded = Loaded::open(
        Path::new(&format!(
            "{VOLUME}/System/deployments/{}",
            selectors.entry.deployment_id()
        )),
        &key,
    )?;
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return Err(invalid("Unsupported boot architecture")),
    };
    let uname = rustix::system::uname();
    loaded.manifest().deployment().require_target(
        architecture,
        &format!("qemu-{architecture}-uefi"),
        uname
            .release()
            .to_str()
            .map_err(|_| invalid("Invalid running kernel identity"))?,
    )?;
    let observed = journal.inspect_boot(&layout)?;
    if observed.selectors() != &selectors {
        return Err(invalid(
            "Native boot selection changed during deployment authentication",
        ));
    }
    if !matches!(
        observed.decision(),
        BootDecision::InitialSetup | BootDecision::Healthy | BootDecision::Candidate
    ) {
        return Err(invalid(
            "Boot requires configuration/fallback reconciliation before root attachment",
        ));
    }
    if snapshot.selected_deployment_id() != selectors.entry.deployment_id() {
        return Err(invalid(
            "Candidate configuration must be prepared and durably selected before root attachment",
        ));
    }
    let configs = open_directory(Path::new(&format!(
        "{VOLUME}/State/var/lib/linos/configuration"
    )))?;
    let configuration = Configuration::parse(&source::read_at(
        &configs,
        &format!("{}.record", snapshot.selected_config_generation()),
        64 * 1024,
    )?)?;
    snapshot.require_selected_configuration(&configuration, loaded.manifest())?;
    let current =
        generation::accounts(&super::configuration::tree::snapshot(Path::new(&format!(
            "{VOLUME}/State/config/{}",
            snapshot.selected_config_generation()
        )))?)?;
    if !loaded
        .requirements()
        .accounts
        .plan(&loaded.requirements().accounts, &current)?
        .is_empty()
    {
        return Err(invalid(
            "Selected configuration is missing required native accounts",
        ));
    }
    Registry::load(Path::new(VOLUME), &key)?.require_target(&loaded.requirements().services)?;
    for service in loaded.requirements().services.services() {
        if service
            .paths()
            .iter()
            .any(|path| !current.owns(path.uid, path.gid))
        {
            return Err(invalid(
                "Selected service data has no current numeric account owner",
            ));
        }
    }
    super::records::transaction::boot::verify_efi_artifacts(loaded.manifest())?;
    Ok(Context {
        layout,
        journal,
        snapshot,
        loaded,
        configuration,
        selectors,
        key,
    })
}

fn receipt(context: &Context) -> io::Result<String> {
    Ok(format!(
        "kind=linos-boot\nformat=1\nvolume_uuid={}\ndeployment_id={}\nentry_transaction={}\nconfig_generation={}\ntrust_key_sha256={}\n",
        context.layout.volume_uuid(),
        context.loaded.manifest().id(),
        context.selectors.entry.transaction(),
        context.configuration.generation(),
        sha256(&context.key)?
    ))
}

/// initramfs local-top, before waiting for /dev/linos-system.
pub fn prepare_root() -> io::Result<()> {
    let selected = selectors()?;
    native::runtime_directories()?;
    let volume = mounts::open_partition(&selected.volume_partuuid)?;
    native::mount_file(
        &volume,
        VOLUME,
        "ext4",
        libc::MS_NODEV | libc::MS_NOSUID,
        "",
    )?;
    let storage = open_directory(Path::new(&format!("{VOLUME}/State/var/lib/linos/storage")))?;
    let layout = Layout::parse(&source::read_at(&storage, "layout", 64 * 1024)?)?;
    let (observed_volume, observed_esp) = mounts::inspect_volume(&layout)?;
    if block_device(&volume)? != observed_volume {
        return Err(invalid(
            "Bootstrap volume differs from complete native layout inspection",
        ));
    }
    let esp = mounts::open_partition(layout.esp_partuuid())?;
    if block_device(&esp)? != observed_esp {
        return Err(invalid("Bootstrap ESP differs from native inventory"));
    }
    native::mount_file(
        &esp,
        "/efi",
        "vfat",
        libc::MS_NODEV | libc::MS_NOSUID | libc::MS_NOEXEC,
        "umask=0077",
    )?;
    let context = context(selected)?;
    context.loaded.artifacts().revalidate()?;
    native::attach_readonly(context.loaded.artifacts().system_image())?;
    let directory = open_directory(Path::new("/run/linos"))?;
    let mut file = File::from(fs::openat(
        &directory,
        RECEIPT,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?);
    file.write_all(receipt(&context)?.as_bytes())?;
    file.sync_all()?;
    directory.sync_all()
}

/// initramfs local-bottom. Debian then moves /run with these child mounts into
/// the real root. Image package metadata is pinned before State covers /var.
pub fn mount_state() -> io::Result<()> {
    let context = context(selectors()?)?;
    let marker = std::fs::read("/root/usr/lib/linos/storage-layout")?;
    if marker != b"layout=2\n" {
        return Err(invalid(
            "Selected root lacks the explicit layout 2 capability marker",
        ));
    }
    native::bind("/root", "/run/linos/image", true)?;
    let generation = context.configuration.generation();
    for (source, target) in [
        (format!("{VOLUME}/State/config/{generation}"), "/root/etc"),
        (format!("{VOLUME}/Users"), "/root/home"),
        (format!("{VOLUME}/Users/root"), "/root/root"),
        (format!("{VOLUME}/State/var"), "/root/var"),
        (format!("{VOLUME}/State/local"), "/root/usr/local"),
        (format!("{VOLUME}/State/srv"), "/root/srv"),
        (format!("{VOLUME}/State/tmp/tmp"), "/root/tmp"),
        (format!("{VOLUME}/State/tmp/var-tmp"), "/root/var/tmp"),
    ] {
        native::bind(&source, target, false)?;
    }
    native::bind("/run/linos/image/var/lib/dpkg", "/root/var/lib/dpkg", true)?;
    native::bind(
        "/run/linos/image/var/lib/apt/extended_states",
        "/root/var/lib/apt/extended_states",
        true,
    )?;
    native::move_mount("/efi", "/root/efi")?;
    // Do not select a configuration from a stale source after mount work.
    if context.journal.read()?.as_ref() != Some(&context.snapshot) {
        return Err(invalid("Journal changed during persistent mount setup"));
    }
    Ok(())
}

/// Real init gate, before identity, enrollment or service data writes.
pub fn verify_running() -> io::Result<()> {
    let context = context(selectors()?)?;
    let runtime = open_directory(Path::new("/run/linos"))?;
    if source::read_at(&runtime, RECEIPT, 4096)? != receipt(&context)?.as_bytes() {
        return Err(invalid(
            "Runtime boot receipt disagrees with selected identity",
        ));
    }
    mounts::inspect(
        &context.layout,
        context.loaded.artifacts(),
        context.configuration.generation(),
    )
}
