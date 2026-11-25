#!/usr/bin/env bash
set -Eeuo pipefail
[[ $# == 1 ]] || { echo 'Usage: layout2-preflight.sh STAGED_ROOT' >&2; exit 2; }
STAGED_ROOT="$1"
refuse() { echo 'An enrolled or ambiguous root cannot become a generic deployment.' >&2; exit 2; }
[[ -d "$STAGED_ROOT" && ! -L "$STAGED_ROOT" ]] || refuse
for directory in root home var var/lib var/lib/linos etc etc/linos; do
    if [[ -e "$STAGED_ROOT/$directory" || -L "$STAGED_ROOT/$directory" ]]; then
        [[ -d "$STAGED_ROOT/$directory" && ! -L "$STAGED_ROOT/$directory" ]] || refuse
    fi
done
for marker in var/lib/linos/owner var/lib/linos/pending-owner etc/linos/setup-complete; do
    [[ ! -e "$STAGED_ROOT/$marker" && ! -L "$STAGED_ROOT/$marker" ]] || refuse
done
[[ -d "$STAGED_ROOT/home" && -d "$STAGED_ROOT/root" ]] || refuse
HOME_ENTRY="$(find "$STAGED_ROOT/home" -mindepth 1 -maxdepth 1 -print -quit)" || refuse
[[ -z "$HOME_ENTRY" ]] || refuse
if [[ -e "$STAGED_ROOT/root/.ssh" || -L "$STAGED_ROOT/root/.ssh" ]]; then
    [[ -d "$STAGED_ROOT/root/.ssh" && ! -L "$STAGED_ROOT/root/.ssh" ]] || refuse
    SSH_ENTRY="$(find "$STAGED_ROOT/root/.ssh" -mindepth 1 -maxdepth 1 -print -quit)" || refuse
    [[ -z "$SSH_ENTRY" ]] || refuse
    # rmdir cannot remove a directory containing credentials, including hidden
    # files. Never recursively delete an input SSH directory to pass the guard.
    rmdir -- "$STAGED_ROOT/root/.ssh"
fi
