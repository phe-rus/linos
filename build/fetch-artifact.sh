#!/usr/bin/env bash
set -Eeuo pipefail

# Fetch from the owner repository, binding the bytes to a run and source commit.
# A checksum inside an artifact alone cannot establish where it came from.
run="${1:?run ID}" source_sha="${2:?source SHA}" name="${3:?artifact name}"
destination="${4:?new destination}" policy="${5:?kernel or development}"
[[ "$run" =~ ^[1-9][0-9]*$ && "$source_sha" =~ ^[a-f0-9]{40}$ ]] || exit 2
[[ "$name" =~ ^[a-zA-Z0-9._-]+$ && ! -e "$destination" ]] || exit 2
repo=phe-rus/linos
metadata="$(gh api "repos/$repo/actions/runs/$run" \
    --jq '[.head_sha,.status,.conclusion,.path,.head_branch,.event] | @tsv')"
IFS=$'\t' read -r actual_sha status conclusion workflow branch event <<< "$metadata"
[[ "$actual_sha" == "$source_sha" && "$status" == completed && "$branch" == main ]] || {
    echo 'Artifact run source, completion or branch mismatch' >&2; exit 1;
}
case "$policy" in
    kernel)
        [[ "$workflow" == .github/workflows/kernel.yml && "$conclusion" == success && "$event" == push ]] || {
            echo 'Kernel reuse requires a successful kernel workflow push on main' >&2; exit 1;
        }
        jobs="$(gh api --paginate "repos/$repo/actions/runs/$run/jobs" \
            --jq '.jobs[] | [.name,.conclusion] | @tsv')"
        for arch in amd64 arm64; do
            grep -Fxq "$(printf 'boot / QEMU %s\tsuccess' "$arch")" <<< "$jobs" || {
                echo "Missing successful kernel boot qualification: $arch" >&2; exit 1;
            }
        done
        ;;
    development)
        [[ "$workflow" == .github/workflows/ci.yml || "$workflow" == .github/workflows/kernel.yml ]] || exit 1
        echo "Development artifact, run conclusion: $conclusion. This is not release qualification." >&2
        ;;
    *) exit 2 ;;
esac
artifact="$(gh api --paginate "repos/$repo/actions/runs/$run/artifacts" \
    --jq ".artifacts[] | select(.name == \"$name\") | [.id,.expired,.digest] | @tsv")"
[[ "$(wc -l <<< "$artifact")" == 1 ]] || exit 1
IFS=$'\t' read -r artifact_id expired digest <<< "$artifact"
[[ "$artifact_id" =~ ^[1-9][0-9]*$ && "$expired" == false && "$digest" =~ ^sha256:[a-f0-9]{64}$ ]] || {
    echo 'Artifact missing, expired, duplicated or without a SHA256 digest' >&2; exit 1;
}
mkdir -p "$(dirname "$destination")"
work="$(mktemp -d "$(dirname "$destination")/.linos-download.XXXXXXXX")"
trap 'rm -rf -- "$work"' EXIT
gh api "repos/$repo/actions/artifacts/$artifact_id/zip" > "$work/artifact.zip"
printf '%s  %s\n' "${digest#sha256:}" "$work/artifact.zip" | sha256sum --check
mkdir "$work/files"
unzip -q "$work/artifact.zip" -d "$work/files"
mv "$work/files" "$destination"
printf 'Verified %s, run %s, source %s, artifact %s, %s\n' "$name" "$run" "$source_sha" "$artifact_id" "$digest"
