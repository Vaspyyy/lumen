//! lumen's lines in `authorized_keys`: the short-lived pairing key and the
//! phones' own keys.
//!
//! The file belongs to the user, and sshd reads it on every login. lumen
//! changes nothing in it but its own lines (other lines stay byte for byte),
//! and every change replaces the whole file at once, so that sshd never
//! reads half of one.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use lumen_link::{PAIRING_COMMENT, PHONE_COMMENT};

/// The key types a phone may log in with: what Android's keystore makes,
/// and what OpenSSH has long accepted.
const PHONE_KEY_TYPES: [&str; 5] =
    ["ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521", "ssh-ed25519", "ssh-rsa"];

/// A phone's public key, checked: one of the types above, whose encoded
/// form holds just what that type holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneKey {
    pub kind: String,
    pub blob: Vec<u8>,
}

impl PhoneKey {
    /// Takes `<type> <base64>` and nothing else. What goes into
    /// `authorized_keys` is made from the decoded key, never copied from
    /// what was received, so nobody can slip options or a second line in.
    pub fn parse(text: &str) -> Result<PhoneKey, String> {
        let words: Vec<&str> = text.split_ascii_whitespace().collect();
        let [kind, encoded] = words[..] else {
            return Err("the key is not in the form \"<type> <key>\"".into());
        };
        if text.len() > 8192 || text.contains(['\n', '\r', '\0']) {
            return Err("the key is not in the form \"<type> <key>\"".into());
        }
        if !PHONE_KEY_TYPES.contains(&kind) {
            return Err(format!("keys of the type {kind} can't be used"));
        }
        let blob = STANDARD.decode(encoded).map_err(|_| "the key is damaged".to_string())?;
        if lumen_link::key_blob(&format!("{kind} {encoded}")).as_deref() != Some(&blob[..]) || !well_formed(kind, &blob)
        {
            return Err("the key is damaged".into());
        }
        Ok(PhoneKey { kind: kind.into(), blob })
    }

    /// The key as `authorized_keys` and `ssh-keygen` write it.
    pub fn line(&self) -> String {
        format!("{} {}", self.kind, STANDARD.encode(&self.blob))
    }

    pub fn fingerprint(&self) -> String {
        lumen_link::fingerprint(&self.line()).map(|f| lumen_link::fingerprint_text(&f)).unwrap_or_default()
    }
}

/// Whether a key's encoded form is made of the parts its type has, and no
/// more: the type's name, then the type's numbers or points.
fn well_formed(kind: &str, blob: &[u8]) -> bool {
    let mut rest = blob;
    let mut parts = Vec::new();
    while !rest.is_empty() {
        let Some(len) = rest.get(..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize) else {
            return false;
        };
        let Some(part) = rest.get(4..4 + len) else { return false };
        parts.push(part);
        rest = &rest[4 + len..];
    }
    match (kind, &parts[..]) {
        ("ssh-ed25519", [name, key]) => *name == kind.as_bytes() && key.len() == 32,
        ("ssh-rsa", [name, e, n]) => *name == kind.as_bytes() && !e.is_empty() && n.len() >= 128,
        (_, [name, curve, point]) => {
            let Some(curve_name) = kind.strip_prefix("ecdsa-sha2-") else { return false };
            *name == kind.as_bytes() && *curve == curve_name.as_bytes() && point.first() == Some(&4)
        },
        _ => false,
    }
}

/// A paired phone, as a line of the file says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phone {
    pub name: String,
    pub fingerprint: String,
}

/// What lumen sees in a line.
#[derive(Debug, PartialEq, Eq)]
enum Line {
    /// A short-lived pairing key, with when it expires (`YYYYMMDDHHMMSS`).
    Pairing { token: String, expiry: Option<String> },
    /// A phone's key.
    Phone { name: String, blob: Vec<u8> },
    /// Anybody else's line: left alone.
    Other,
}

