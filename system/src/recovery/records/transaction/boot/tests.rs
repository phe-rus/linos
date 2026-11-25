use super::*;
use crate::recovery::records::transaction::Operation;

fn layout() -> Layout {
    Layout::parse(format!("kind=linos-layout\nformat=2\nvolume_uuid=11111111-1111-1111-1111-111111111111\ndisk_guid=22222222-2222-2222-2222-222222222222\nesp_partuuid=33333333-3333-3333-3333-333333333333\nrecovery_partuuid=44444444-4444-4444-4444-444444444444\nlinos_partuuid=55555555-5555-5555-5555-555555555555\nfilesystem=ext4\nfilesystem_uuid=66666666-6666-6666-6666-666666666666\nencryption=none\nluks_uuid=none\ndevice_key_sha256={}\nbaseline_manifest_sha256={}\n", "a".repeat(64), "b".repeat(64)).as_bytes()).unwrap()
}

fn variable(value: &str) -> Vec<u8> {
    let mut bytes = 6u32.to_le_bytes().to_vec();
    for word in value.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn options(entry: &Entry, trial: bool) -> Vec<u8> {
    render(&layout(), entry, trial)
        .lines()
        .find_map(|line| line.strip_prefix("options "))
        .unwrap()
        .as_bytes()
        .to_vec()
}

fn healthy() -> Transaction {
    let id = "a".repeat(64);
    let receipt = "b".repeat(64);
    // Start at 9 to exercise numeric ordering across 9 -> 10.
    Transaction::installed(&id, 9, &receipt)
        .unwrap()
        .approve(&Entry::new(&id, 9).unwrap(), &receipt)
        .unwrap()
}

fn trial() -> Transaction {
    healthy()
        .prepare(Operation::Packages, &"e".repeat(64))
        .unwrap()
        .staged()
        .unwrap()
        .begin_trial()
        .unwrap()
        .published(&"b".repeat(64))
        .unwrap()
}

fn inventory_fixture(journal: &Transaction, counter: Counter) -> BTreeMap<String, Vec<u8>> {
    let mut entries = BTreeMap::new();
    let layout = layout();
    for healthy in [journal.healthy(), journal.previous()]
        .into_iter()
        .flatten()
    {
        entries.insert(
            filename(healthy, Counter::Stable),
            render(&layout, healthy, false).into_bytes(),
        );
    }
    if let Some(candidate) = journal.candidate() {
        entries.insert(
            filename(candidate, counter),
            render(&layout, candidate, true).into_bytes(),
        );
    }
    entries
}

fn firmware_fixture(entry: &Entry, counter: Counter) -> Firmware {
    Firmware {
        selected: variable(&filename(entry, Counter::Stable)),
        counted: if counter == Counter::Stable {
            None
        } else {
            Some(variable(&format!(
                "\\loader\\entries\\{}",
                filename(entry, counter)
            )))
        },
    }
}

#[test]
fn selectors_require_unique_plain_readonly_loop_contract() {
    let entry = Entry::new(&"a".repeat(64), 10).unwrap();
    let cmdline = String::from_utf8(options(&entry, true)).unwrap();
    let parsed = Selectors::parse(cmdline.as_bytes()).unwrap();
    assert_eq!(parsed.entry, entry);
    assert!(parsed.blessing_masked);
    let firmware = format!(
        "initrd=\\EFI\\Linos\\deployments\\{}\\initramfs {cmdline}",
        entry.deployment_id()
    );
    assert_eq!(Selectors::parse(firmware.as_bytes()).unwrap(), parsed);
    assert!(Selectors::parse(firmware.replace("\\initramfs", "\\other").as_bytes()).is_err());
    assert!(Selectors::parse(format!("{firmware} initrd=/another").as_bytes()).is_err());
    for bad in [
        format!("{cmdline} rw"),
        format!("{cmdline} ro"),
        format!("{cmdline} linos.layout=2"),
        format!("{cmdline} linos.unknown=1"),
        format!("{cmdline} {MASK}"),
        cmdline.replace("rootflags=noload", "rootflags=rw"),
        cmdline.replace("linos.transaction=10", "linos.transaction=010"),
        cmdline.replace("root=/dev/linos-system", "root=/dev/vda3"),
        format!("{cmdline} \"rw\""),
        format!("{cmdline}\nro"),
    ] {
        assert!(Selectors::parse(bad.as_bytes()).is_err(), "accepted {bad}");
    }
    assert!(Selectors::parse(&vec![b'a'; MAX_TEXT + 1]).is_err());
}

#[test]
fn entry_assets_ordering_and_counter_forms_are_exact() {
    let journal = trial();
    let layout = layout();
    let entry = journal.candidate().unwrap();
    assert_eq!(entry.transaction(), 10);
    let text = render(&layout, entry, true);
    assert!(text.contains("version 10\n"));
    assert!(entry_contents(text.as_bytes(), &layout, entry, true).is_ok());
    for (from, to) in [
        ("version 10", "version 9"),
        ("sort-key linos", "sort-key other"),
        ("/kernel", "/other"),
        ("/initramfs", "/other"),
        (MASK, "quiet"),
    ] {
        assert!(entry_contents(text.replace(from, to).as_bytes(), &layout, entry, true).is_err());
    }
    assert!(
        entry_contents(
            format!("{text}version 10\n").as_bytes(),
            &layout,
            entry,
            true
        )
        .is_err()
    );
    for counter in [
        Counter::Stable,
        Counter::Untried,
        Counter::First,
        Counter::Last,
    ] {
        assert_eq!(
            parse_filename(&filename(entry, counter)).unwrap(),
            (entry.clone(), counter)
        );
    }
    let base = filename(entry, Counter::Stable);
    for count in ["+02", "+2-0", "+1", "+0-3", "+3", "+1-01", "+1-1+0-2"] {
        assert!(parse_filename(&base.replace(".conf", &format!("{count}.conf"))).is_err());
    }
}

#[test]
fn counted_boot_binds_firmware_kernel_entry_and_journal() {
    let journal = trial();
    let entry = journal.candidate().unwrap();
    let layout = layout();
    for counter in [Counter::First, Counter::Last] {
        let entries = inventory_fixture(&journal, counter);
        let firmware = firmware_fixture(entry, counter);
        let observed = reconcile(
            &layout,
            &journal,
            &options(entry, true),
            &firmware,
            &entries,
        )
        .unwrap();
        assert_eq!(observed.decision(), BootDecision::Candidate);
        assert_eq!(&observed.selectors().entry, entry);
        let mut wrong = firmware.clone();
        wrong.selected = variable("other.conf");
        assert!(reconcile(&layout, &journal, &options(entry, true), &wrong, &entries).is_err());
        let mut wrong = firmware.clone();
        wrong.counted = None;
        assert!(reconcile(&layout, &journal, &options(entry, true), &wrong, &entries).is_err());
        let mut wrong = firmware.clone();
        wrong.counted = Some(variable("\\elsewhere\\candidate.conf"));
        assert!(reconcile(&layout, &journal, &options(entry, true), &wrong, &entries).is_err());
        assert!(
            reconcile(
                &layout,
                &journal,
                &options(entry, false),
                &firmware,
                &entries
            )
            .is_err()
        );
    }
    let entries = inventory_fixture(&journal, Counter::Untried);
    assert!(
        reconcile(
            &layout,
            &journal,
            &options(entry, true),
            &firmware_fixture(entry, Counter::Untried),
            &entries
        )
        .is_err()
    );
}

#[test]
fn fallback_requires_exact_exhaustion_and_stable_candidate_is_not_blessed_implicitly() {
    let journal = trial();
    let healthy = journal.healthy().unwrap();
    let layout = layout();
    let firmware = firmware_fixture(healthy, Counter::Stable);
    for counter in [Counter::Untried, Counter::First, Counter::Stable] {
        assert!(
            reconcile(
                &layout,
                &journal,
                &options(healthy, false),
                &firmware,
                &inventory_fixture(&journal, counter)
            )
            .is_err()
        );
    }
    let entries = inventory_fixture(&journal, Counter::Last);
    assert_eq!(
        reconcile(
            &layout,
            &journal,
            &options(healthy, false),
            &firmware,
            &entries
        )
        .unwrap()
        .decision(),
        BootDecision::MergeFallback
    );
    let mut duplicate = entries.clone();
    let candidate = journal.candidate().unwrap();
    duplicate.insert(
        filename(candidate, Counter::First),
        render(&layout, candidate, true).into_bytes(),
    );
    assert!(
        reconcile(
            &layout,
            &journal,
            &options(healthy, false),
            &firmware,
            &duplicate
        )
        .is_err()
    );
    let mut missing = entries.clone();
    missing.remove(&filename(healthy, Counter::Stable));
    assert!(
        reconcile(
            &layout,
            &journal,
            &options(candidate, true),
            &firmware_fixture(candidate, Counter::Last),
            &missing
        )
        .is_err()
    );
    let mut unknown = entries.clone();
    let other = Entry::new(&"f".repeat(64), 8).unwrap();
    unknown.insert(
        filename(&other, Counter::Stable),
        render(&layout, &other, false).into_bytes(),
    );
    assert!(
        reconcile(
            &layout,
            &journal,
            &options(healthy, false),
            &firmware,
            &unknown
        )
        .is_err()
    );
}

#[test]
fn stable_initial_and_healthy_boots_and_strict_efi_decoding() {
    let journal = healthy();
    let entry = journal.healthy().unwrap();
    let layout = layout();
    let entries = inventory_fixture(&journal, Counter::Stable);
    let firmware = firmware_fixture(entry, Counter::Stable);
    assert_eq!(
        reconcile(
            &layout,
            &journal,
            &options(entry, false),
            &firmware,
            &entries
        )
        .unwrap()
        .decision(),
        BootDecision::Healthy
    );
    let initial = Transaction::installed(entry.deployment_id(), 9, &"b".repeat(64)).unwrap();
    assert_eq!(
        reconcile(
            &layout,
            &initial,
            &options(entry, false),
            &firmware,
            &entries
        )
        .unwrap()
        .decision(),
        BootDecision::InitialSetup
    );
    let name = filename(entry, Counter::Stable);
    assert_eq!(efi_text(&variable(&name)).unwrap(), name);
    for bad in [
        vec![],
        variable(""),
        variable("x\0y"),
        variable("é"),
        vec![0; 1026],
    ] {
        assert!(efi_text(&bad).is_err());
    }
    let mut truncated = variable(&name);
    truncated.pop();
    assert!(efi_text(&truncated).is_err());
    let mut extra_terminator = variable(&name);
    extra_terminator.extend_from_slice(&[0, 0]);
    assert!(efi_text(&extra_terminator).is_err());
}

#[test]
fn initial_health_commits_only_the_exact_authenticated_installation_and_retries_receipt_publication()
 {
    use crate::recovery::records::{
        signing_fixture::Signer, state::Configuration, transaction::journal::Journal,
    };
    use std::os::unix::fs::PermissionsExt;
    let signer = Signer::new();
    let manifest = signer.deployment();
    let recovery = signer.recovery();
    let installed_layout = Layout::parse(
        layout()
            .encode()
            .replace(
                &format!("baseline_manifest_sha256={}", "b".repeat(64)),
                &format!("baseline_manifest_sha256={}", manifest.id()),
            )
            .as_bytes(),
    )
    .unwrap();
    let (configuration, _) = Configuration::initial(manifest.id(), 1, &"c".repeat(64)).unwrap();
    let path = std::env::current_dir()
        .unwrap()
        .join("target/initial-health-tests")
        .join(std::process::id().to_string());
    std::fs::create_dir_all(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let journal = Journal::open(&path).unwrap();
    let initial = journal
        .initialize_install(&installed_layout, &manifest, &configuration, &recovery, 1)
        .unwrap();
    let publication = journal.initial_publication(&initial).unwrap();
    let entry = Entry::new(manifest.id(), 1).unwrap();
    let entries = BTreeMap::from([(
        filename(&entry, Counter::Stable),
        render(&layout(), &entry, false).into_bytes(),
    )]);
    let observed = reconcile(
        &layout(),
        &initial,
        &options(&entry, false),
        &firmware_fixture(&entry, Counter::Stable),
        &entries,
    )
    .unwrap();
    let other_entry = Entry::new(&"f".repeat(64), 1).unwrap();
    let other_initial =
        Transaction::installed(other_entry.deployment_id(), 1, &"b".repeat(64)).unwrap();
    let other_entries = BTreeMap::from([(
        filename(&other_entry, Counter::Stable),
        render(&layout(), &other_entry, false).into_bytes(),
    )]);
    let wrong = reconcile(
        &layout(),
        &other_initial,
        &options(&other_entry, false),
        &firmware_fixture(&other_entry, Counter::Stable),
        &other_entries,
    )
    .unwrap();
    assert!(
        journal
            .approve_initial(
                &initial,
                &installed_layout,
                &manifest,
                &configuration,
                &wrong
            )
            .is_err()
    );
    assert_eq!(journal.read().unwrap(), Some(initial.clone()));
    journal
        .persist_receipt(&initial, publication.health_receipt().unwrap().as_bytes())
        .unwrap();
    let healthy = journal
        .approve_initial(
            &initial,
            &installed_layout,
            &manifest,
            &configuration,
            &observed,
        )
        .unwrap();
    assert_eq!(healthy.healthy(), Some(&entry));
    assert_eq!(healthy.generation(), 1);
    journal
        .verify_initial_health(&healthy, &installed_layout, &manifest, &configuration)
        .unwrap();
    assert!(
        journal
            .initialize_install(&installed_layout, &manifest, &configuration, &recovery, 1)
            .is_err()
    );
    assert!(
        journal
            .approve_initial(
                &healthy,
                &installed_layout,
                &manifest,
                &configuration,
                &observed
            )
            .is_err()
    );
    let file = path.join(format!(
        "receipts/1/{}.record",
        healthy.health_receipt_sha256().unwrap()
    ));
    std::fs::write(file, b"tampered initial health").unwrap();
    assert!(
        journal
            .verify_initial_health(&healthy, &installed_layout, &manifest, &configuration)
            .is_err()
    );
    drop(journal);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn completed_fallback_retires_only_exact_exhausted_bytes_and_retries_after_removal() {
    use crate::recovery::records::{deployment::sha256, state::Configuration};
    let journal = trial();
    let failed = journal.candidate().unwrap();
    let good = journal.healthy().unwrap();
    let entries = inventory_fixture(&journal, Counter::Last);
    let expected = sha256(render(&layout(), failed, true).as_bytes()).unwrap();
    let configuration =
        Configuration::selected(2, good.deployment_id(), Some(1), 1, &"f".repeat(64)).unwrap();
    let digest = "b".repeat(64);
    let completed = journal
        .exhausted()
        .unwrap()
        .fallback_intent(&digest)
        .unwrap()
        .fallback(&configuration, &digest)
        .unwrap();
    let observe = |entries: &BTreeMap<String, Vec<u8>>| {
        reconcile(
            &layout(),
            &completed,
            &options(good, false),
            &firmware_fixture(good, Counter::Stable),
            entries,
        )
    };
    assert!(observe(&entries).is_err());
    let retirement = retirement_inventory(failed, &expected, entries.clone()).unwrap();
    assert_eq!(retirement.name, filename(failed, Counter::Last));
    assert!(retirement.bytes.is_some());
    assert_eq!(
        observe(&retirement.remaining).unwrap().decision(),
        BootDecision::Healthy
    );
    let retry = retirement_inventory(failed, &expected, retirement.remaining).unwrap();
    assert!(retry.bytes.is_none());
    assert_eq!(
        observe(&retry.remaining).unwrap().decision(),
        BootDecision::Healthy
    );
    let mut corrupted = entries;
    corrupted.get_mut(&retirement.name).unwrap().push(b' ');
    assert!(retirement_inventory(failed, &expected, corrupted).is_err());
    let early = retirement_inventory(
        failed,
        &expected,
        inventory_fixture(&journal, Counter::First),
    )
    .unwrap();
    assert!(early.bytes.is_none());
    assert!(observe(&early.remaining).is_err());
    let mut missing = retry.remaining;
    missing.remove(&filename(good, Counter::Stable));
    assert!(observe(&missing).is_err());
}

#[test]
fn interrupted_blessing_resumes_only_matching_persisted_intent_and_unchanged_bytes() {
    use crate::recovery::records::{
        deployment::sha256,
        state::Configuration,
        transaction::receipts::{HealthIntent, Publication},
    };
    let layout = layout();
    let journal = healthy()
        .prepare(Operation::Update, &"e".repeat(64))
        .unwrap()
        .staged()
        .unwrap()
        .begin_trial()
        .unwrap();
    let candidate = journal.candidate().unwrap();
    let good = journal.healthy().unwrap();
    let entry_digest = sha256(render(&layout, candidate, true).as_bytes()).unwrap();
    let publication = Publication::parse(format!("kind=linos-publication\nformat=1\ntransaction={}\nsource_journal_generation={}\nsource_config_generation=1\nhealthy_id={}\nhealthy_entry_transaction={}\ncandidate_id={}\ncandidate_entry_transaction={}\nentry_sha256={entry_digest}\nkernel_sha256={}\ninitramfs_sha256={}\n", journal.transaction(), journal.generation(), good.deployment_id(), good.transaction(), candidate.deployment_id(), candidate.transaction(), "c".repeat(64), "d".repeat(64)).as_bytes()).unwrap();
    let journal = journal.published(&publication.digest().unwrap()).unwrap();
    let config = Configuration::parse(format!("kind=linos-configuration\nformat=1\ngeneration=2\ndeployment_id={}\nparent_generation=1\nschema=1\nmerge_receipt_sha256={}\nphase=selected\n", journal.candidate().unwrap().deployment_id(), "f".repeat(64)).as_bytes()).unwrap();
    let journal = journal.select_candidate(&config).unwrap();
    let candidate = journal.candidate().unwrap();
    let entries = inventory_fixture(&journal, Counter::First);
    let firmware = firmware_fixture(candidate, Counter::First);
    let observed = reconcile(
        &layout,
        &journal,
        &options(candidate, true),
        &firmware,
        &entries,
    )
    .unwrap();
    let health = HealthIntent::new(&journal, &publication, &observed).unwrap();
    let intent = journal.health_intent(&health.digest().unwrap()).unwrap();
    health.verify(&intent, &publication).unwrap();
    let mut stable = entries.clone();
    let bytes = stable.remove(&filename(candidate, Counter::First)).unwrap();
    stable.insert(filename(candidate, Counter::Stable), bytes);
    assert!(
        reconcile(
            &layout,
            &intent,
            &options(candidate, true),
            &firmware,
            &stable
        )
        .is_err()
    );
    let observed = reconcile_impl(
        &layout,
        &intent,
        &options(candidate, true),
        &firmware,
        &stable,
        Some(&health),
    )
    .unwrap();
    assert_eq!(observed.decision(), BootDecision::ResumeApproval);
    let approved = intent
        .approve(candidate, &health.digest().unwrap())
        .unwrap();
    assert_eq!(approved.previous(), journal.healthy());
    assert_eq!(approved.healthy(), journal.candidate());
    assert_eq!(approved.selected_config_generation(), 2);
    health.verify_completed(&approved, &publication).unwrap();
    assert_eq!(
        reconcile_impl(
            &layout,
            &approved,
            &options(candidate, true),
            &firmware,
            &stable,
            Some(&health)
        )
        .unwrap()
        .decision(),
        BootDecision::Healthy
    );
    // Rebooted after native blessing has no volatile count variable.
    let rebooted = firmware_fixture(candidate, Counter::Stable);
    assert_eq!(
        reconcile_impl(
            &layout,
            &intent,
            &options(candidate, true),
            &rebooted,
            &stable,
            Some(&health)
        )
        .unwrap()
        .decision(),
        BootDecision::ResumeApproval
    );
    let name = filename(candidate, Counter::Stable);
    let altered = String::from_utf8(stable.get(&name).unwrap().clone())
        .unwrap()
        .replace("title Linos Server", "title Changed");
    stable.insert(name, altered.into_bytes());
    assert!(
        reconcile_impl(
            &layout,
            &intent,
            &options(candidate, true),
            &firmware,
            &stable,
            Some(&health)
        )
        .is_err()
    );
    let mut wrong = firmware.clone();
    wrong.counted = Some(variable("\\loader\\entries\\another+1-1.conf"));
    assert!(
        reconcile_impl(
            &layout,
            &intent,
            &options(candidate, true),
            &wrong,
            &entries,
            Some(&health)
        )
        .is_err()
    );
}

#[test]
fn repeated_approval_binds_and_retires_only_the_older_previous_entry() {
    use crate::recovery::records::{
        deployment::sha256,
        state::Configuration,
        transaction::receipts::{HealthIntent, Publication},
    };
    let digest = "b".repeat(64);
    let oldest = Entry::new(&"d".repeat(64), 8).unwrap();
    let initial = Transaction::installed(oldest.deployment_id(), 8, &digest)
        .unwrap()
        .approve(&oldest, &digest)
        .unwrap();
    let first = initial
        .prepare(Operation::Update, &"a".repeat(64))
        .unwrap()
        .staged()
        .unwrap()
        .begin_trial()
        .unwrap()
        .published(&digest)
        .unwrap();
    let first_config = Configuration::selected(2, &"a".repeat(64), Some(1), 1, &digest).unwrap();
    let healthy = first
        .select_candidate(&first_config)
        .unwrap()
        .approve(first.candidate().unwrap(), &digest)
        .unwrap();
    let trial = healthy
        .prepare(Operation::Update, &"e".repeat(64))
        .unwrap()
        .staged()
        .unwrap()
        .begin_trial()
        .unwrap();
    let candidate = trial.candidate().unwrap();
    let good = trial.healthy().unwrap();
    let entry_digest = sha256(render(&layout(), candidate, true).as_bytes()).unwrap();
    let publication = Publication::parse(format!("kind=linos-publication\nformat=1\ntransaction={}\nsource_journal_generation={}\nsource_config_generation=2\nhealthy_id={}\nhealthy_entry_transaction={}\ncandidate_id={}\ncandidate_entry_transaction={}\nentry_sha256={entry_digest}\nkernel_sha256={digest}\ninitramfs_sha256={digest}\n", trial.transaction(), trial.generation(), good.deployment_id(), good.transaction(), candidate.deployment_id(), candidate.transaction()).as_bytes()).unwrap();
    let published = trial.published(&publication.digest().unwrap()).unwrap();
    let configuration =
        Configuration::selected(3, candidate.deployment_id(), Some(2), 1, &digest).unwrap();
    let selected = published.select_candidate(&configuration).unwrap();
    let mut entries = inventory_fixture(&selected, Counter::First);
    let firmware = firmware_fixture(candidate, Counter::First);
    let observed = reconcile(
        &layout(),
        &selected,
        &options(candidate, true),
        &firmware,
        &entries,
    )
    .unwrap();
    let health = HealthIntent::new(&selected, &publication, &observed).unwrap();
    assert_eq!(health.previous(), Some(&oldest));
    let parsed = HealthIntent::parse(health.encode().as_bytes()).unwrap();
    assert_eq!(parsed, health);
    let intent = selected.health_intent(&health.digest().unwrap()).unwrap();
    let completed = intent
        .approve(candidate, &health.digest().unwrap())
        .unwrap();
    health.verify_completed(&completed, &publication).unwrap();
    let bytes = entries
        .remove(&filename(candidate, Counter::First))
        .unwrap();
    entries.insert(filename(candidate, Counter::Stable), bytes);
    assert!(
        reconcile_impl(
            &layout(),
            &completed,
            &options(candidate, true),
            &firmware,
            &entries,
            Some(&health)
        )
        .is_err()
    );
    let retirement = approval_retirement_inventory(&completed, &health, entries.clone()).unwrap();
    assert_eq!(retirement.name, Some(filename(&oldest, Counter::Stable)));
    assert_eq!(
        reconcile_impl(
            &layout(),
            &completed,
            &options(candidate, true),
            &firmware,
            &retirement.remaining,
            Some(&health)
        )
        .unwrap()
        .decision(),
        BootDecision::Healthy
    );
    assert!(
        retirement
            .remaining
            .contains_key(&filename(good, Counter::Stable))
    );
    assert!(
        approval_retirement_inventory(&completed, &health, retirement.remaining)
            .unwrap()
            .name
            .is_none()
    );
    let mut tampered = entries;
    tampered
        .get_mut(&filename(&oldest, Counter::Stable))
        .unwrap()
        .push(b' ');
    assert!(approval_retirement_inventory(&completed, &health, tampered).is_err());
    let legacy = health
        .encode()
        .replace("format=2\n", "format=1\n")
        .lines()
        .take(16)
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    assert!(
        HealthIntent::parse(legacy.as_bytes())
            .unwrap()
            .verify(&selected, &publication)
            .is_err()
    );
}
