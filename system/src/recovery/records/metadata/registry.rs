//! Current service registry and independently retained signed contract evidence.
use super::services::{Contract, Service};
use crate::recovery::{
    records::{
        deployment::{sha256, source},
        invalid,
        state::ServiceState,
    },
    update::journal::open_directory,
};
use rustix::fs::{self, Mode, OFlags};
use std::{
    collections::BTreeMap,
    fs::File,
    io,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::Path,
};

pub struct Registry {
    states: Vec<ServiceState>,
    retained: BTreeMap<String, Service>,
}

fn service_path(volume: &File, service: &Service) -> io::Result<()> {
    for path in service.paths() {
        let mut directory = File::from(fs::openat(
            volume,
            "State",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        directory = File::from(fs::openat(
            &directory,
            path.role.as_str(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let parts: Vec<_> = path.relative.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            let flags = OFlags::RDONLY
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC
                | if index + 1 < parts.len() {
                    OFlags::DIRECTORY
                } else {
                    OFlags::empty()
                };
            directory = File::from(fs::openat(&directory, *part, flags, Mode::empty())?);
            let metadata = directory.metadata()?;
            if index + 1 == parts.len() {
                if (!metadata.is_file() && !metadata.is_dir())
                    || metadata.uid() != path.uid
                    || metadata.gid() != path.gid
                {
                    return Err(invalid(
                        "Retained service data path or numeric ownership differs from its contract",
                    ));
                }
            } else if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
                return Err(invalid(
                    "Service data ancestry is writable by an unrelated principal",
                ));
            }
        }
    }
    Ok(())
}

impl Registry {
    /// Paths are derived from the verified private volume, never from arbitrary
    /// absolute metadata paths. The supplied key is provisioned independently.
    pub fn load(volume: &Path, trusted_key: &[u8]) -> io::Result<Self> {
        let volume_fd = open_directory(volume)?;
        let state = volume.join("State/var/lib/linos");
        let directory = open_directory(&state.join("services"))?;
        let mut states = Vec::new();
        let mut retained = BTreeMap::new();
        for item in std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
            if states.len() == 128 {
                return Err(invalid("Service registry exceeds its bound"));
            }
            let name = item?
                .file_name()
                .into_string()
                .map_err(|_| invalid("Invalid registry filename"))?;
            let id = name
                .strip_suffix(".record")
                .ok_or_else(|| invalid("Unknown service registry file"))?;
            let record = ServiceState::parse(&source::read_at(&directory, &name, 64 * 1024)?)?;
            if record.service_id() != id || record.migration_receipt_sha256().is_some() {
                return Err(invalid(
                    "Registry identity mismatch or unsupported service migration",
                ));
            }
            let evidence = open_directory(
                &state
                    .join("service-contracts")
                    .join(record.contract_sha256()),
            )?;
            let manifest = source::manifest(&evidence, trusted_key)?;
            let bytes = source::read_at(&evidence, "services", 16 * 1024 * 1024)?;
            let expected = &manifest.deployment().artifacts[5];
            if bytes.len() as u64 != expected.bytes || sha256(&bytes)? != expected.sha256 {
                return Err(invalid("Retained service list differs from signed source"));
            }
            let contracts = Contract::parse(&bytes)?;
            let service = contracts
                .services()
                .iter()
                .find(|service| {
                    service.id() == id && service.descriptor_sha256() == record.contract_sha256()
                })
                .ok_or_else(|| {
                    invalid("Pinned service descriptor is absent from authenticated metadata")
                })?;
            if source::read_at(&evidence, "descriptor", 64 * 1024)? != service.descriptor() {
                return Err(invalid(
                    "Retained descriptor bytes differ from authenticated source",
                ));
            }
            service_path(&volume_fd, service)?;
            retained.insert(record.contract_sha256().into(), service.clone());
            states.push(record);
        }
        Ok(Self { states, retained })
    }
    pub fn require_target(&self, target: &Contract) -> io::Result<()> {
        target.require_registry(&self.states, &self.retained)
    }
    pub fn states(&self) -> &[ServiceState] {
        &self.states
    }
}