fn classify(line: &[u8]) -> Line {
    let Ok(text) = std::str::from_utf8(line) else { return Line::Other };
    let text = text.trim_end_matches(['\n', '\r']);
    if text.trim_start().starts_with('#') {
        return Line::Other;
    }
    let Some((options, blob, comment)) = split_key(text) else { return Line::Other };
    if let Some(token) = comment.strip_prefix(PAIRING_COMMENT) {
        let expiry = options
            .split_once("expiry-time=\"")
            .and_then(|(_, after)| after.split_once('"'))
            .map(|(stamp, _)| stamp.to_string());
        return Line::Pairing { token: token.trim().into(), expiry };
    }
    if let Some(name) = comment.strip_prefix(PHONE_COMMENT) {
        return Line::Phone { name: name.trim().into(), blob };
    }
    Line::Other
}

/// Splits a line into what comes before the key type (options), the key
/// and the comment after it.
fn split_key(text: &str) -> Option<(&str, Vec<u8>, &str)> {
    let words: Vec<(usize, &str)> =
        text.split_ascii_whitespace().map(|word| (word.as_ptr() as usize - text.as_ptr() as usize, word)).collect();
    for pair in words.windows(2) {
        let [(kind_at, kind), (key_at, key)] = pair else { continue };
        let Ok(blob) = STANDARD.decode(key) else { continue };
        let named = blob.get(..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
        if named.and_then(|len| blob.get(4..4 + len)) == Some(kind.as_bytes()) {
            let comment = text[key_at + key.len()..].trim();
            return Some((&text[..*kind_at], blob, comment));
        }
    }
    None
}

fn lines(text: &[u8]) -> impl Iterator<Item = &[u8]> {
    text.split_inclusive(|&b| b == b'\n')
}

/// The file without the pairing lines whose time is up: what a `lumen pair`
/// that was killed left behind. `now` is a local `YYYYMMDDHHMMSS`.
pub fn without_stale(text: &[u8], now: &str) -> Vec<u8> {
    keep(text, |line| !matches!(classify(line), Line::Pairing { expiry: Some(expiry), .. } if expiry.as_str() < now))
}

pub fn without_pairing(text: &[u8], token: &str) -> Vec<u8> {
    keep(text, |line| !matches!(classify(line), Line::Pairing { token: t, .. } if t == token))
}

fn keep(text: &[u8], mut wanted: impl FnMut(&[u8]) -> bool) -> Vec<u8> {
    lines(text).filter(|line| wanted(line)).flatten().copied().collect()
}

/// The file with one more line at the end.
pub fn with_line(text: &[u8], line: &str) -> Vec<u8> {
    let mut out = text.to_vec();
    if !out.is_empty() && !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    out.extend_from_slice(line.as_bytes());
    out.push(b'\n');
    out
}

/// The short-lived key's line: it may only run `lumen pair-enroll`, only
/// until `expiry`, and nothing else (`restrict` takes away forwarding, a
/// PTY, `~/.ssh/rc`).
pub fn pairing_line(lumen: &Path, token: &str, expiry: &str, public_key_line: &str) -> Result<String, String> {
    let command = format!("{} {} {token}", shell_quote(lumen)?, lumen_link::ENROLL_COMMAND);
    Ok(format!(
        "command=\"{}\",restrict,expiry-time=\"{expiry}\" {public_key_line} {PAIRING_COMMENT}{token}",
        option_quote(&command)
    ))
}

/// A path for the user's shell, which sshd runs the command with: fish,
/// bash and zsh all take single quotes the same way, as long as there is no
/// backslash in them (fish reads `\\` in single quotes as one).
fn shell_quote(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or("the path of lumen is not UTF-8")?;
    if text.contains(['\\', '\n', '\r', '\0']) {
        return Err(format!("the path of lumen ({text}) has a character in it a shell can't be given safely"));
    }
    Ok(format!("'{}'", text.replace('\'', r"'\''")))
}

/// A value for `command="..."`: sshd takes `\"` as a quote inside and
/// copies every other backslash as it is.
fn option_quote(value: &str) -> String {
    value.replace('"', "\\\"")
}

/// The file with the phone's key in place of the pairing line, in one go.
/// A line with the same key is replaced; a name another phone has already
/// gets a number. Returns the new text and the name the phone got.
pub fn enrolled(text: &[u8], token: &str, key: &PhoneKey, name: &str) -> (Vec<u8>, String) {
    let rest = keep(text, |line| match classify(line) {
        Line::Pairing { token: t, .. } => t != token,
        Line::Phone { blob, .. } => blob != key.blob,
        Line::Other => true,
    });
    let taken: Vec<String> = phones_in(&rest).into_iter().map(|(name, _)| name).collect();
    let mut chosen = name.to_string();
    let mut number = 2;
    while taken.contains(&chosen) {
        chosen = format!("{name} {number}");
        number += 1;
    }
    let line = format!("restrict,pty {} {PHONE_COMMENT}{chosen}", key.line());
    (with_line(&rest, &line), chosen)
}

fn phones_in(text: &[u8]) -> Vec<(String, Vec<u8>)> {
    lines(text)
        .filter_map(|line| match classify(line) {
            Line::Phone { name, blob } => Some((name, blob)),
            _ => None,
        })
        .collect()
}

pub fn phones(text: &[u8]) -> Vec<Phone> {
    phones_in(text)
        .into_iter()
        .map(|(name, blob)| {
            let fingerprint = lumen_link::fingerprint_text(&sha256(&blob));
            Phone { name, fingerprint }
        })
        .collect()
}

fn sha256(blob: &[u8]) -> [u8; 32] {
    // the fingerprint of a line is the hash of its key, whatever its type
    let line = format!("{} {}", key_type(blob), STANDARD.encode(blob));
    lumen_link::fingerprint(&line).unwrap_or_default()
}

fn key_type(blob: &[u8]) -> String {
    let len = blob.get(..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0);
    blob.get(4..4 + len).map(|name| String::from_utf8_lossy(name).into_owned()).unwrap_or_default()
}

/// The file without the phone of exactly this name, or `None` if there is
/// no such phone.
pub fn without_phone(text: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut found = false;
    let rest = keep(text, |line| match classify(line) {
        Line::Phone { name: n, .. } if n == name => {
            found = true;
            false
        },
        _ => true,
    });
    found.then_some(rest)
}

/// Where the file is: the one given, else `~/.ssh/authorized_keys`.
pub fn path(given: Option<PathBuf>, home: &Path) -> PathBuf {
    given.unwrap_or_else(|| home.join(".ssh").join("authorized_keys"))
}

/// Reads the file, or nothing when there is none yet.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    match fs::read(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Changes the file: `change` gets what is in it and returns what it should
/// be. The folder (0700) and the file (0600) are made when missing. Two
/// lumens changing it at once take turns, and sshd sees either the old file
/// or the new one, never a mix.
pub fn edit<T>(path: &Path, change: impl FnOnce(&[u8]) -> T) -> io::Result<T>
where
    T: Edited,
{
    let (_lock, real) = lock(path)?;
    let mut text = Vec::new();
    File::open(&real)?.read_to_end(&mut text)?;
    let result = change(&text);
    if let Some(new) = result.text()
        && new != text.as_slice()
    {
        replace(&real, new)?;
    }
    Ok(result)
}

/// What an edit returns: the new text, if there is one, and whatever else.
pub trait Edited {
    fn text(&self) -> Option<&[u8]>;
}

impl Edited for Vec<u8> {
    fn text(&self) -> Option<&[u8]> {
        Some(self)
    }
}

impl<E> Edited for Result<Vec<u8>, E> {
    fn text(&self) -> Option<&[u8]> {
        self.as_ref().ok().map(Vec::as_slice)
    }
}

impl<A> Edited for (Vec<u8>, A) {
    fn text(&self) -> Option<&[u8]> {
        Some(&self.0)
    }
}

/// Opens the file (making it if needed) and holds a lock on it. The file a
/// lock is held on may have been replaced by the time the lock is had: then
/// the new one is locked instead.
fn lock(path: &Path) -> io::Result<(File, PathBuf)> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && !dir.exists()
    {
        fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    loop {
        // a link is followed: the file it points to is the one changed
        let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let file = OpenOptions::new().read(true).append(true).create(true).mode(0o600).open(&real)?;
        // SAFETY: flock on a descriptor this function owns
        if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let held = file.metadata()?;
        match fs::metadata(&real) {
            Ok(now) if now.ino() == held.ino() && now.dev() == held.dev() => return Ok((file, real)),
            _ => continue,
        }
    }
}

/// Writes a new file next to the old one, with its mode, and puts it in
/// the old one's place.
fn replace(real: &Path, text: &[u8]) -> io::Result<()> {
    let mode = fs::metadata(real).map(|m| m.permissions().mode() & 0o7777).unwrap_or(0o600);
    let name = real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = real.with_file_name(format!(".{name}.lumen-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temp)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(text)?;
        file.sync_all()?;
        fs::rename(&temp, real)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const ED25519: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs";

    fn ecdsa() -> String {
        // a P-256 key in the form Android's keystore gives it: the name, the curve, a point
        let mut blob = Vec::new();
        for part in [&b"ecdsa-sha2-nistp256"[..], b"nistp256", &[4u8; 65]] {
            blob.extend_from_slice(&(part.len() as u32).to_be_bytes());
            blob.extend_from_slice(part);
        }
        format!("ecdsa-sha2-nistp256 {}", STANDARD.encode(blob))
    }

    fn other_lines() -> Vec<u8> {
        // someone's own lines, with odd spacing, a comment and a line that is no key
        b"# my keys\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs  me@laptop \r\n\nfrom=\"10.0.0.1\" ssh-rsa AAAA notakey\n".to_vec()
    }

    #[test]
    fn a_pairing_line_comes_and_goes() {
        let line = pairing_line(Path::new("/usr/bin/lumen"), "0123456789abcdef", "20261111120000", ED25519).unwrap();
        assert_eq!(
            line,
            format!(
                "command=\"'/usr/bin/lumen' pair-enroll 0123456789abcdef\",restrict,expiry-time=\"20261111120000\" {ED25519} lumen-pairing:0123456789abcdef"
            )
        );
        let text = with_line(&other_lines(), &line);
        assert!(text.starts_with(&other_lines()));
        assert_eq!(without_pairing(&text, "0123456789abcdef"), other_lines());
        // another pairing's line stays
        assert_eq!(without_pairing(&text, "ffffffffffffffff"), text);
    }

    #[test]
    fn stale_pairing_lines_go() {
        let old = pairing_line(Path::new("/bin/lumen"), "aaaaaaaaaaaaaaaa", "20261111115959", ED25519).unwrap();
        let fresh = pairing_line(Path::new("/bin/lumen"), "bbbbbbbbbbbbbbbb", "20261111120100", ED25519).unwrap();
        let text = with_line(&with_line(&other_lines(), &old), &fresh);
        let cleaned = without_stale(&text, "20261111120000");
        assert_eq!(cleaned, with_line(&other_lines(), &fresh));
    }

    #[test]
    fn a_line_goes_after_a_last_line_without_newline() {
        assert_eq!(with_line(b"ssh-ed25519 AAAA a", "x"), b"ssh-ed25519 AAAA a\nx\n");
        assert_eq!(with_line(b"", "x"), b"x\n");
    }

    #[test]
    fn quoting_for_the_shell_and_for_sshd() {
        assert_eq!(shell_quote(Path::new("/opt/my lumen/lumen")).unwrap(), "'/opt/my lumen/lumen'");
        assert_eq!(shell_quote(Path::new("/opt/it's/lumen")).unwrap(), r"'/opt/it'\''s/lumen'");
        assert!(shell_quote(Path::new("/opt/a\\b/lumen")).is_err());
        assert!(shell_quote(Path::new("/opt/a\nb/lumen")).is_err());
        let line = pairing_line(Path::new("/opt/say \"hi\"/lumen"), "0123456789abcdef", "1", ED25519).unwrap();
        assert!(
            line.starts_with(r#"command="'/opt/say \"hi\"/lumen' pair-enroll 0123456789abcdef",restrict,"#),
            "{line}"
        );
        // what sshd hands the shell is the quoted path again
        let dequoted =
            line.split_once("command=\"").unwrap().1.split("\",restrict").next().unwrap().replace("\\\"", "\"");
        assert_eq!(dequoted, "'/opt/say \"hi\"/lumen' pair-enroll 0123456789abcdef");
    }

    #[test]
    fn enrolling_puts_the_phone_in_place_of_the_pairing_line() {
        let pairing = pairing_line(Path::new("/bin/lumen"), "0123456789abcdef", "20991231000000", ED25519).unwrap();
        let text = with_line(&other_lines(), &pairing);
        let key = PhoneKey::parse(&ecdsa()).unwrap();
        let (new, name) = enrolled(&text, "0123456789abcdef", &key, "Pixel 9");
        assert_eq!(name, "Pixel 9");
        assert_eq!(new, with_line(&other_lines(), &format!("restrict,pty {} lumen-phone:Pixel 9", ecdsa())));
        assert_eq!(phones(&new), vec![Phone { name: "Pixel 9".into(), fingerprint: key.fingerprint() }]);
    }

    #[test]
    fn the_same_key_again_replaces_its_line() {
        let key = PhoneKey::parse(&ecdsa()).unwrap();
        let (once, _) = enrolled(&other_lines(), "x", &key, "Pixel 9");
        let (twice, name) = enrolled(&once, "x", &key, "Pixel 9 Pro");
        assert_eq!(name, "Pixel 9 Pro");
        assert_eq!(phones(&twice).len(), 1);
        assert!(twice.starts_with(&other_lines()));
    }

    #[test]
    fn a_name_clash_gets_a_number() {
        let first = PhoneKey::parse(&ecdsa()).unwrap();
        let second = PhoneKey::parse(ED25519).unwrap();
        let (text, _) = enrolled(b"", "x", &first, "Pixel");
        let (text, name) = enrolled(&text, "x", &second, "Pixel");
        assert_eq!(name, "Pixel 2");
        let names: Vec<String> = phones(&text).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Pixel", "Pixel 2"]);
    }

    #[test]
    fn unpairing_takes_exactly_that_name() {
        let first = PhoneKey::parse(&ecdsa()).unwrap();
        let second = PhoneKey::parse(ED25519).unwrap();
        let (text, _) = enrolled(&other_lines(), "x", &first, "Pixel");
        let (text, _) = enrolled(&text, "x", &second, "Pixel 2");
        assert_eq!(without_phone(&text, "Pix"), None);
        let rest = without_phone(&text, "Pixel").unwrap();
        let names: Vec<String> = phones(&rest).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Pixel 2"]);
        assert!(rest.starts_with(&other_lines()));
    }

    #[test]
    fn what_a_phone_may_not_send_as_its_key() {
        assert!(PhoneKey::parse(&ecdsa()).is_ok());
        assert!(PhoneKey::parse(ED25519).is_ok());
        let smuggled = [
            format!("command=\"sh\" {ED25519}"),
            format!("{ED25519} comment"),
            format!("{ED25519}\nssh-ed25519 AAAA"),
            format!("{ED25519}\n"),
            "ssh-dss AAAAB3NzaC1kc3M=".to_string(),
            // the word in front is not the type the key itself names
            ED25519.replace("ssh-ed25519", "ssh-rsa"),
            "ssh-ed25519 !!!".to_string(),
            String::new(),
        ];
        for text in smuggled {
            assert!(PhoneKey::parse(&text).is_err(), "{text:?}");
        }
        // a key with something after its parts
        let mut blob = STANDARD.decode(ED25519.split(' ').nth(1).unwrap()).unwrap();
        blob.extend_from_slice(&[0, 0, 0, 1, b'x']);
        assert!(PhoneKey::parse(&format!("ssh-ed25519 {}", STANDARD.encode(blob))).is_err());
        // the line lumen writes is made from the key, not copied
        let spaced = format!("  {}  ", ecdsa().replace(' ', "   "));
        assert_eq!(PhoneKey::parse(&spaced).unwrap().line(), ecdsa());
    }

    #[test]
    fn editing_a_file_keeps_its_mode_and_other_lines() {
        let dir = std::env::temp_dir().join(format!("lumen-authorized-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("ssh").join("authorized_keys");
        // made when missing, with the folder
        edit(&file, |text| with_line(text, "a")).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"a\n");
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(file.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        fs::write(&file, other_lines()).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        edit(&file, |text| with_line(text, "b")).unwrap();
        assert_eq!(fs::read(&file).unwrap(), with_line(&other_lines(), "b"));
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o640);
        // nothing left behind
        assert_eq!(fs::read_dir(file.parent().unwrap()).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
