//! Links in a terminal's output (OSC 8), and what a click on one does.
//!
//! glint marks the names in a listing as `glint-copy://host/path` links.
//! Other terminals hand those to the desktop, which starts glint again to
//! fill the clipboard; lumen knows them, and does it itself.

use std::ffi::OsString;
use std::io::Write;
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
    /// Anything else: a web page, a mail address.
    Url(String),
}

impl Target {
    /// The file the link is about, if it is about one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Target::Copy(path) | Target::File(path) => Some(path),
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
        "lumen-image" => None,
        _ => Some(Target::Url(uri.to_string())),
    }
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

fn percent_decode(text: &str) -> Option<Vec<u8>> {
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
