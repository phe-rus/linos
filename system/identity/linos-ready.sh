#!/bin/sh
# Only the actual Linos kernel exposes this handshake. A compatible fallback
# kernel is never incorrectly reported as a completed Linos boot.
set -eu
state=/sys/kernel/linos/boot_state
test -r "$state"
test -w "$state"
case "$(cat "$state")" in
    starting)
        # Enrollment is complete and a local owner exists before this runs.
        printf 'ready\n' > "$state"
        test "$(cat "$state")" = ready
        ;;
    ready)
        exit 0
        ;;
    *)
        echo "Unexpected Linos kernel boot state" >&2
        exit 1
        ;;
esac
