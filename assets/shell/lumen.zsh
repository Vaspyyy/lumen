# lumen: marks for zsh. In ~/.zshrc, as its last line:
#     eval "$(lumen shell-integration zsh)"
#
# The shell tells the terminal where a prompt begins (A) and ends (B), where
# the output of a command begins (C) and how the command ended (D). These are
# the marks of OSC 133, which other terminals read too.
if [[ -o interactive && -z ${__lumen_marks-} ]]; then
    typeset -g __lumen_marks=1

    # How the command ended. Only after a command: a shell that has just
    # started has none to tell of.
    __lumen_ended() {
        local ended=$?
        if [[ -n ${__lumen_ran-} ]]; then
            typeset -g __lumen_ran=
            print -n "\e]133;D;$ended\a"
        fi
        # A new prompt is on its way (L). One that zsh writes without this is
        # the open one again: after a resize, or on a cleared screen.
        print -n '\e]133;L\a'
    }

    # A prompt that a theme has just set anew is marked again.
    __lumen_prompt() {
        if [[ $PS1 != *'133;A'* ]]; then
            PS1=$'%{\e]133;A\a%}'$PS1$'%{\e]133;B\a%}'
        fi
        # The prompt at the right of the line, and the one a further line of a
        # command begins with, are marked as prompts (P), so that lumen does
        # not take them for a part of the command. Only in lumen: the B that
        # ends them is where a command begins to other terminals.
        if [[ ${TERM_PROGRAM-} == lumen ]]; then
            # (one prompt under two names, of which either may count as unset)
            local right=${RPROMPT-${RPS1-}}
            if [[ -n $right && $right != *'133;P'* ]]; then
                RPROMPT=$'%{\e]133;P;k=r\a%}'$right$'%{\e]133;B\a%}'
            fi
            if [[ -n ${PS2-} && $PS2 != *'133;P'* ]]; then
                PS2=$'%{\e]133;P;k=c\a%}'$PS2$'%{\e]133;B\a%}'
            fi
        fi
    }

    # The command is sent off. Its text goes along: what stands on the screen
    # beside it may be a right prompt.
    __lumen_output() {
        emulate -L zsh
        setopt no_multibyte
        typeset -g __lumen_ran=1
        local command=${1:-$3} text= c
        local -i i
        # a command longer than this is read from the screen
        if (( ${#command} == 0 || ${#command} > 4096 )); then
            print -n '\e]133;C\a'
            return
        fi
        for (( i = 1; i <= ${#command}; i++ )); do
            c=$command[i]
            case $c in
                [a-zA-Z0-9._~/-]) text+=$c ;;
                *) text+=%${(l:2::0:)$(( [##16] #c ))} ;;
            esac
        done
        print -n "\e]133;C;cmdline_url=$text\a"
    }

    autoload -Uz add-zsh-hook
    add-zsh-hook precmd __lumen_prompt
    add-zsh-hook preexec __lumen_output
    # first of what runs before a prompt: it has to see how the command ended
    typeset -ga precmd_functions
    precmd_functions=(__lumen_ended $precmd_functions)
fi
