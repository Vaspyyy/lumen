# lumen: marks for fish older than 4.0. In ~/.config/fish/config.fish:
#     lumen shell-integration fish | source
#
# fish 4.0 and newer mark their commands themselves, and need none of this.
#
# The shell tells the terminal where a prompt begins (A), where the output of
# a command begins (C) and how the command ended (D). These are the marks of
# OSC 133, which other terminals read too.
if status is-interactive; and not set -q __lumen_marks
    set -g __lumen_marks 1
    if test (string split . -- $version)[1] -lt 4
        function __lumen_prompt --on-event fish_prompt
            printf '\e]133;A\a'
        end
        function __lumen_output --on-event fish_preexec
            printf '\e]133;C;cmdline_url=%s\a' (string escape --style=url -- $argv[1])
        end
        function __lumen_ended --on-event fish_postexec
            printf '\e]133;D;%s\a' $status
        end
    end
end
