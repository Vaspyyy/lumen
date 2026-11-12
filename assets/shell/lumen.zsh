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
    }

    # A prompt that a theme has just set anew is marked again.
    __lumen_prompt() {
        if [[ $PS1 != *'133;A'* ]]; then
            PS1=$'%{\e]133;A\a%}'$PS1$'%{\e]133;B\a%}'
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
