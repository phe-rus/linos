use super::*;

pub(super) fn manifest() -> String {
    let mut text = format!(
        "kind=linos-deployment\nformat=3\norigin=ci\nbase_manifest_sha256=none\nplan_sha256=none\nsource_sha={}\nbuild_id=20261124.1\nrelease=9\narchitecture=amd64\nhardware_profile=qemu-amd64-uefi\nkernel_release=6.12.58-linos\nconfig_schema_min=1\nconfig_schema_max=2\n",
        "a".repeat(40)
    );
    for (index, name) in [
        "system",
        "kernel",
        "initramfs",
        "inventory",
        "applications",
        "services",
        "accounts",
        "extensions",
    ]
    .iter()
    .enumerate()
    {
        text.push_str(&format!(
            "{name}_bytes={}\n{name}_sha256={}\n",
            4096 + index,
            (index + 1).to_string().repeat(64)
        ));
    }
    text
}

pub(super) fn field(text: &str, key: &str, value: &str) -> String {
    text.lines()
        .map(|line| {
            if line.split_once('=').unwrap().0 == key {
                format!("{key}={value}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

fn parse_field(key: &str, value: &str) -> io::Result<Deployment> {
    Deployment::parse(field(&manifest(), key, value).as_bytes())
}

fn openssl(args: &[&str]) -> Vec<u8> {
    let output = Command::new("/usr/bin/openssl")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

pub(super) struct Signer {
    private: File,
    public: Vec<u8>,
}

impl Signer {
    fn new(algorithm: &[&str]) -> Self {
        let private = snapshot(&openssl(algorithm)).unwrap();
        let public = openssl(&["pkey", "-pubout", "-in", &descriptor_path(&private)]);
        Self { private, public }
    }

    pub(super) fn ed25519() -> Self {
        Self::new(&["genpkey", "-algorithm", "ED25519"])
    }

    fn sign(&self, domain: &[u8], sums: &[u8]) -> Vec<u8> {
        let input = snapshot(&[domain, sums].concat()).unwrap();
        openssl(&[
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            &descriptor_path(&self.private),
            "-in",
            &descriptor_path(&input),
        ])
    }

    pub(super) fn verify(&self, manifest: &str, sums: &[u8]) -> io::Result<OfficialManifest> {
        OfficialManifest::authenticate(
            manifest.as_bytes(),
            sums,
            &self.sign(DOMAIN, sums),
            &self.public,
        )
    }
}

pub(super) fn sums(text: &str) -> Vec<u8> {
    let parsed = Deployment::parse(text.as_bytes()).unwrap();
    let mut sums = format!("{}  manifest\n", sha256(text.as_bytes()).unwrap());
    for (name, artifact) in ARTIFACT_NAMES.iter().zip(parsed.artifacts) {
        sums.push_str(&format!("{}  {name}\n", artifact.sha256));
    }
    sums.into_bytes()
}

#[test]
fn parses_exact_identity_provenance_target_and_artifact_order() {
    let parsed = Deployment::parse(manifest().as_bytes()).unwrap();
    assert_eq!(parsed.origin, Origin::Ci);
    assert_eq!(parsed.source_sha, "a".repeat(40));
    assert_eq!(parsed.build_id, "20261124.1");
    assert_eq!(parsed.release, 9);
    assert_eq!((parsed.config_schema_min, parsed.config_schema_max), (1, 2));
    for (index, artifact) in parsed.artifacts.iter().enumerate() {
        assert_eq!(artifact.bytes, 4096 + index as u64);
        assert_eq!(artifact.sha256, (index + 1).to_string().repeat(64));
    }
    parsed
        .require_target("amd64", "qemu-amd64-uefi", "6.12.58-linos")
        .unwrap();
    for (arch, profile, kernel) in [
        ("arm64", "qemu-amd64-uefi", "6.12.58-linos"),
        ("amd64", "other", "6.12.58-linos"),
        ("amd64", "qemu-amd64-uefi", "other"),
    ] {
        assert!(parsed.require_target(arch, profile, kernel).is_err());
    }
    assert!(parse_field("architecture", "arm64").is_ok());
    assert!(parse_field("architecture", "riscv64").is_err());
}

#[test]
fn rejects_noncanonical_and_overflowing_numbers_before_use() {
    for key in [
        "release",
        "config_schema_min",
        "config_schema_max",
        "system_bytes",
        "kernel_bytes",
        "initramfs_bytes",
        "inventory_bytes",
        "applications_bytes",
        "services_bytes",
        "accounts_bytes",
        "extensions_bytes",
    ] {
        for value in [
            "",
            "01",
            "+1",
            "-1",
            " 1",
            "1 ",
            "1.0",
            "18446744073709551616",
        ] {
            assert!(parse_field(key, value).is_err(), "{key}={value}");
        }
    }
    assert!(parse_field("release", "0").is_err());
    assert!(parse_field("release", "18446744073709551615").is_ok());
    assert!(parse_field("config_schema_max", "4294967296").is_err());
    assert!(parse_field("config_schema_min", "3").is_err());
    assert!(parse_field("config_schema_min", "0").is_ok());
    assert!(parse_field("config_schema_max", "4294967295").is_ok());
}

#[test]
fn enforces_each_artifact_size_and_hash_bound() {
    for (name, limit) in [
        "system",
        "kernel",
        "initramfs",
        "inventory",
        "applications",
        "services",
        "accounts",
        "extensions",
    ]
    .into_iter()
    .zip([
        4096 * MIB,
        128 * MIB,
        128 * MIB,
        16 * MIB,
        16 * MIB,
        16 * MIB,
        16 * MIB,
        16 * MIB,
    ]) {
        let key = format!("{name}_bytes");
        assert!(parse_field(&key, "0").is_err());
        assert!(parse_field(&key, &limit.to_string()).is_ok());
        assert!(parse_field(&key, &(limit + 1).to_string()).is_err());
        for hash in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
            assert!(parse_field(&format!("{name}_sha256"), &hash).is_err());
        }
    }
}

#[test]
fn rejects_schema_changes_and_malformed_identity_fields() {
    let text = manifest();
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    for index in 0..lines.len() {
        let mut missing = lines.clone();
        missing.remove(index);
        assert!(Deployment::parse(missing.concat().as_bytes()).is_err());
        let mut duplicate = lines.clone();
        duplicate.insert(index, lines[index]);
        assert!(Deployment::parse(duplicate.concat().as_bytes()).is_err());
        if index + 1 < lines.len() {
            let mut swapped = lines.clone();
            swapped.swap(index, index + 1);
            assert!(Deployment::parse(swapped.concat().as_bytes()).is_err());
        }
    }
    for bad in [
        format!("{text}unknown=value\n"),
        text.replace('\n', "\r\n"),
        text.trim_end().to_owned(),
        format!("{text}\n"),
    ] {
        assert!(Deployment::parse(bad.as_bytes()).is_err());
    }
    for (key, value) in [
        ("kind", "linos-recovery"),
        ("format", "2"),
        ("format", "03"),
        ("source_sha", &"A".repeat(40)),
        ("source_sha", &"a".repeat(39)),
        ("build_id", "../../escape"),
        ("hardware_profile", "bad profile"),
        ("kernel_release", "kernel\0"),
    ] {
        assert!(parse_field(key, value).is_err());
    }
}

#[test]
fn requires_both_local_provenance_links_and_no_ci_links() {
    assert!(parse_field("origin", "local").is_err());
    assert!(parse_field("origin", "unknown").is_err());
    assert!(parse_field("base_manifest_sha256", &"a".repeat(64)).is_err());
    assert!(parse_field("plan_sha256", &"b".repeat(64)).is_err());
    let local = field(
        &field(
            &field(&manifest(), "origin", "local"),
            "base_manifest_sha256",
            &"a".repeat(64),
        ),
        "plan_sha256",
        &"b".repeat(64),
    );
    assert_eq!(
        Deployment::parse(local.as_bytes()).unwrap().origin,
        Origin::Local {
            base_manifest_sha256: "a".repeat(64),
            plan_sha256: "b".repeat(64)
        }
    );
    let signer = Signer::ed25519();
    // Even an authentic CI signature does not grant the device composition role.
    assert!(signer.verify(&local, &sums(&local)).is_err());
}

#[test]
fn authenticates_exact_manifest_identity_and_signed_digests() {
    assert_eq!(
        sha256(b"abc").unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let signer = Signer::ed25519();
    let text = manifest();
    let result = signer.verify(&text, &sums(&text)).unwrap();
    assert_eq!(result.id(), sha256(text.as_bytes()).unwrap());
    assert_eq!(
        result.deployment(),
        &Deployment::parse(text.as_bytes()).unwrap()
    );
}

#[test]
fn refuses_other_signing_domains_and_unprefixed_legacy_signatures() {
    let signer = Signer::ed25519();
    let text = manifest();
    let sums = sums(&text);
    for domain in [
        b"LINOS-RECOVERY-V1\n".as_slice(),
        b"LINOS-OFFLINE-V3\n",
        b"",
        b"LINOS-DEPLOYMENT-V3\r\n",
        b"LINOS-DEPLOYMENT-V2\n",
    ] {
        let signature = signer.sign(domain, &sums);
        assert!(
            OfficialManifest::authenticate(text.as_bytes(), &sums, &signature, &signer.public)
                .is_err()
        );
    }
}

#[test]
fn refuses_tampering_wrong_keys_wrong_algorithms_and_signature_lengths() {
    let signer = Signer::ed25519();
    let other = Signer::ed25519();
    let text = manifest();
    let sums = sums(&text);
    let signature = signer.sign(DOMAIN, &sums);
    assert!(
        OfficialManifest::authenticate(
            field(&text, "release", "10").as_bytes(),
            &sums,
            &signature,
            &signer.public
        )
        .is_err()
    );
    let mut corrupt_sums = sums.clone();
    corrupt_sums[0] = b'f';
    assert!(
        OfficialManifest::authenticate(text.as_bytes(), &corrupt_sums, &signature, &signer.public)
            .is_err()
    );
    let mut corrupt_sig = signature.clone();
    corrupt_sig[0] ^= 1;
    assert!(
        OfficialManifest::authenticate(text.as_bytes(), &sums, &corrupt_sig, &signer.public)
            .is_err()
    );
    assert!(
        OfficialManifest::authenticate(text.as_bytes(), &sums, &signature, &other.public).is_err()
    );
    for signature in [&signature[..63], &[0u8; 65][..]] {
        assert!(
            OfficialManifest::authenticate(text.as_bytes(), &sums, signature, &signer.public)
                .is_err()
        );
    }
    let ec = Signer::new(&[
        "genpkey",
        "-algorithm",
        "EC",
        "-pkeyopt",
        "ec_paramgen_curve:P-256",
    ]);
    assert!(
        OfficialManifest::authenticate(text.as_bytes(), &sums, &signature, &ec.public).is_err()
    );
    assert!(
        OfficialManifest::authenticate(text.as_bytes(), &sums, &signature, b"invalid public key")
            .is_err()
    );
}

#[test]
fn refuses_even_signed_checksum_reordering_aliases_and_digest_disagreement() {
    let signer = Signer::ed25519();
    let text = manifest();
    let sums = String::from_utf8(sums(&text)).unwrap();
    let lines: Vec<_> = sums.split_inclusive('\n').collect();
    let mut reordered = lines.clone();
    reordered.swap(0, 1);
    let mut missing = lines.clone();
    missing.remove(1);
    let bad = [
        reordered.concat(),
        missing.concat(),
        format!("{sums}{}", lines[0]),
        sums.replace("  system.img\n", "  ../system.img\n"),
        sums.replace("  kernel\n", "  ./kernel\n"),
        sums.replace("  manifest\n", " *manifest\n"),
        sums.replace('\n', "\r\n"),
        sums.trim_end().to_owned(),
        sums.replacen(&"1".repeat(64), &"a".repeat(64), 1),
    ];
    for sums in bad {
        assert!(signer.verify(&text, sums.as_bytes()).is_err());
    }
}

#[test]
fn rejects_oversized_inputs_and_seals_verification_snapshots() {
    let signer = Signer::ed25519();
    let text = manifest();
    let sums = sums(&text);
    let sig = signer.sign(DOMAIN, &sums);
    assert!(
        OfficialManifest::authenticate(
            &vec![b'x'; MAX_RECORD_BYTES + 1],
            &sums,
            &sig,
            &signer.public
        )
        .is_err()
    );
    assert!(
        OfficialManifest::authenticate(
            text.as_bytes(),
            &vec![b'x'; MAX_SUMS_BYTES + 1],
            &sig,
            &signer.public
        )
        .is_err()
    );
    assert!(
        OfficialManifest::authenticate(
            text.as_bytes(),
            &sums,
            &sig,
            &vec![b'x'; MAX_KEY_BYTES + 1]
        )
        .is_err()
    );
    let mut file = snapshot(b"immutable verification input").unwrap();
    assert!(file.write_all(b"mutation").is_err());
    assert!(file.set_len(0).is_err());
}
