//! Service path/schema contracts. Runtime registry and retained descriptors are
//! checked separately before attaching data.
use super::{Fields, id};
use crate::recovery::records::{
    invalid,
    state::{Attachment, ServiceState},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathOwner {
    pub role: String,
    pub relative: String,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    id: String,
    executable_version: String,
    reader_min: u32,
    reader_max: u32,
    writer: u32,
    paths: Vec<PathOwner>,
    descriptor: Vec<u8>,
    descriptor_sha256: String,
}

impl Service {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn executable_version(&self) -> &str {
        &self.executable_version
    }
    pub fn paths(&self) -> &[PathOwner] {
        &self.paths
    }
    pub fn descriptor(&self) -> &[u8] {
        &self.descriptor
    }
    pub fn descriptor_sha256(&self) -> &str {
        &self.descriptor_sha256
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contract {
    services: Vec<Service>,
}

fn relative(value: &str) -> io::Result<String> {
    if value.len() > 1024
        || value.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
    {
        return Err(invalid(
            "Service path must be canonical and relative to its State role",
        ));
    }
    Ok(value.into())
}

impl Contract {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut fields = Fields::new(bytes)?;
        if fields.next("kind")? != "linos-services" || fields.next("format")? != "1" {
            return Err(invalid("Unsupported services metadata"));
        }
        let count = fields.number("count", 128)?;
        let mut services = Vec::new();
        let mut ids = BTreeSet::new();
        let mut owners: Vec<(String, String)> = Vec::new();
        for index in 0..count {
            let start = fields.position();
            let prefix = format!("service.{index:04}");
            let service_id = id(fields.next(&format!("{prefix}.id"))?)?;
            if !ids.insert(service_id.clone()) {
                return Err(invalid("Duplicate service identity"));
            }
            let executable_version = fields
                .next(&format!("{prefix}.executable_version"))?
                .to_owned();
            if executable_version.bytes().any(|b| b <= b' ') {
                return Err(invalid("Invalid service executable version"));
            }
            let reader_min =
                fields.number(&format!("{prefix}.reader_min"), u32::MAX as u64)? as u32;
            let reader_max =
                fields.number(&format!("{prefix}.reader_max"), u32::MAX as u64)? as u32;
            let writer = fields.number(&format!("{prefix}.writer"), u32::MAX as u64)? as u32;
            if reader_min > reader_max || writer < reader_min || writer > reader_max {
                return Err(invalid("Service writer cannot read its own schema"));
            }
            let count = fields.number(&format!("{prefix}.path_count"), 128)?;
            if count == 0 {
                return Err(invalid("Managed data service requires registered paths"));
            }
            let mut paths = Vec::new();
            for path in 0..count {
                let prefix = format!("{prefix}.path.{path:04}");
                let role = fields.next(&format!("{prefix}.role"))?;
                if !matches!(role, "var" | "srv") {
                    return Err(invalid("Service data path has an unsupported State role"));
                }
                let relative = relative(fields.next(&format!("{prefix}.relative"))?)?;
                if role == "var"
                    && ["lib/linos", "lib/dpkg", "lib/apt", "run", "lock"]
                        .iter()
                        .any(|reserved| {
                            relative == *reserved
                                || relative.starts_with(&format!("{reserved}/"))
                                || reserved.starts_with(&format!("{relative}/"))
                        })
                {
                    return Err(invalid("Service path collides with platform managed State"));
                }
                if owners.iter().any(|(other_role, other)| {
                    other_role == role
                        && (*other == relative
                            || other.starts_with(&format!("{relative}/"))
                            || relative.starts_with(&format!("{other}/")))
                }) {
                    return Err(invalid("Service data registrations overlap"));
                }
                owners.push((role.into(), relative.clone()));
                paths.push(PathOwner {
                    role: role.into(),
                    relative,
                    uid: fields.number(&format!("{prefix}.uid"), u32::MAX as u64 - 1)? as u32,
                    gid: fields.number(&format!("{prefix}.gid"), u32::MAX as u64 - 1)? as u32,
                });
            }
            if fields.bytes_since(start).len() > 64 * 1024 {
                return Err(invalid(
                    "Service descriptor exceeds the scalar record bound",
                ));
            }
            services.push(Service {
                id: service_id,
                executable_version,
                reader_min,
                reader_max,
                writer,
                paths,
                descriptor: fields.bytes_since(start).to_vec(),
                descriptor_sha256: crate::recovery::records::deployment::sha256(
                    fields.bytes_since(start),
                )?,
            });
        }
        fields.finish()?;
        Ok(Self { services })
    }
    pub fn services(&self) -> &[Service] {
        &self.services
    }

    /// Descriptors have already been authenticated through their retained source
    /// manifests. Every current record, including dormant data, must retain one.
    /// Removing a service needs a durable dormant record before switching.
    pub fn require_registry(
        &self,
        states: &[ServiceState],
        retained: &BTreeMap<String, Service>,
    ) -> io::Result<()> {
        let mut seen = BTreeSet::new();
        for state in states {
            if !seen.insert(state.service_id()) {
                return Err(invalid("Duplicate service State record"));
            }
            let pinned = retained
                .get(state.contract_sha256())
                .ok_or_else(|| invalid("Missing retained service descriptor"))?;
            if pinned.id != state.service_id()
                || pinned.descriptor_sha256 != state.contract_sha256()
            {
                return Err(invalid("Retained descriptor names a different service"));
            }
            match self
                .services
                .iter()
                .find(|service| service.id == state.service_id())
            {
                Some(target) => {
                    if target.paths != pinned.paths {
                        return Err(invalid("Service path or numeric ownership changed"));
                    }
                    state.require_schema(target.reader_min, target.reader_max, target.writer)?;
                }
                None if state.attachment() == Attachment::Active => {
                    return Err(invalid(
                        "Removed service must be recorded dormant before selection",
                    ));
                }
                None => {}
            }
        }
        if self
            .services
            .iter()
            .any(|service| !seen.contains(service.id.as_str()))
        {
            return Err(invalid(
                "Service must have durable registration before first attachment",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contract(path: &str, writer: u32) -> Contract {
        Contract::parse(format!("kind=linos-services\nformat=1\ncount=1\nservice.0000.id=db\nservice.0000.executable_version=1.0\nservice.0000.reader_min=1\nservice.0000.reader_max=2\nservice.0000.writer={writer}\nservice.0000.path_count=1\nservice.0000.path.0000.role=var\nservice.0000.path.0000.relative={path}\nservice.0000.path.0000.uid=50\nservice.0000.path.0000.gid=50\n").as_bytes()).unwrap()
    }
    #[test]
    fn dormant_reattachment_requires_pinned_paths_and_schema() {
        let target = contract("lib/db", 1);
        let pinned = target.services()[0].clone();
        let retained = BTreeMap::from([(pinned.descriptor_sha256().into(), pinned.clone())]);
        for attachment in ["active", "dormant"] {
            let state = ServiceState::parse(format!("kind=linos-service-state\nformat=1\nservice_id=db\nschema=1\nattachment={attachment}\ncontract_sha256={}\nmigration_receipt_sha256=none\n", pinned.descriptor_sha256()).as_bytes()).unwrap();
            target
                .require_registry(std::slice::from_ref(&state), &retained)
                .unwrap();
            assert!(
                target
                    .require_registry(std::slice::from_ref(&state), &BTreeMap::new())
                    .is_err()
            );
            assert!(
                contract("lib/other", 1)
                    .require_registry(std::slice::from_ref(&state), &retained)
                    .is_err()
            );
            assert!(
                contract("lib/db", 2)
                    .require_registry(std::slice::from_ref(&state), &retained)
                    .is_err()
            );
            let removed = Contract::parse(b"kind=linos-services\nformat=1\ncount=0\n").unwrap();
            assert_eq!(
                removed
                    .require_registry(std::slice::from_ref(&state), &retained)
                    .is_ok(),
                attachment == "dormant"
            );
        }
        assert!(target.require_registry(&[], &retained).is_err());
    }
    #[test]
    fn unsupported_paths_counts_and_indices_refuse() {
        let valid = contract("lib/db", 1);
        let mut bytes = b"kind=linos-services\nformat=1\ncount=1\n".to_vec();
        bytes.extend_from_slice(valid.services()[0].descriptor());
        for (from, to) in [
            ("relative=lib/db", "relative=../db"),
            ("relative=lib/db", "relative=lib/linos"),
            ("count=1\n", "count=129\n"),
            ("service.0000.id", "service.0001.id"),
            ("writer=1", "writer=3"),
        ] {
            assert!(
                Contract::parse(
                    String::from_utf8(bytes.clone())
                        .unwrap()
                        .replace(from, to)
                        .as_bytes()
                )
                .is_err()
            );
        }
    }
}
