# lumen: marks for bash. In ~/.bashrc, as its last line:
#     eval "$(lumen shell-integration bash)"
#
# The shell tells the terminal where a prompt begins (A) and ends (B), where
# the output of a command begins (C) and how the command ended (D). These are
# the marks of OSC 133, which other terminals read too.
if [[ $- == *i* && -z ${__lumen_marks-} ]]; then
    __lumen_marks=1

    # First of what runs before a prompt: it has to see how the command ended.
    __lumen_ended() {
        local ended=$?
        printf '\e]133;D;%s\a' "$ended"
        return "$ended"
    }

    # Last of what runs before a prompt: a prompt that a theme has just set
    # anew is marked again.
    __lumen_prompt() {
        local ended=$?
        if [[ $PS1 != *'133;A'* ]]; then
            PS1='\[\e]133;A\a\]'$PS1'\[\e]133;B\a\]'
        fi
        return "$ended"
    }

    # PS0 is printed after a command is read and before it runs.
    PS0='\e]133;C\a'${PS0-}

    if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a'* ]]; then
        PROMPT_COMMAND=(__lumen_ended "${PROMPT_COMMAND[@]}" __lumen_prompt)
    else
        PROMPT_COMMAND="__lumen_ended${PROMPT_COMMAND:+$'\n'$PROMPT_COMMAND}"$'\n'"__lumen_prompt"
    fi
fi
