//! Links in a terminal's output, and what a click on one does.
//!
//! A program marks a link with OSC 8. glint marks the names in a listing as
//! `glint-copy://host/path` links: other terminals hand those to the
//! desktop, which starts glint again to fill the clipboard; lumen knows
//! them, and does it itself.
//!
//! What a program doesn't mark is found in its text ([`in_text`]): a web
//! address, and a file that is there, with the line an error or a search
//! hit names behind it.

use std::ffi::OsString;
use std::io::Write;
use std::ops::Range;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// What a link leads to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A file on this machine that is offered for copying: a click puts it
    /// on the clipboard.
    Copy(PathBuf),
    /// A file or folder on this machine.
    File(PathBuf),
    /// A line in a file on this machine: what an error or a search hit names.
    Place { path: PathBuf, line: u32, column: Option<u32> },
    /// Anything else: a web page, a mail address.
    Url(String),
}

impl Target {
    /// The file the link is about, if it is about one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Target::Copy(path) | Target::File(path) | Target::Place { path, .. } => Some(path),
            Target::Url(_) => None,
        }
    }
}

/// What `uri` leads to. None for what lumen can't follow: a file on another
/// machine, or one of its own marks.
pub fn target(uri: &str) -> Option<Target> {
    let (scheme, rest) = uri.split_once(':')?;
    match scheme.to_ascii_lowercase().as_str() {
        "glint-copy" => local_path(rest).map(Target::Copy),
        "file" => local_path(rest).map(Target::File),
        "lumen-image" | "lumen-mark" => None,
        _ => Some(Target::Url(uri.to_string())),
    }
}

/// What plain text names at its character `at`: a web address, or a file
/// with or without a line in it. `text` is a line of output, `find` says
/// where a path as it is written leads, if there is such a file. Along with
/// the target comes the part of the text that names it.
pub fn in_text(text: &[char], at: usize, find: impl FnOnce(&str) -> Option<PathBuf>) -> Option<(Target, Range<usize>)> {
    if text.get(at)?.is_whitespace() {
        return None;
    }
    // the word around `at`
    let start = text[..at].iter().rposition(|c| c.is_whitespace()).map_or(0, |space| space + 1);
    let end = text[at..].iter().position(|c| c.is_whitespace()).map_or(text.len(), |space| at + space);
    let word = &text[start..end];
    let starts_with = |from: usize, prefix: &str| word[from..].iter().copied().take(prefix.len()).eq(prefix.chars());

    // a web address begins anywhere in the word, and ends before what only closes a sentence around it
    let address = (0..word.len()).find(|&from| starts_with(from, "https://") || starts_with(from, "http://"));
    if let Some(from) = address {
        let mut to = word.len();
        while to > from {
            let count = |c: char| word[from..to].iter().filter(|&&other| other == c).count();
            let closes = match word[to - 1] {
                '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '>' => true,
                ')' => count('(') < count(')'),
                ']' => count('[') < count(']'),
                '}' => count('{') < count('}'),
                _ => false,
            };
            if !closes {
                break;
            }
            to -= 1;
        }
        let address: String = word[from..to].iter().collect();
        let named = address.split_once("://").is_some_and(|(_, rest)| !rest.is_empty());
        let on_it = (start + from..start + to).contains(&at);
        return (named && on_it).then(|| (Target::Url(address), start + from..start + to));
    }

    // a file: without what is wrapped around it
    let (mut from, mut to) = (0, word.len());
    while from < to && matches!(word[from], '(' | '[' | '{' | '<' | '"' | '\'' | '`') {
        from += 1;
    }
    while to > from && matches!(word[to - 1], ')' | ']' | '}' | '>' | '"' | '\'' | '`' | ',' | ';' | '.' | ':') {
        // a closing bracket of `file(12,5)` belongs to it
        if word[to - 1] == ')' && word[from..to].contains(&'(') {
            break;
        }
        to -= 1;
    }
    let digits = |from: usize| word[from..to].iter().take_while(|c| c.is_ascii_digit()).count();
    let number = |from: usize, len: usize| word[from..from + len].iter().collect::<String>().parse::<u32>().ok();
    // `file:12`, `file:12:5`, `file(12)`, `file(12,5)`
    let mut place = None;
    for mark in from + 1..to {
        let (between, close) = match word[mark] {
            ':' => (':', None),
            '(' => (',', Some(')')),
            _ => continue,
        };
        let line_len = digits(mark + 1);
        if line_len == 0 {
            continue;
        }
        let Some(line) = number(mark + 1, line_len) else { continue };
        let mut after = mark + 1 + line_len;
        let mut column = None;
        if after < to && word[after] == between {
            let column_len = digits(after + 1);
            if let Some(found) = number(after + 1, column_len).filter(|_| column_len > 0) {
                column = Some(found);
                after += 1 + column_len;
            }
        }
        if let Some(close) = close {
            if after >= to || word[after] != close {
                continue;
            }
            after += 1;
        }
        place = Some((mark, line, column, after));
        break;
    }
    let path_end = place.map_or(to, |(mark, ..)| mark);
    let name: String = word[from..path_end].iter().collect();
    // what could be a file: a path, or a name with an ending
    let likely = !name.contains("://")
        && name.len() <= 4096
        && (name.contains('/') || name.contains('.'))
        && name.chars().any(|c| c.is_alphanumeric());
    if !likely {
        return None;
    }
    // `File "x.py", line 12`: how Python names a place
    let place = place.or_else(|| {
        let rest: String = text[end..].iter().take(24).collect();
        let number = rest.strip_prefix(" line ")?;
        let line = number[..number.find(|c: char| !c.is_ascii_digit()).unwrap_or(number.len())].parse().ok()?;
        Some((path_end, line, None, path_end))
    });
    let name_end = place.map_or(path_end, |(.., after)| after);
    if !(start + from..start + name_end).contains(&at) {
        return None;
    }
    let path = find(&name)?;
    let target = match place {
        Some((_, line, column, _)) if path.is_file() => Target::Place { path, line, column },
        _ => Target::File(path),
    };
    Some((target, start + from..start + name_end))
}

