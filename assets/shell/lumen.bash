# lumen: marks for bash. In ~/.bashrc, as its last line:
#     eval "$(lumen shell-integration bash)"
#
# The shell tells the terminal where a prompt begins (A) and ends (B), where
# the output of a command begins (C) and how the command ended (D). These are
# the marks of OSC 133, which other terminals read too.
if [[ $- == *i* ]]; then
    # Last of what runs before a prompt, so that a prompt a theme has just set
    # anew is marked again, and so that nothing of lumen's runs before a
    # theme's own commands: they find $? and PIPESTATUS as the command left them.
    __lumen_prompt() {
        local esc=$'\e' bel=$'\a'
        # How the command ended is said in the prompt itself: when bash writes
        # it, $? is the command's again, whatever ran in between.
        local ended="\\[\${__lumen_ran:+${esc}]133;D;\$?${bel}}${esc}]133;A${bel}\\]"
        local plain="\\[${esc}]133;A${bel}\\]" after="\\[${esc}]133;B${bel}\\]"
        local before=$ended
        if ! shopt -q promptvars; then
            # A prompt that takes no variables can't say $?. That the command
            # ended is told here, and how it ended stays unknown.
            before=$plain
            [[ -n ${__lumen_first-} ]] && printf '\e]133;D\a' >&2
        fi
        # there is no command to tell of before a shell's first prompt
        if [[ -n ${__lumen_first-} ]]; then
            __lumen_ran=1
        else
            __lumen_first=1
        fi
        # The prompt begins on a line of its own (L), also after output that
        # ended without a line break. bash itself doesn't see to that.
        printf '\e]133;L\a' >&2
        # What was put in front of a marked prompt stays in front: whoever put
        # it there may take it off again. But not what has a $ or a ` in it:
        # that may run a command when the prompt is written, and after it $?
        # is that command's.
        local front=${PS1-}
        front=${front%%"$before"*}
        if [[ ${PS1-} != *"$before"*"$after" || $front == *'$'* || $front == *'`'* ]]; then
            # A prompt that was set anew, or added to, since it was last marked.
            local inner=${PS1-}
            inner=${inner//"$ended"/}
            inner=${inner//"$plain"/}
            PS1=$before${inner//"$after"/}$after
        fi
        # A further line of a command begins with PS2. It is marked as a prompt
        # (P), so that lumen does not take it for a part of the command. Only
        # where TERM_PROGRAM says lumen: the B that ends it is where a command
        # begins to other terminals.
        if [[ ${TERM_PROGRAM-} == lumen && -n ${PS2-} && $PS2 != *'133;P'* ]]; then
            PS2="\\[${esc}]133;P;k=c${bel}\\]${PS2}${after}"
        fi
    }

    # These lines may be read more than once, as when ~/.bashrc is read again:
    # what is still in place is left, and what the file took out is put back.

    # PS0 is printed after a command is read and before it runs.
    [[ ${PS0-} == *'133;C'* ]] || PS0='\e]133;C\a'${PS0-}

    if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a'* ]]; then
        # one by one: joined, they are apart by whatever IFS begins with
        __lumen_there=
        for __lumen_one in "${PROMPT_COMMAND[@]}"; do
            [[ $__lumen_one == *__lumen_prompt* ]] && __lumen_there=1
        done
        [[ -n $__lumen_there ]] || PROMPT_COMMAND+=(__lumen_prompt)
        unset __lumen_there __lumen_one
    elif [[ ${PROMPT_COMMAND-} != *__lumen_prompt* ]]; then
        PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND
}__lumen_prompt"
    fi
fi
