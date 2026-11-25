#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

fail() { echo "$*" >&2; exit 1; }
field() {
    local key="$1" file="$2" value
    [[ "$(grep -c "^$key=" "$file")" == 1 ]] || fail "Missing or repeated field: $key"
    value="$(sed -n "s/^$key=//p" "$file")"
    printf '%s\n' "$value"
}
identity() {
    arch="$1" sha="$2"
    [[ "$arch" == amd64 || "$arch" == arm64 ]] || fail 'Invalid architecture'
    [[ "$sha" =~ ^[a-f0-9]{40}$ ]] || fail 'Invalid source SHA'
}
# Fixed allowlist, no arbitrary checksum paths or archive members.
files=(boot/kernel boot/initramfs disk.ext4 rootfs.tar kernel-manifest dpkg-status release candidate-manifest kernel-provenance)
verify() (
    cd "$1"
    identity "$2" "$3"
    for file in "${files[@]}" SHA256SUMS; do
        [[ -s "$file" && ! -L "$file" ]] || fail "Missing or linked candidate file: $file"
    done
    [[ -d boot && ! -L boot ]] || fail 'Invalid boot directory'
    [[ "$(wc -l < SHA256SUMS)" == "${#files[@]}" ]] || fail 'Incomplete checksum manifest'
    for file in "${files[@]}"; do
        awk -v name="$file" 'length($1) == 64 && $1 !~ /[^a-f0-9]/ && substr($0,65) == "  " name { n++ } END { exit n != 1 }' SHA256SUMS || fail "Missing checksum: $file"
    done
    if [[ "${4:-all}" == payload ]]; then
        # Payloads use the original rootfs and boot files, never the test disk.
        for file in "${files[@]}"; do
            [[ "$file" == disk.ext4 ]] && continue
            grep -Fxq "$(sha256sum "$file")" SHA256SUMS || fail "Changed payload input: $file"
        done
    else
        sha256sum --strict --check SHA256SUMS
    fi
    [[ "$(field format candidate-manifest)" == 1 && "$(field edition candidate-manifest)" == server &&
       "$(field architecture candidate-manifest)" == "$arch" && "$(field source_sha candidate-manifest)" == "$sha" &&
       "$(field state candidate-manifest)" == development ]] || fail 'Candidate identity mismatch'
    build="$(field build_id candidate-manifest)"
    [[ "$build" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || fail 'Invalid build ID'
    grep -Fxq "BUILD_ID=\"$build\"" release || fail 'Release metadata mismatch'
    for file in kernel initramfs; do
        [[ "$(field "${file}_sha256" kernel-manifest)" == "$(sha256sum "boot/$file" | cut -d' ' -f1)" ]] || fail 'Boot hash mismatch'
    done
)

case "${1:-}" in
    pack)
        # Run immediately after assembly, before any QEMU test changes disk.ext4.
        image="${2:?image directory}"; identity "${3:?architecture}" "$(git -C "$ROOT" rev-parse HEAD)"
        out="${4:?new artifact directory}"
        [[ ! -e "$out" ]] || fail 'Artifact directory already exists'
        mkdir -p "$out"
        out="$(realpath "$out")"
        cp "$ROOT/system/release/usr/lib/linos/release" "$image/release"
        tar -xOf "$image/rootfs.tar" ./var/lib/dpkg/status > "$image/dpkg-status"
        mode="$(field mode "$image/kernel-manifest")"
        kernel_run=none kernel_sha=none package_hash=none
        if [[ "$mode" == linos ]]; then
            kernel_run="${LINOS_KERNEL_RUN_ID:?verified kernel run}"
            kernel_sha="${LINOS_KERNEL_SOURCE_SHA:?kernel source SHA}"
            [[ "$kernel_run" =~ ^[1-9][0-9]*$ && "$kernel_sha" =~ ^[a-f0-9]{40}$ ]] || fail 'Invalid kernel identity'
            cp "${LINOS_KERNEL_PROVENANCE:?kernel provenance}" "$image/kernel-provenance"
            package_hash="$(sha256sum "${LINOS_KERNEL_PACKAGE:?kernel package}" | cut -d' ' -f1)"
        elif [[ "$mode" == compat ]]; then
            printf 'mode=compat\nsource=dpkg-status\n' > "$image/kernel-provenance"
        else
            fail 'Unsupported candidate kernel mode'
        fi
        {
            printf 'format=1\nedition=server\nstate=development\nsource_sha=%s\narchitecture=%s\n' "$sha" "$arch"
            printf 'build_id=%s\n' "$(sed -n 's/^BUILD_ID="\([^"]*\)"$/\1/p' "$image/release")"
            printf 'run_id=%s\nrun_attempt=%s\n' "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-1}"
            if [[ -z "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ]]; then
                printf 'source_dirty=false\n'
            else
                printf 'source_dirty=true\n'
            fi
            printf 'kernel_run_id=%s\nkernel_source_sha=%s\nkernel_package_sha256=%s\n' "$kernel_run" "$kernel_sha" "$package_hash"
        } > "$image/candidate-manifest"
        (cd "$image"; sha256sum "${files[@]}" > SHA256SUMS)
        verify "$image" "$arch" "$sha"
        tar --sparse -C "$image" -czf "$out/candidate.tar.gz" "${files[@]}" SHA256SUMS
        (cd "$out"; sha256sum candidate.tar.gz > candidate.tar.gz.sha256)
        ;;
    restore)
        bundle="$(realpath "${2:?bundle directory}")"; identity "${3:?architecture}" "${4:?source SHA}"
        destination="${5:?new image directory}"
        [[ ! -e "$destination" ]] || fail 'Refusing to overwrite existing candidate'
        (cd "$bundle"; [[ "$(wc -l < candidate.tar.gz.sha256)" == 1 ]];
            grep -Eq '^[a-f0-9]{64}  candidate\.tar\.gz$' candidate.tar.gz.sha256;
            sha256sum --strict --check candidate.tar.gz.sha256)
        # Inspect before extraction. Only regular files from the fixed list are accepted.
        members="$(tar -tzf "$bundle/candidate.tar.gz")"
        expected="$(printf '%s\n' "${files[@]}" SHA256SUMS | sort)"
        [[ "$(sort <<< "$members")" == "$expected" ]] || fail 'Unexpected archive members'
        if tar -tvzf "$bundle/candidate.tar.gz" | awk 'substr($0,1,1) != "-" { bad=1 } END { exit !bad }'; then
            fail 'Candidate archive contains nonregular files'
        fi
        mkdir -p "$(dirname "$destination")"
        work="$(mktemp -d "$(dirname "$destination")/.linos-restore.XXXXXXXX")"
        trap 'rm -rf -- "$work"' EXIT
        tar --extract --gzip --sparse --no-same-owner --no-same-permissions -f "$bundle/candidate.tar.gz" -C "$work"
        verify "$work" "$arch" "$sha"
        mv "$work" "$destination"
        ;;
    verify) verify "${2:?image directory}" "${3:?architecture}" "${4:?source SHA}" ;;
    verify-payload) verify "${2:?image directory}" "${3:?architecture}" "${4:?source SHA}" payload ;;
    download-development)
        run="${2:?run ID}"; identity "${3:?architecture}" "${4:?source SHA}"
        out="${5:?new download directory}"
        bash "$ROOT/build/fetch-artifact.sh" "$run" "$sha" "server-candidate-$arch" "$out" development
        ;;
    *) echo 'Usage: candidate.sh pack IMAGE ARCH OUTPUT | restore BUNDLE ARCH SHA IMAGE | verify IMAGE ARCH SHA | download-development RUN ARCH SHA OUTPUT' >&2; exit 2 ;;
esac
