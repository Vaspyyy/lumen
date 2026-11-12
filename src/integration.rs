//! `lumen shell-integration <shell>`: the lines that make a shell mark its
//! commands (see [`crate::commands`]).
//!
//! fish does that by itself from version 4.0 on. bash and zsh are told to
//! with one line in their startup file, which reads these lines in. They
//! are printed, not written anywhere: what a shell runs at startup is its
//! owner's to decide.

pub const USAGE: &str = "\
SHELLS
    lumen shell-integration bash|zsh|fish
                                        print the lines that make a shell mark its commands;
                                        for bash: eval \"$(lumen shell-integration bash)\"
";

const BASH: &str = include_str!("../assets/shell/lumen.bash");
const ZSH: &str = include_str!("../assets/shell/lumen.zsh");
const FISH: &str = include_str!("../assets/shell/lumen.fish");

/// Runs the command if `args` is one, and gives its exit status.
pub fn run(args: &[String]) -> Option<i32> {
    let (command, rest) = args.split_first()?;
    if command != "shell-integration" {
        return None;
    }
    let script = match rest {
        [shell] => match shell.as_str() {
            "bash" => BASH,
            "zsh" => ZSH,
            "fish" => FISH,
            other => {
                eprintln!("lumen: no shell integration for {other}: there is one for bash, zsh and fish");
                return Some(2);
            },
        },
        _ => {
            eprintln!("lumen: shell-integration wants one shell: bash, zsh or fish\nTry 'lumen --help'.");
            return Some(2);
        },
    };
    print!("{script}");
    Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_marks_all_four() {
        for (script, marks) in [(BASH, "ABCD"), (ZSH, "ABCD"), (FISH, "ACD")] {
            for mark in marks.chars() {
                assert!(script.contains(&format!("133;{mark}")), "{mark} is missing");
            }
        }
    }

    #[test]
    fn only_known_shells() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(run(&args(&["pair"])), None);
        assert_eq!(run(&args(&[])), None);
        assert_eq!(run(&args(&["shell-integration"])), Some(2));
        assert_eq!(run(&args(&["shell-integration", "csh"])), Some(2));
        assert_eq!(run(&args(&["shell-integration", "bash", "zsh"])), Some(2));
    }
}
