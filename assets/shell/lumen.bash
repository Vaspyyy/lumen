# lumen: marks for bash. In ~/.bashrc, as its last line:
#     eval "$(lumen shell-integration bash)"
#
# The shell tells the terminal where a prompt begins (A) and ends (B), where
# the output of a command begins (C) and how the command ended (D). These are
# the marks of OSC 133, which other terminals read too.
if [[ $- == *i* && -z ${__lumen_marks-} ]]; then
    __lumen_marks=1

    # Last of what runs before a prompt, so that a prompt a theme has just set
    # anew is marked again, and so that nothing of lumen's runs before a
    # theme's own commands: they find $? and PIPESTATUS as the command left them.
    __lumen_prompt() {
        # there is no command to tell of before a shell's first prompt
        if [[ -n ${__lumen_first-} ]]; then
            __lumen_ran=1
        else
            __lumen_first=1
        fi
        if [[ $PS1 != *'133;A'* ]]; then
            local esc=$'\e' bel=$'\a'
            if shopt -q promptvars; then
                # How the command ended is said in the prompt itself: when bash
                # writes it, $? is the command's again, whatever ran in between.
                PS1="\\[\${__lumen_ran:+${esc}]133;D;\$?${bel}}${esc}]133;A${bel}\\]${PS1}\\[${esc}]133;B${bel}\\]"
            else
                PS1="\\[${esc}]133;A${bel}\\]${PS1}\\[${esc}]133;B${bel}\\]"
            fi
        fi
    }

    # PS0 is printed after a command is read and before it runs.
    PS0='\e]133;C\a'${PS0-}

    __lumen_line=$'\n'
    if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a'* ]]; then
        PROMPT_COMMAND=("${PROMPT_COMMAND[@]}" __lumen_prompt)
    else
        PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND$__lumen_line}__lumen_prompt"
    fi

    # A shell whose prompt takes no variables can't say $? in it: there the
    # command's end is told first of all that runs before a prompt.
    if ! shopt -q promptvars; then
        __lumen_ended() {
            local ended=$?
            [[ -n ${__lumen_first-} ]] && printf '\e]133;D;%s\a' "$ended"
            return "$ended"
        }
        if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a'* ]]; then
            PROMPT_COMMAND=(__lumen_ended "${PROMPT_COMMAND[@]}")
        else
            PROMPT_COMMAND="__lumen_ended$__lumen_line$PROMPT_COMMAND"
        fi
    fi
    unset __lumen_line
fi
