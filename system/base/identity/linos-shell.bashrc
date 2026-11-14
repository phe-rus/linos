# Interactive Bash configuration for the optional Linos Shell mode.
# Preserve the user's existing Bash configuration and command semantics.
if [ -r "$HOME/.bashrc" ]; then
    . "$HOME/.bashrc"
fi

# Return to the still-running Linos Home session with Ctrl+T.
# Bash readline owns the key only at its prompt. Child programs such as Vim,
# SSH, sudo and tmux retain their own terminal input.
if [[ $- == *i* ]]; then
    bind -x '"\\C-t": "exit"'
    PS1='\\[\\e[36m\\]linos\\[\\e[0m\\] \\w ❯ '
fi
