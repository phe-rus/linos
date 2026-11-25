#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$ROOT/target/layout2-preflight-tests"
WORK="$(mktemp -d "$ROOT/target/layout2-preflight-tests/case.XXXXXXXX")"
trap 'rm -rf -- "$WORK"' EXIT
fresh() {
    rm -rf -- "$WORK/root"
    mkdir -p "$WORK/root/var/lib/linos" "$WORK/root/etc/linos" "$WORK/root/root" "$WORK/root/home"
}
reject() {
    if bash "$ROOT/build/layout2-preflight.sh" "$WORK/root" >/dev/null 2>&1; then
        echo "Generic preflight accepted unsafe fixture: $1" >&2
        exit 1
    fi
}
fresh
bash "$ROOT/build/layout2-preflight.sh" "$WORK/root"
mkdir "$WORK/root/root/.ssh"
bash "$ROOT/build/layout2-preflight.sh" "$WORK/root"
[[ ! -e "$WORK/root/root/.ssh" ]]
for marker in var/lib/linos/owner var/lib/linos/pending-owner etc/linos/setup-complete; do
    fresh
    touch "$WORK/root/$marker"
    reject "$marker"
    rm -- "$WORK/root/$marker"
    ln -s /nonexistent "$WORK/root/$marker"
    reject "linked $marker"
done
fresh
mkdir "$WORK/root/root/.ssh"
printf 'synthetic test key\n' > "$WORK/root/root/.ssh/.private"
reject 'hidden SSH key'
[[ -s "$WORK/root/root/.ssh/.private" ]]
fresh
mkdir "$WORK/empty-ssh"
ln -s "$WORK/empty-ssh" "$WORK/root/root/.ssh"
reject 'linked SSH directory'
fresh
mkfifo "$WORK/root/root/.ssh"
reject 'special SSH object'
fresh
mkdir "$WORK/root/home/owner"
reject 'owner home'
fresh
rm -d "$WORK/root/home"
ln -s "$WORK/empty-ssh" "$WORK/root/home"
reject 'linked home'
fresh
rm -d "$WORK/root/root"
ln -s "$WORK/empty-ssh" "$WORK/root/root"
reject 'linked root parent'
fresh
rm -d "$WORK/root/etc/linos"
ln -s "$WORK/empty-ssh" "$WORK/root/etc/linos"
reject 'linked enrollment parent'
echo 'PASS: generic preflight permits only empty SSH directories and refuses enrolled material'
