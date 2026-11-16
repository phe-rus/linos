# Only interactive local/SSH login shells. Noninteractive commands are untouched.
case $- in *i*) ;; *) return 0 ;; esac
[ -t 0 ] && [ -t 1 ] || return 0
[ -r /usr/lib/linos/release ] || return 0
[ -z "${LINOS_NO_WELCOME:-}" ] || return 0
# Home is the native interactive session by default. Users can opt into a
# conventional shell at login without changing their actual Linux login shell.
# A shell launched from inside Home returns to Home when it exits.
_linos_mode=home
if [ -r "$HOME/.config/linos/default-mode" ]; then
    IFS= read -r _linos_mode < "$HOME/.config/linos/default-mode" || :
fi
if [ "$_linos_mode" != shell ] && [ -x /usr/bin/linos ]; then
    # Present the native Linos session without leftover PAM/MOTD text.
    if [ "${TERM:-dumb}" != dumb ]; then
        printf '\033[3J\033[2J\033[H'
    fi
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
unset _linos_mode

# A short Linos shell prompt when the user explicitly returns from the UI.
# Bash only; do not alter other shell or remote command environments.
if [ -n "${BASH_VERSION:-}" ] && [ -z "${LINOS_NO_PROMPT:-}" ]; then
    PS1='\[\e[36m\]\u@\h\[\e[0m\] \w ❯ '
fi

# Clean the presentation when exiting the Bash login shell as well.
if [ -n "${BASH_VERSION:-}" ] && [ -z "${LINOS_NO_WELCOME:-}" ]; then
    trap 'printf "\033[3J\033[2J\033[H"' EXIT
fi
