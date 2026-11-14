# Only interactive local/SSH login shells. No shell prompt modifications.
case $- in *i*) ;; *) return 0 ;; esac
[ -t 0 ] && [ -t 1 ] || return 0
[ -r /usr/lib/linos/release ] || return 0
[ -z "${LINOS_NO_WELCOME:-}" ] || return 0
# In interactive sign-ins, enter the Linos home. It returns to the ordinary
# shell on /shell or Ctrl+D; third-party TUIs are never wrapped.
if [ -x /usr/bin/linos ]; then
    /usr/bin/linos
fi
