# Linos 13 interactive login shell greeting. No prompt or terminal ownership changes.
case $- in *i*) ;; *) return 0 ;; esac
[ -t 0 ] && [ -t 1 ] || return 0
[ -r /usr/lib/linos/release ] || return 0
[ -z "${LINOS_NO_WELCOME:-}" ] || return 0
_linos_color=""
_linos_reset=""
if [ -z "${NO_COLOR+x}" ] && [ "${TERM:-dumb}" != dumb ]; then
  _linos_color="$(printf '\033[36m')"
  _linos_reset="$(printf '\033[0m')"
fi
printf '\n  %s◆%s  Linos 13 Server\n' "$_linos_color" "$_linos_reset"
printf '  │  Welcome back, %s.\n' "${USER:-user}"
printf '  └─ Run linos to manage this server.\n\n'
unset _linos_color _linos_reset