/// Where a path that a program printed leads: as it is if it starts at the
/// root or at `~`, else from the folder the program ran in. Tools that are
/// run further down often name files from the project's top, so the folders
/// above are tried too. None if there is nothing by that name.
pub fn find_file(name: &str, folder: Option<&Path>) -> Option<PathBuf> {
    let name = Path::new(name);
    if let Ok(rest) = name.strip_prefix("~") {
        return dirs::home_dir().map(|home| home.join(rest)).filter(|path| path.exists());
    }
    if name.is_absolute() {
        return name.exists().then(|| name.to_path_buf());
    }
    // a bare name is only looked for where the program ran: further up it would be found by chance
    let above = if name.components().count() > 1 { 6 } else { 1 };
    let home = dirs::home_dir();
    for folder in folder?.ancestors().take(above) {
        let path = folder.join(name);
        if path.exists() {
            return Some(path);
        }
        if home.as_deref() == Some(folder) {
            break;
        }
    }
    None
}

/// The path in `//host/path` (or `/path`), when the host is this machine.
fn local_path(rest: &str) -> Option<PathBuf> {
    let path = match rest.strip_prefix("//") {
        Some(rest) => {
            let (host, path) = rest.split_at(rest.find('/')?);
            let here =
                host.is_empty() || host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case(hostname());
            if !here {
                return None;
            }
            path
        },
        None => rest,
    };
    let path = path.split(['?', '#']).next()?;
    if !path.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(OsString::from_vec(percent_decode(path)?)))
}

fn hostname() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        std::fs::read_to_string("/proc/sys/kernel/hostname").map(|s| s.trim().to_string()).unwrap_or_default()
    })
}

