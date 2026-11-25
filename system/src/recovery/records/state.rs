//! Mutable State record syntax and explicit compatibility checks.
//! Parsing neither authenticates State nor establishes journal authority.

use super::{deployment::Deployment, hash, invalid, ordered};
use std::io;

pub(super) fn decimal(value: &str) -> io::Result<u64> {
    if value.is_empty()
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value.len() > 1 && value.starts_with('0')
    {
        return Err(invalid("Expected canonical unsigned decimal"));
    }
    value.parse().map_err(invalid)
}

pub(super) fn positive(value: &str) -> io::Result<u64> {
    let value = decimal(value)?;
    if value == 0 {
        return Err(invalid("Configuration generations start at one"));
    }
    Ok(value)
}

fn schema(value: &str) -> io::Result<u32> {
    u32::try_from(decimal(value)?).map_err(invalid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationPhase {
    Prepared,
    Selected,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    generation: u64,
    deployment_id: String,
    parent_generation: Option<u64>,
    schema: u32,
    merge_receipt_sha256: String,
    phase: ConfigurationPhase,
}

impl Configuration {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(
            bytes,
            [
                "kind",
                "format",
                "generation",
                "deployment_id",
                "parent_generation",
                "schema",
                "merge_receipt_sha256",
                "phase",
            ],
        )?;
        if f[0] != "linos-configuration" || f[1] != "1" {
            return Err(invalid("Unsupported configuration kind or format"));
        }
        let generation = positive(f[2])?;
        let parent_generation = match f[4] {
            "none" => None,
            value => Some(positive(value)?),
        };
        if (generation == 1) != parent_generation.is_none()
            || parent_generation.is_some_and(|parent| parent >= generation)
        {
            return Err(invalid("Configuration parent must precede its generation"));
        }
        let phase = match f[7] {
            "prepared" => ConfigurationPhase::Prepared,
            "selected" => ConfigurationPhase::Selected,
            "retired" => ConfigurationPhase::Retired,
            _ => return Err(invalid("Unknown configuration phase")),
        };
        Ok(Self {
            generation,
            deployment_id: hash(f[3])?,
            parent_generation,
            schema: schema(f[5])?,
            merge_receipt_sha256: hash(f[6])?,
            phase,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn parent_generation(&self) -> Option<u64> {
        self.parent_generation
    }
    pub fn deployment_id(&self) -> &str {
        &self.deployment_id
    }
    pub fn schema(&self) -> u32 {
        self.schema
    }
    pub fn merge_receipt_sha256(&self) -> &str {
        &self.merge_receipt_sha256
    }
    pub fn phase(&self) -> ConfigurationPhase {
        self.phase
    }

    /// The caller must supply the reconciled journal selection and authenticated
    /// deployment with its exact manifest digest. This does not verify receipts,
    /// account contents, directory ancestry or the journal itself.
    pub fn require_selection(
        &self,
        selected_generation: u64,
        selected_deployment_id: &str,
        deployment: &Deployment,
    ) -> io::Result<()> {
        hash(selected_deployment_id)?;
        if self.generation != selected_generation
            || self.deployment_id != selected_deployment_id
            || self.phase != ConfigurationPhase::Selected
            || self.schema < deployment.config_schema_min
            || self.schema > deployment.config_schema_max
        {
            return Err(invalid("Configuration selection or schema is incompatible"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attachment {
    Active,
    Dormant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceState {
    service_id: String,
    schema: u32,
    attachment: Attachment,
    contract_sha256: String,
    migration_receipt_sha256: Option<String>,
}

impl ServiceState {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(
            bytes,
            [
                "kind",
                "format",
                "service_id",
                "schema",
                "attachment",
                "contract_sha256",
                "migration_receipt_sha256",
            ],
        )?;
        if f[0] != "linos-service-state" || f[1] != "1" {
            return Err(invalid("Unsupported service state kind or format"));
        }
        if f[2].len() > 64
            || !f[2]
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            || !f[2]
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        {
            return Err(invalid("Invalid service identifier"));
        }
        let attachment = match f[4] {
            "active" => Attachment::Active,
            "dormant" => Attachment::Dormant,
            _ => return Err(invalid("Unknown service attachment")),
        };
        Ok(Self {
            service_id: f[2].to_owned(),
            schema: schema(f[3])?,
            attachment,
            contract_sha256: hash(f[5])?,
            migration_receipt_sha256: match f[6] {
                "none" => None,
                value => Some(hash(value)?),
            },
        })
    }

    pub fn service_id(&self) -> &str {
        &self.service_id
    }
    pub fn schema(&self) -> u32 {
        self.schema
    }
    pub fn attachment(&self) -> Attachment {
        self.attachment
    }
    pub fn contract_sha256(&self) -> &str {
        &self.contract_sha256
    }
    pub fn migration_receipt_sha256(&self) -> Option<&str> {
        self.migration_receipt_sha256.as_deref()
    }

    /// Check the schema portion of attachment, including dormant reattachment.
    /// Caller must separately validate the new and pinned descriptors, registered
    /// paths, numeric ownership and authentication evidence before permitting I/O.
    pub fn require_schema(&self, reader_min: u32, reader_max: u32, writer: u32) -> io::Result<()> {
        if reader_min > reader_max
            || self.schema < reader_min
            || self.schema > reader_max
            || writer != self.schema
        {
            return Err(invalid(
                "Service requires a compatible schema preserving writer",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(generation: &str, parent: &str, schema: &str, phase: &str) -> String {
        format!(
            "kind=linos-configuration\nformat=1\ngeneration={generation}\ndeployment_id={}\nparent_generation={parent}\nschema={schema}\nmerge_receipt_sha256={}\nphase={phase}\n",
            "a".repeat(64),
            "b".repeat(64)
        )
    }

    fn service(id: &str, schema: &str, attachment: &str, receipt: &str) -> String {
        format!(
            "kind=linos-service-state\nformat=1\nservice_id={id}\nschema={schema}\nattachment={attachment}\ncontract_sha256={}\nmigration_receipt_sha256={receipt}\n",
            "c".repeat(64)
        )
    }

    #[test]
    fn configuration_lineage_and_schema_bounds() {
        let initial =
            Configuration::parse(config("1", "none", "0", "prepared").as_bytes()).unwrap();
        assert_eq!(initial.parent_generation(), None);
        assert_eq!(initial.phase(), ConfigurationPhase::Prepared);
        let later =
            Configuration::parse(config("10", "7", "4294967295", "retired").as_bytes()).unwrap();
        assert_eq!(later.generation(), 10);
        assert_eq!(later.parent_generation(), Some(7));
        assert_eq!(later.schema(), u32::MAX);
        assert_eq!(later.deployment_id(), "a".repeat(64));
        assert_eq!(later.merge_receipt_sha256(), "b".repeat(64));
        for (generation, parent) in [
            ("0", "none"),
            ("01", "none"),
            ("1", "1"),
            ("2", "none"),
            ("2", "0"),
            ("2", "2"),
            ("2", "3"),
            ("18446744073709551616", "1"),
        ] {
            assert!(
                Configuration::parse(config(generation, parent, "1", "selected").as_bytes())
                    .is_err()
            );
        }
        for value in ["01", "-1", "+1", "4294967296", "1 "] {
            assert!(
                Configuration::parse(config("1", "none", value, "selected").as_bytes()).is_err()
            );
        }
    }

    #[test]
    fn selection_requires_current_pair_phase_and_schema() {
        let mut deployment = Deployment {
            origin: super::super::deployment::Origin::Ci,
            source_sha: "a".repeat(40),
            build_id: "test".into(),
            release: 1,
            architecture: "amd64".into(),
            hardware_profile: "generic".into(),
            kernel_release: "test".into(),
            config_schema_min: 2,
            config_schema_max: 3,
            artifacts: std::array::from_fn(|_| super::super::deployment::Artifact {
                bytes: 1,
                sha256: "a".repeat(64),
            }),
        };
        let record = Configuration::parse(config("2", "1", "2", "selected").as_bytes()).unwrap();
        let id = "a".repeat(64);
        assert!(record.require_selection(2, &id, &deployment).is_ok());
        assert!(record.require_selection(1, &id, &deployment).is_err());
        assert!(
            record
                .require_selection(2, &"b".repeat(64), &deployment)
                .is_err()
        );
        assert!(record.require_selection(2, "none", &deployment).is_err());
        for phase in ["prepared", "retired"] {
            let record = Configuration::parse(config("2", "1", "2", phase).as_bytes()).unwrap();
            assert!(record.require_selection(2, &id, &deployment).is_err());
        }
        deployment.config_schema_min = 3;
        assert!(record.require_selection(2, &id, &deployment).is_err());
        deployment.config_schema_min = 0;
        deployment.config_schema_max = 1;
        assert!(record.require_selection(2, &id, &deployment).is_err());
    }

    #[test]
    fn service_identity_attachment_and_receipts() {
        let record =
            ServiceState::parse(service("db-main.1", "0", "dormant", "none").as_bytes()).unwrap();
        assert_eq!(record.service_id(), "db-main.1");
        assert_eq!(record.attachment(), Attachment::Dormant);
        assert_eq!(record.migration_receipt_sha256(), None);
        assert_eq!(record.contract_sha256(), "c".repeat(64));
        let receipt = "d".repeat(64);
        let record =
            ServiceState::parse(service("db", "4294967295", "active", &receipt).as_bytes())
                .unwrap();
        assert_eq!(record.schema(), u32::MAX);
        assert_eq!(record.migration_receipt_sha256(), Some(receipt.as_str()));
        for id in ["", ".db", "../db", "DB", "db/path", &"a".repeat(65)] {
            assert!(ServiceState::parse(service(id, "1", "active", "none").as_bytes()).is_err());
        }
        for value in ["01", "-1", "4294967296"] {
            assert!(
                ServiceState::parse(service("db", value, "active", "none").as_bytes()).is_err()
            );
        }
        assert!(ServiceState::parse(service("db", "1", "removed", "none").as_bytes()).is_err());
    }

    #[test]
    fn both_active_and_dormant_attachments_require_preserving_writers() {
        for attachment in ["active", "dormant"] {
            let record =
                ServiceState::parse(service("db", "3", attachment, "none").as_bytes()).unwrap();
            assert!(record.require_schema(2, 4, 3).is_ok());
            assert!(record.require_schema(3, 3, 3).is_ok());
            for (min, max, writer) in [(4, 5, 3), (0, 2, 3), (4, 2, 3), (2, 4, 2), (2, 4, 4)] {
                assert!(record.require_schema(min, max, writer).is_err());
            }
        }
    }

    #[test]
    fn refuses_noncanonical_records_and_hashes() {
        let config = config("1", "none", "1", "selected");
        let service = service("db", "1", "active", "none");
        for text in [&config, &service] {
            let parse = |bytes: &[u8]| {
                if text.starts_with("kind=linos-configuration") {
                    Configuration::parse(bytes).map(|_| ())
                } else {
                    ServiceState::parse(bytes).map(|_| ())
                }
            };
            assert!(parse(text.trim_end().as_bytes()).is_err());
            assert!(parse(format!("{text}\n").as_bytes()).is_err());
            assert!(parse(format!("{text}format=1\n").as_bytes()).is_err());
            assert!(parse(text.replace("format=1", "format=2").as_bytes()).is_err());
            assert!(parse(text.replace("sha256=", "sha256=A").as_bytes()).is_err());
            assert!(parse(text.replace('\n', "\r\n").as_bytes()).is_err());
        }
        assert!(
            Configuration::parse(config.replace("phase=selected", "phase=unknown").as_bytes())
                .is_err()
        );
        assert!(
            ServiceState::parse(service.replace("sha256=none", "sha256=NONE").as_bytes()).is_err()
        );
    }
}
