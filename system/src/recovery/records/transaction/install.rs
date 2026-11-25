//! Durable initial publication evidence. The installer must verify target bytes,
//! independent Recovery and sync their files before initializing the journal.
use super::{Entry, Operation, Phase, Transaction, boot};
use crate::recovery::records::{
    Layout,
    deployment::{OfficialManifest, sha256},
    hash, invalid, ordered,
    recovery::OfficialRecovery,
    state::{Configuration, positive},
    uuid,
};
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    transaction: u64,
    volume_uuid: String,
    deployment_id: String,
    configuration_sha256: String,
    recovery_manifest_sha256: String,
    entry_sha256: String,
    kernel_sha256: String,
    initramfs_sha256: String,
    device_key_sha256: String,
}

impl Installation {
    pub fn new(
        layout: &Layout,
        manifest: &OfficialManifest,
        configuration: &Configuration,
        recovery: &OfficialRecovery,
        transaction: u64,
    ) -> io::Result<Self> {
        if recovery.architecture() != manifest.deployment().architecture
            || recovery.hardware_profile() != manifest.deployment().hardware_profile
        {
            return Err(invalid(
                "Installation Recovery targets a different platform",
            ));
        }
        Self::build(layout, manifest, configuration, recovery.id(), transaction)
    }
    fn build(
        layout: &Layout,
        manifest: &OfficialManifest,
        configuration: &Configuration,
        recovery_id: &str,
        transaction: u64,
    ) -> io::Result<Self> {
        layout.require_plain()?;
        if layout.baseline_manifest_sha256() != manifest.id() {
            return Err(invalid(
                "Initial deployment differs from the layout baseline",
            ));
        }
        configuration.require_selection(1, manifest.id(), manifest.deployment())?;
        let entry = Entry::new(manifest.id(), transaction)?;
        Ok(Self {
            transaction,
            volume_uuid: layout.volume_uuid().into(),
            deployment_id: manifest.id().into(),
            configuration_sha256: sha256(configuration.encode().as_bytes())?,
            recovery_manifest_sha256: hash(recovery_id)?,
            entry_sha256: sha256(boot::render(layout, &entry, false).as_bytes())?,
            kernel_sha256: manifest.deployment().artifacts[1].sha256.clone(),
            initramfs_sha256: manifest.deployment().artifacts[2].sha256.clone(),
            device_key_sha256: layout.device_key_sha256().into(),
        })
    }
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let f = ordered(
            bytes,
            [
                "kind",
                "format",
                "transaction",
                "volume_uuid",
                "deployment_id",
                "configuration_sha256",
                "recovery_manifest_sha256",
                "entry_sha256",
                "kernel_sha256",
                "initramfs_sha256",
                "device_key_sha256",
            ],
        )?;
        if f[0] != "linos-installation" || f[1] != "1" {
            return Err(invalid("Unsupported installation receipt"));
        }
        Ok(Self {
            transaction: positive(f[2])?,
            volume_uuid: uuid(f[3])?,
            deployment_id: hash(f[4])?,
            configuration_sha256: hash(f[5])?,
            recovery_manifest_sha256: hash(f[6])?,
            entry_sha256: hash(f[7])?,
            kernel_sha256: hash(f[8])?,
            initramfs_sha256: hash(f[9])?,
            device_key_sha256: hash(f[10])?,
        })
    }
    pub fn encode(&self) -> String {
        format!(
            "kind=linos-installation\nformat=1\ntransaction={}\nvolume_uuid={}\ndeployment_id={}\nconfiguration_sha256={}\nrecovery_manifest_sha256={}\nentry_sha256={}\nkernel_sha256={}\ninitramfs_sha256={}\ndevice_key_sha256={}\n",
            self.transaction,
            self.volume_uuid,
            self.deployment_id,
            self.configuration_sha256,
            self.recovery_manifest_sha256,
            self.entry_sha256,
            self.kernel_sha256,
            self.initramfs_sha256,
            self.device_key_sha256
        )
    }
    pub fn digest(&self) -> io::Result<String> {
        sha256(self.encode().as_bytes())
    }
    pub fn transaction(&self) -> u64 {
        self.transaction
    }
    pub fn deployment_id(&self) -> &str {
        &self.deployment_id
    }
    pub fn entry_sha256(&self) -> &str {
        &self.entry_sha256
    }
    pub fn verify(
        &self,
        snapshot: &Transaction,
        layout: &Layout,
        manifest: &OfficialManifest,
        configuration: &Configuration,
    ) -> io::Result<()> {
        let initial = snapshot.phase() == Phase::InstallPending
            && snapshot.generation() == 0
            && snapshot.healthy().is_none();
        let complete = snapshot.phase() == Phase::Healthy
            && snapshot.generation() == 1
            && snapshot.healthy() == Some(&Entry::new(&self.deployment_id, self.transaction)?);
        if (!initial && !complete)
            || snapshot.operation() != Operation::Install
            || snapshot.candidate().is_some()
            || snapshot.previous().is_some()
            || snapshot.selected_config_generation() != 1
            || snapshot.source_config_generation() != 1
            || snapshot.selected_deployment_id() != self.deployment_id
            || snapshot.publication_receipt_sha256() != Some(self.digest()?.as_str())
            || *self
                != Self::build(
                    layout,
                    manifest,
                    configuration,
                    &self.recovery_manifest_sha256,
                    snapshot.transaction(),
                )?
        {
            return Err(invalid(
                "Installation receipt differs from native boot selection",
            ));
        }
        Ok(())
    }
    pub fn health_receipt(&self) -> io::Result<String> {
        Ok(format!(
            "kind=linos-initial-health\nformat=1\ntransaction={}\nsource_journal_generation=0\npublication_receipt_sha256={}\nconfiguration_sha256={}\nentry_sha256={}\ndecision=setup-ready\n",
            self.transaction,
            self.digest()?,
            self.configuration_sha256,
            self.entry_sha256
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_publication_receipts_are_canonical_and_versioned() {
        let record = Installation {
            transaction: 1,
            volume_uuid: "11111111-1111-4111-8111-111111111111".into(),
            deployment_id: "a".repeat(64),
            configuration_sha256: "b".repeat(64),
            recovery_manifest_sha256: "c".repeat(64),
            entry_sha256: "d".repeat(64),
            kernel_sha256: "e".repeat(64),
            initramfs_sha256: "f".repeat(64),
            device_key_sha256: "1".repeat(64),
        };
        let text = record.encode();
        assert_eq!(Installation::parse(text.as_bytes()).unwrap(), record);
        for (from, to) in [
            ("format=1", "format=2"),
            ("transaction=1", "transaction=01"),
            ("volume_uuid=11111111", "volume_uuid=invalid"),
        ] {
            assert!(Installation::parse(text.replace(from, to).as_bytes()).is_err());
        }
        assert!(Installation::parse(format!("{text}extra=value\n").as_bytes()).is_err());
        assert!(
            record
                .health_receipt()
                .unwrap()
                .contains(&record.digest().unwrap())
        );
    }
}
