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
        if [[ ${PS1-} != *"$before"*"$after" ]]; then
            # A prompt that was set anew, or added to at its end, since it was
            # last marked. What was put in front of it stays in front: whoever
            # put it there may take it off again.
            local inner=${PS1-}
            inner=${inner//"$ended"/}
            inner=${inner//"$plain"/}
            PS1=$before${inner//"$after"/}$after
        fi
    }

    # These lines may be read more than once, as when ~/.bashrc is read again:
    # what is still in place is left, and what the file took out is put back.

    # PS0 is printed after a command is read and before it runs.
    [[ ${PS0-} == *'133;C'* ]] || PS0='\e]133;C\a'${PS0-}

    if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a'* ]]; then
        [[ " ${PROMPT_COMMAND[*]} " == *' __lumen_prompt '* ]] || PROMPT_COMMAND+=(__lumen_prompt)
    elif [[ ${PROMPT_COMMAND-} != *__lumen_prompt* ]]; then
        PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND
}__lumen_prompt"
    fi
fi
