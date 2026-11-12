# lumen: marks for zsh. In ~/.zshrc, as its last line:
#     eval "$(lumen shell-integration zsh)"
#
# The shell tells the terminal where a prompt begins (A) and ends (B), where
# the output of a command begins (C) and how the command ended (D). These are
# the marks of OSC 133, which other terminals read too.
if [[ -o interactive && -z ${__lumen_marks-} ]]; then
    typeset -g __lumen_marks=1

    __lumen_ended() {
        print -n "\e]133;D;$?\a"
    }

    # A prompt that a theme has just set anew is marked again.
    __lumen_prompt() {
        if [[ $PS1 != *'133;A'* ]]; then
            PS1=$'%{\e]133;A\a%}'$PS1$'%{\e]133;B\a%}'
        fi
    }

    __lumen_output() {
        print -n '\e]133;C\a'
    }

    autoload -Uz add-zsh-hook
    add-zsh-hook precmd __lumen_prompt
    add-zsh-hook preexec __lumen_output
    # first of what runs before a prompt: it has to see how the command ended
    typeset -ga precmd_functions
    precmd_functions=(__lumen_ended $precmd_functions)
fi