pub fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Paths as the clipboard's `text/uri-list`.
fn uri_list(paths: &[PathBuf]) -> String {
    let mut list = String::new();
    for path in paths {
        list.push_str("file://");
        for &byte in path.as_os_str().as_bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => list.push(byte as char),
                _ => list.push_str(&format!("%{byte:02X}")),
            }
        }
        list.push_str("\r\n");
    }
    list
}

/// Puts files on the clipboard, as a file manager does: pasting them
/// somewhere copies them. This goes through wl-copy or xclip, which stay
/// behind to hold the clipboard.
pub fn copy_files(paths: &[PathBuf]) -> Result<(), String> {
    let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    let (tool, args): (&str, &[&str]) = if set("WAYLAND_DISPLAY") {
        ("wl-copy", &["--type", "text/uri-list"])
    } else if set("DISPLAY") {
        ("xclip", &["-selection", "clipboard", "-t", "text/uri-list"])
    } else {
        return Err("no desktop session to copy to".into());
    };
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound if tool == "wl-copy" => {
                "copying files needs wl-copy (wl-clipboard)".to_string()
            },
            std::io::ErrorKind::NotFound => format!("copying files needs {tool}"),
            _ => format!("{tool}: {e}"),
        })?;
    let wrote = child.stdin.take().map(|mut stdin| stdin.write_all(uri_list(paths).as_bytes()));
    // the tool exits once it has left its clipboard holder behind: wait for that elsewhere
    std::thread::spawn(move || _ = child.wait());
    match wrote {
        Some(Ok(())) => Ok(()),
        _ => Err(format!("{tool} didn't take the file")),
    }
}

