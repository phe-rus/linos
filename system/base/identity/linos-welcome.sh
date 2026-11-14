# Only interactive local/SSH login shells. Noninteractive commands are untouched.
case $- in *i*) ;; *) return 0 ;; esac
[ -t 0 ] && [ -t 1 ] || return 0
[ -r /usr/lib/linos/release ] || return 0
[ -z "${LINOS_NO_WELCOME:-}" ] || return 0
# In interactive sign-ins, enter the Linos home. It returns to the ordinary
# shell on /shell or Ctrl+D; third-party TUIs are never wrapped.
if [ -x /usr/bin/linos ]; then
    /usr/bin/linos
    _linos_result=$?
    if [ "$_linos_result" -eq 20 ]; then
        # Exit only the current interactive login shell. The system login
        # service will show a clean, fresh authentication prompt.
        printf '\033[3J\033[2J\033[H'
        exit 0
    fi
    unset _linos_result
fi

# A short Linos shell prompt when the user explicitly returns from the UI.
# Bash only; do not alter other shell or remote command environments.
if [ -n "${BASH_VERSION:-}" ] && [ -z "${LINOS_NO_PROMPT:-}" ]; then
    PS1='\[\e[36m\]\u@\h\[\e[0m\] \w ❯ '
fi

# Clean the presentation when exiting the Bash login shell as well.
if [ -n "${BASH_VERSION:-}" ] && [ -z "${LINOS_NO_WELCOME:-}" ]; then
    trap 'printf "\033[3J\033[2J\033[H"' EXIT
fi
