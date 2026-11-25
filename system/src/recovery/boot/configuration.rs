//! Configuration selection belongs to early boot, under the volume
//! journal lock and before persistent state is bound into the selected root.
use super::*;
use crate::recovery::records::transaction::{Event, Operation, Phase, receipts::FallbackReceipt};

pub(super) fn select_candidate(
    journal: &Journal,
    snapshot: &Transaction,
    layout: &Layout,
    target: &Loaded,
    key: &[u8],
    selectors: &Selectors,
) -> io::Result<Transaction> {
    let publication = journal.publication(snapshot)?;
    publication.verify(snapshot, layout, target.manifest())?;
    let configuration = prepare(journal, snapshot, target, key, None)?;
    validate_target(target, key, &configuration)?;
    require_observation(journal, layout, selectors, BootDecision::Candidate)?;
    journal.apply(
        snapshot,
        Event::SelectCandidate {
            configuration: &configuration,
        },
    )
}

fn prepare(
    journal: &Journal,
    snapshot: &Transaction,
    target: &Loaded,
    key: &[u8],
    fallback: Option<&FallbackReceipt>,
) -> io::Result<Configuration> {
    let source = Loaded::open(
        Path::new(&format!(
            "{VOLUME}/System/deployments/{}",
            snapshot.selected_deployment_id()
        )),
        key,
    )?;
    let source_configuration = selected_configuration(snapshot, &source)?;
    Registry::load(Path::new(VOLUME), key)?.require_target(&target.requirements().services)?;
    if let Some(receipt) = fallback {
        return generation::published_fallback(
            Path::new(VOLUME),
            snapshot,
            target,
            source_configuration.schema(),
            receipt,
            &journal.publication(snapshot)?,
        );
    }
    let configuration = match generation::published(
        Path::new(VOLUME),
        snapshot,
        target,
        source_configuration.schema(),
    )? {
        Some(configuration) => configuration,
        None => {
            let old =
                native::VendorRoot::mount(source.artifacts().system_image(), "vendor-source")?;
            let new =
                native::VendorRoot::mount(target.artifacts().system_image(), "vendor-target")?;
            let workspace = native::configuration_workspace()?;
            let prepared = generation::Prepared::build(generation::Inputs {
                volume: Path::new(VOLUME),
                journal: snapshot,
                configuration: &source_configuration,
                source: &source,
                target,
                base_vendor: &old.etc(),
                current: Path::new(&format!(
                    "{VOLUME}/State/config/{}",
                    snapshot.selected_config_generation()
                )),
                target_vendor: &new.etc(),
                workspace: &workspace,
            })?;
            prepared.publish(Path::new(VOLUME), journal)?
        }
    };
    Ok(configuration)
}

fn require_observation(
    journal: &Journal,
    layout: &Layout,
    selectors: &Selectors,
    decision: BootDecision,
) -> io::Result<crate::recovery::records::transaction::boot::Observation> {
    let observed = journal.inspect_boot(layout)?;
    if observed.decision() != decision || observed.selectors() != selectors {
        return Err(invalid(
            "Boot selection changed during configuration preparation",
        ));
    }
    Ok(observed)
}

pub(super) fn validate_target(
    target: &Loaded,
    key: &[u8],
    configuration: &Configuration,
) -> io::Result<()> {
    target.artifacts().revalidate()?;
    require_accounts(target, configuration.generation())?;
    super::super::records::transaction::boot::verify_efi_artifacts(target.manifest())?;
    Registry::load(Path::new(VOLUME), key)?.require_target(&target.requirements().services)?;
    Ok(())
}

fn failed_manifest(
    snapshot: &Transaction,
    key: &[u8],
    id: &str,
) -> io::Result<super::super::records::deployment::OfficialManifest> {
    let directory = open_directory(Path::new(&format!("{VOLUME}/System/deployments/{id}")))?;
    let manifest = source::manifest(&directory, key)?;
    if manifest.id() != id || snapshot.publication_receipt_sha256().is_none() {
        return Err(invalid(
            "Failed deployment differs from durable fallback authority",
        ));
    }
    Ok(manifest)
}

pub(super) fn select_fallback(
    journal: &Journal,
    snapshot: &Transaction,
    layout: &Layout,
    target: &Loaded,
    key: &[u8],
    selectors: &Selectors,
) -> io::Result<Transaction> {
    require_observation(journal, layout, selectors, BootDecision::MergeFallback)?;
    let exhausted = if snapshot.phase() == Phase::Trial {
        journal.apply(snapshot, Event::Exhausted)?
    } else {
        snapshot.clone()
    };
    let failed_id = exhausted
        .candidate()
        .ok_or_else(|| invalid("Missing exhausted candidate"))?
        .deployment_id();
    let failed = failed_manifest(&exhausted, key, failed_id)?;
    let publication = journal.publication(&exhausted)?;
    publication.verify(&exhausted, layout, &failed)?;
    let existing = journal.fallback_intent(&exhausted)?;
    let configuration = prepare(journal, &exhausted, target, key, existing.as_ref())?;
    validate_target(target, key, &configuration)?;
    let observed = require_observation(journal, layout, selectors, BootDecision::MergeFallback)?;
    let intent = match existing {
        Some(_) => exhausted,
        None => {
            let receipt = FallbackReceipt::new(
                &exhausted,
                &publication,
                &observed,
                &configuration,
                target.manifest(),
            )?;
            journal.record_fallback_intent(
                &exhausted,
                layout,
                &failed,
                target.manifest(),
                &configuration,
                &receipt,
            )?
        }
    };
    journal.resume_fallback(&intent, layout, &failed, target.manifest(), &configuration)
}

/// A completed selection may still have its exhausted EFI entry if cleanup was
/// interrupted. Reconcile it before ordinary inventory rejects unknown entries.
pub(super) fn resume_retirement(
    journal: &Journal,
    snapshot: &Transaction,
    layout: &Layout,
    target: &Loaded,
    key: &[u8],
) -> io::Result<()> {
    if snapshot.phase() != Phase::Healthy || snapshot.operation() == Operation::Install {
        return Ok(());
    }
    let configuration = selected_configuration(snapshot, target)?;
    validate_target(target, key, &configuration)?;
    if let Some(receipt) = journal.fallback_intent(snapshot)? {
        let failed = failed_manifest(snapshot, key, receipt.failed().deployment_id())?;
        journal.resume_fallback(snapshot, layout, &failed, target.manifest(), &configuration)?;
    } else {
        journal.resume_approval(snapshot, layout, target.manifest())?;
    }
    Ok(())
}
