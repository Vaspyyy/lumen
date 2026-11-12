//! Opening a place in a file: which program is started, and with which
//! words.
//!
//! The program is the config's `[editor] command`, or else what `$VISUAL`
//! or `$EDITOR` name. Editors say where to go in different ways; lumen knows
//! the common ones by name, and takes a line with `{file}`, `{line}` and
//! `{column}` in it for the rest.

use std::path::Path;

/// How an editor is started for a place in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub argv: Vec<String>,
    /// It runs in a terminal, so it gets a tab. Otherwise it has a window of its own.
    pub terminal: bool,
}

/// How a program is told the line.
#[derive(Clone, Copy, PartialEq)]
enum Form {
    /// `+line file`: vi and most editors of a terminal.
    Plus,
    /// `+line:column file`
    PlusColumn,
    /// `file:line:column`
    Suffix,
    /// `--goto file:line:column`
    Goto,
    /// `--line line --column column file`
    Flags,
}

/// What lumen knows of an editor by its name: how it takes the line, and
/// whether it runs in a terminal.
fn known(program: &str, args: &[String]) -> (Form, bool) {
    let has = |flags: &[&str]| args.iter().any(|arg| flags.contains(&arg.as_str()));
    match program {
        "code" | "code-insiders" | "codium" | "vscodium" | "cursor" | "windsurf" => (Form::Goto, false),
        "zed" | "zeditor" | "zedit" | "subl" | "sublime_text" | "geany" => (Form::Suffix, false),
        "kate" | "kwrite" => (Form::Flags, false),
        "idea" | "pycharm" | "clion" | "rustrover" | "webstorm" | "goland" | "rider" | "phpstorm" => {
            (Form::Flags, false)
        },
        "gedit" | "xed" | "pluma" | "featherpad" => (Form::Plus, false),
        // Emacs opens a window of its own unless it is told to stay in the terminal
        "emacs" => (Form::PlusColumn, has(&["-nw", "--no-window-system"])),
        "emacsclient" => (Form::PlusColumn, has(&["-t", "-nw", "--tty"])),
        "hx" | "helix" => (Form::Suffix, true),
        "micro" | "kak" => (Form::PlusColumn, true),
        _ => (Form::Plus, true),
    }
}

