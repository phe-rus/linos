#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
mkdir -p target/candidate-tests
work="$(mktemp -d "$ROOT/target/candidate-tests/run.XXXXXXXX")"
trap 'rm -rf -- "$work"' EXIT
sha="$(git rev-parse HEAD)"
mkdir -p "$work/image/boot" "$work/rootfs/var/lib/dpkg"
printf 'Package: fixture\nVersion: 1\nArchitecture: amd64\nStatus: install ok installed\n' > "$work/rootfs/var/lib/dpkg/status"
tar -C "$work/rootfs" -cf "$work/image/rootfs.tar" ./var
printf kernel > "$work/image/boot/kernel"
printf initramfs > "$work/image/boot/initramfs"
truncate -s 4M "$work/image/disk.ext4"
printf disk | dd of="$work/image/disk.ext4" conv=notrunc status=none
{
    printf 'mode=compat\nrelease=fixture\n'
    printf 'kernel_sha256=%s\n' "$(sha256sum "$work/image/boot/kernel" | cut -d' ' -f1)"
    printf 'initramfs_sha256=%s\n' "$(sha256sum "$work/image/boot/initramfs" | cut -d' ' -f1)"
} > "$work/image/kernel-manifest"
reject() {
    local label="$1"; shift
    if "$@" > "$work/rejection.log" 2>&1; then
        echo "FAIL: accepted $label" >&2; exit 1
    fi
    cp "$work/rejection.log" "$ROOT/target/candidate-tests/$label.log"
    echo "PASS: rejected $label"
}
bash build/candidate.sh pack "$work/image" amd64 "$work/bundle"
# Mutating a test guest must not change the captured candidate.
printf changed >> "$work/image/disk.ext4"
bash build/candidate.sh verify-payload "$work/image" amd64 "$sha"
reject changed-test-disk bash build/candidate.sh verify "$work/image" amd64 "$sha"
bash build/candidate.sh restore "$work/bundle" amd64 "$sha" "$work/restored"
[[ "$(stat -c %s "$work/restored/disk.ext4")" == 4194304 ]]
cmp "$work/rootfs/var/lib/dpkg/status" "$work/restored/dpkg-status"
reject wrong-architecture bash build/candidate.sh verify "$work/restored" arm64 "$sha"
reject wrong-source bash build/candidate.sh verify "$work/restored" amd64 0000000000000000000000000000000000000000
reject existing-image bash build/candidate.sh restore "$work/bundle" amd64 "$sha" "$work/restored"
printf tampered >> "$work/restored/boot/kernel"
reject altered-boot bash build/candidate.sh verify "$work/restored" amd64 "$sha"
reject altered-payload bash build/candidate.sh verify-payload "$work/restored" amd64 "$sha"
# Valid archive checksum must not authorize extra paths or symbolic links.
cp -a "$work/bundle" "$work/unsafe"
printf extra > "$work/image/extra"
tar -C "$work/image" -czf "$work/unsafe/candidate.tar.gz" extra
(cd "$work/unsafe"; sha256sum candidate.tar.gz > candidate.tar.gz.sha256)
reject extra-members bash build/candidate.sh restore "$work/unsafe" amd64 "$sha" "$work/no-extra"
rm "$work/image/boot/kernel"
ln -s /etc/passwd "$work/image/boot/kernel"
tar --sparse -C "$work/image" -czf "$work/unsafe/candidate.tar.gz" \
    boot/kernel boot/initramfs disk.ext4 rootfs.tar kernel-manifest dpkg-status release candidate-manifest kernel-provenance SHA256SUMS
(cd "$work/unsafe"; sha256sum candidate.tar.gz > candidate.tar.gz.sha256)
reject linked-members bash build/candidate.sh restore "$work/unsafe" amd64 "$sha" "$work/no-links"
printf corrupt >> "$work/bundle/candidate.tar.gz"
reject altered-archive bash build/candidate.sh restore "$work/bundle" amd64 "$sha" "$work/no-corrupt"
echo 'PASS: original candidate survives guest mutation and restores with package inventory'

# Exercise API rejection paths without network access or real credentials.
mkdir "$work/bin"
cat > "$work/bin/gh" <<'MOCK'
#!/usr/bin/env bash
case "$*" in
    *'/artifacts/9/zip'*) printf changed ;;
    *'/artifacts'*) printf '9\t%s\tsha256:%064d\n' "${MOCK_EXPIRED:-false}" 0 ;;
    *'/jobs'*)
        printf 'boot / QEMU amd64\t%s\nboot / QEMU arm64\tsuccess\n' "${MOCK_BOOT:-success}" ;;
    *'/actions/runs/'*)
        printf '%s\tcompleted\t%s\t.github/workflows/kernel.yml\tmain\tpush\n' "$MOCK_SHA" "${MOCK_CONCLUSION:-success}" ;;
    *) exit 2 ;;
esac
MOCK
chmod +x "$work/bin/gh"
export PATH="$work/bin:$PATH" MOCK_SHA="$sha"
reject kernel-source env MOCK_SHA=0000000000000000000000000000000000000000 bash build/fetch-artifact.sh 1 "$sha" linos-kernel-amd64 "$work/download" kernel
reject kernel-failed env MOCK_CONCLUSION=failure bash build/fetch-artifact.sh 1 "$sha" linos-kernel-amd64 "$work/download" kernel
reject kernel-unqualified env MOCK_BOOT=skipped bash build/fetch-artifact.sh 1 "$sha" linos-kernel-amd64 "$work/download" kernel
reject kernel-expired env MOCK_EXPIRED=true bash build/fetch-artifact.sh 1 "$sha" linos-kernel-amd64 "$work/download" kernel
reject artifact-digest bash build/fetch-artifact.sh 1 "$sha" linos-kernel-amd64 "$work/download" kernel