/// A path as one word of a shell command.
pub fn shell_quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_' | '+' | ',' | '@' | ':'));
    if plain { text.into_owned() } else { format!("'{}'", text.replace('\'', "'\\''")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_links_lead() {
        assert_eq!(target("glint-copy:///home/u/a%20b.txt"), Some(Target::Copy("/home/u/a b.txt".into())));
        assert_eq!(target("glint-copy://localhost/etc/hosts"), Some(Target::Copy("/etc/hosts".into())));
        assert_eq!(target("file:///tmp/x"), Some(Target::File("/tmp/x".into())));
        assert_eq!(target("file:/tmp/x"), Some(Target::File("/tmp/x".into())));
        assert_eq!(target("https://example.com/a?b#c"), Some(Target::Url("https://example.com/a?b#c".into())));
        // a file on another machine can't be copied from here
        assert_eq!(target("glint-copy://some-other-host.invalid/etc/hosts"), None);
        assert_eq!(target("lumen-image:1:0:0"), None);
        assert_eq!(target("no scheme"), None);
    }

    /// What the text names at the first character of `on`, and the part of the text that names it.
    fn named(text: &str, on: &str) -> Option<(Target, String)> {
        let chars: Vec<char> = text.chars().collect();
        let at = text[..text.find(on).unwrap()].chars().count();
        // every file is there, in /w
        let (target, range) = in_text(&chars, at, |name| Some(Path::new("/w").join(name)))?;
        Some((target, chars[range].iter().collect()))
    }

    #[test]
    fn web_addresses_in_plain_text() {
        let url = |text: &str, on: &str| named(text, on);
        let found = |address: &str| Some((Target::Url(address.to_string()), address.to_string()));
        assert_eq!(url("see https://example.com/a?b=1#c for more", "example"), found("https://example.com/a?b=1#c"));
        assert_eq!(url("(https://example.com/a).", "https"), found("https://example.com/a"));
        assert_eq!(
            url("https://en.wikipedia.org/wiki/Rust_(language),", "wiki"),
            found("https://en.wikipedia.org/wiki/Rust_(language)")
        );
        assert_eq!(url("<http://example.com>", "http"), found("http://example.com"));
        assert_eq!(url("docs: \"https://example.com/x\"", "https"), found("https://example.com/x"));
        // the mouse is on the bracket before it, on a space, on a scheme with nothing behind it
        assert_eq!(url("(https://example.com)", "("), None);
        assert_eq!(url("a  b", " "), None);
        assert_eq!(url("https://", "https"), None);
    }

    #[test]
    fn files_in_plain_text() {
        let file = |text: &str, on: &str| named(text, on);
        // (the files named here are not there, so the line is left out of what is found: see the next test)
        let found = |path: &str, part: &str| Some((Target::File(PathBuf::from(path)), part.to_string()));
        assert_eq!(file("   --> src/main.rs:12:5", "src"), found("/w/src/main.rs", "src/main.rs:12:5"));
        assert_eq!(file("src/a.rs:12:    let x = 1;", "src"), found("/w/src/a.rs", "src/a.rs:12"));
        assert_eq!(file("./main.go:7:2: undefined: x", "main"), found("/w/./main.go", "./main.go:7:2"));
        assert_eq!(file("at (src/app.ts(3,14)): error", "app"), found("/w/src/app.ts", "src/app.ts(3,14)"));
        assert_eq!(file("  File \"tools/x.py\", line 12, in f", "tools"), found("/w/tools/x.py", "tools/x.py"));
        assert_eq!(file("see README.md.", "README"), found("/w/README.md", "README.md"));
        assert_eq!(file("`docs/a.md`,", "docs"), found("/w/docs/a.md", "docs/a.md"));
        // words that are no file names, and the mouse beside the name
        assert_eq!(file("exit 12:30 today", "12"), None);
        assert_eq!(file("word", "word"), None);
        assert_eq!(file("...", "."), None);
        assert_eq!(file("(src/a.rs)", "("), None);
        // a file that isn't there is nothing
        let chars: Vec<char> = "src/gone.rs:3".chars().collect();
        assert_eq!(in_text(&chars, 2, |_| None), None);
    }

    #[test]
    fn a_line_in_a_file_that_is_there() {
        let folder = std::env::temp_dir().join(format!("lumen-links-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("crate/src")).unwrap();
        std::fs::write(folder.join("crate/src/lib.rs"), "").unwrap();
        std::fs::write(folder.join("top.txt"), "").unwrap();
        let from = folder.join("crate/src");
        let at = |text: &str| {
            let chars: Vec<char> = text.chars().collect();
            in_text(&chars, 0, |name| find_file(name, Some(&from))).map(|(target, _)| target)
        };
        let lib = folder.join("crate/src/lib.rs");
        assert_eq!(at("lib.rs:4:9"), Some(Target::Place { path: lib.clone(), line: 4, column: Some(9) }));
        assert_eq!(at("lib.rs(4)"), Some(Target::Place { path: lib.clone(), line: 4, column: None }));
        // named from the folder above, as a tool run further up does
        assert_eq!(
            at("src/lib.rs:1"),
            Some(Target::Place { path: folder.join("crate/src/lib.rs"), line: 1, column: None })
        );
        assert_eq!(at("crate/src/lib.rs"), Some(Target::File(lib.clone())));
        // a bare name is not looked for further up, a folder has no lines, and an absolute path is taken as it is
        assert_eq!(at("top.txt:1"), None);
        assert_eq!(at("crate/src:3"), Some(Target::File(folder.join("crate/src"))));
        let absolute = format!("{}:2", lib.display());
        assert_eq!(at(&absolute), Some(Target::Place { path: lib, line: 2, column: None }));
        assert_eq!(find_file("lib.rs", None), None);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_clipboard_gets_escaped_uris() {
        let list = uri_list(&["/home/u/a b.txt".into(), "/tmp/ü".into()]);
        assert_eq!(list, "file:///home/u/a%20b.txt\r\nfile:///tmp/%C3%BC\r\n");
    }

    #[test]
    fn paths_as_shell_words() {
        assert_eq!(shell_quote(Path::new("/tmp/a.txt")), "/tmp/a.txt");
        assert_eq!(shell_quote(Path::new("/tmp/a b")), "'/tmp/a b'");
        assert_eq!(shell_quote(Path::new("it's")), "'it'\\''s'");
    }
}