/// The words of a command line: split at spaces, with quotes and
/// backslashes keeping spaces in a word.
pub fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (c, quote) {
            ('\\', None | Some('"')) => {
                if let Some(next) = chars.next() {
                    word.push(next);
                    started = true;
                }
            },
            ('\'' | '"', None) => {
                quote = Some(c);
                started = true;
            },
            (c, Some(open)) if c == open => quote = None,
            (c, None) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            },
            (c, _) => {
                word.push(c);
                started = true;
            },
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// How the editor is started for this place. `command` and `terminal` are
/// what the config says, `env` reads a variable of the environment. None
/// where no editor is named anywhere.
pub fn launch(
    command: Option<&str>,
    terminal: Option<bool>,
    env: impl Fn(&str) -> Option<String>,
    path: &Path,
    line: u32,
    column: Option<u32>,
) -> Option<Launch> {
    let named = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
    let command =
        named(command.map(str::to_string)).or_else(|| named(env("VISUAL"))).or_else(|| named(env("EDITOR")))?;
    let mut argv = words(&command);
    let program = Path::new(argv.first()?).file_name()?.to_string_lossy().into_owned();
    let (form, in_terminal) = known(&program, &argv[1..]);
    let file = path.to_string_lossy().into_owned();
    let column_or_first = column.unwrap_or(1);
    if command.contains("{file}") {
        for word in &mut argv {
            *word = word
                .replace("{file}", &file)
                .replace("{line}", &line.to_string())
                .replace("{column}", &column_or_first.to_string());
        }
    } else {
        match form {
            Form::Plus => argv.extend([format!("+{line}"), file]),
            Form::PlusColumn => argv.extend([format!("+{line}:{column_or_first}"), file]),
            Form::Suffix => argv.push(format!("{file}:{line}:{column_or_first}")),
            Form::Goto => argv.extend(["--goto".to_string(), format!("{file}:{line}:{column_or_first}")]),
            Form::Flags => argv.extend([
                "--line".to_string(),
                line.to_string(),
                "--column".to_string(),
                column_or_first.to_string(),
                file,
            ]),
        }
    }
    Some(Launch { argv, terminal: terminal.unwrap_or(in_terminal) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(command: Option<&str>, terminal: Option<bool>, env: &[(&str, &str)]) -> Option<(String, bool)> {
        let env = |name: &str| env.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string());
        let launch = launch(command, terminal, env, Path::new("/src/a b.rs"), 12, Some(5))?;
        Some((launch.argv.join("|"), launch.terminal))
    }

    #[test]
    fn editors_take_the_line_in_their_own_way() {
        let with = |command: &str| started(Some(command), None, &[]).unwrap();
        assert_eq!(with("nvim"), ("nvim|+12|/src/a b.rs".into(), true));
        assert_eq!(with("/usr/bin/vim -p"), ("/usr/bin/vim|-p|+12|/src/a b.rs".into(), true));
        assert_eq!(with("hx"), ("hx|/src/a b.rs:12:5".into(), true));
        assert_eq!(with("micro"), ("micro|+12:5|/src/a b.rs".into(), true));
        assert_eq!(with("code -r"), ("code|-r|--goto|/src/a b.rs:12:5".into(), false));
        assert_eq!(with("zed"), ("zed|/src/a b.rs:12:5".into(), false));
        assert_eq!(with("kate"), ("kate|--line|12|--column|5|/src/a b.rs".into(), false));
        assert_eq!(with("emacs"), ("emacs|+12:5|/src/a b.rs".into(), false));
        assert_eq!(with("emacs -nw"), ("emacs|-nw|+12:5|/src/a b.rs".into(), true));
        assert_eq!(with("emacsclient -t"), ("emacsclient|-t|+12:5|/src/a b.rs".into(), true));
        // one lumen has never heard of is taken for an editor of the terminal
        assert_eq!(with("ed"), ("ed|+12|/src/a b.rs".into(), true));
    }

    #[test]
    fn a_line_with_places_is_filled_in() {
        let (argv, terminal) = started(Some("myedit --at {line},{column} '{file}' --wait"), None, &[]).unwrap();
        assert_eq!(argv, "myedit|--at|12,5|/src/a b.rs|--wait");
        assert!(terminal);
        // what the config says about the terminal counts, for a known editor too
        assert!(!started(Some("myedit {file}"), Some(false), &[]).unwrap().1);
        assert!(started(Some("code"), Some(true), &[]).unwrap().1);
        // without a column, the first
        let launch = launch(Some("hx"), None, |_| None, Path::new("/a.rs"), 3, None).unwrap();
        assert_eq!(launch.argv, ["hx", "/a.rs:3:1"]);
    }

    #[test]
    fn the_config_comes_before_the_environment() {
        let env = [("VISUAL", "code"), ("EDITOR", "vi")];
        assert_eq!(started(Some("hx"), None, &env).unwrap().0, "hx|/src/a b.rs:12:5");
        assert_eq!(started(None, None, &env).unwrap().0, "code|--goto|/src/a b.rs:12:5");
        assert_eq!(started(Some("  "), None, &env[1..]).unwrap().0, "vi|+12|/src/a b.rs");
        assert_eq!(started(None, None, &[("VISUAL", ""), ("EDITOR", "nano")]).unwrap().0, "nano|+12|/src/a b.rs");
        assert_eq!(started(None, None, &[]), None);
    }

    #[test]
    fn words_of_a_command_line() {
        assert_eq!(words("  code   --wait "), ["code", "--wait"]);
        assert_eq!(words(r#"'my editor' "a b" c\ d ''"#), ["my editor", "a b", "c d", ""]);
        assert_eq!(words(r#"say "it's" 'a "b"'"#), ["say", "it's", r#"a "b""#]);
        assert_eq!(words(""), Vec::<String>::new());
    }
}
