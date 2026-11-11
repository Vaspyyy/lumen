//! What lumen on a PC and lumen on a phone say to each other.
//!
//! A phone is paired once, with `lumen pair` on the PC:
//!
//! 1. The PC makes a key that is good for a few minutes and for one thing
//!    only: running `lumen pair-enroll`. It shows an [`Offer`] as a QR code:
//!    where the PC is, which keys it proves itself with, and that key.
//! 2. The phone makes its own key in its secure hardware, logs in over SSH
//!    with the key from the offer, and sends an [`EnrollRequest`] with its
//!    public key. The PC asks its user, puts the phone's key in
//!    `authorized_keys` in place of the short-lived one, and answers with an
//!    [`EnrollReply`].
//!
//! From then on the phone logs in over SSH with its own key, which never
//! leaves the phone, and runs `lumen attach` to reach a terminal that keeps
//! running on the PC while the phone is away.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What a pairing link starts with.
pub const LINK_PREFIX: &str = "lumen://pair/";

/// The command the short-lived key is allowed to run, followed by [`Offer::token`].
pub const ENROLL_COMMAND: &str = "pair-enroll";

/// What the comment of a paired phone's line in `authorized_keys` starts with.
pub const PHONE_COMMENT: &str = "lumen-phone:";

/// What the comment of the short-lived key's line starts with.
pub const PAIRING_COMMENT: &str = "lumen-pairing:";

const VERSION: u8 = 1;
const MAX_NAME: usize = 64;
const MAX_LIST: usize = 8;

/// What the QR code of `lumen pair` holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// The user to log in as.
    pub user: String,
    /// The port sshd listens on.
    pub port: u16,
    /// What to call the PC: its host name.
    pub label: String,
    /// The private half of the short-lived key (Ed25519).
    pub seed: [u8; 32],
    /// Where the PC may be reached, most likely first: addresses or names.
    pub addresses: Vec<String>,
    /// SHA-256 over each public key the PC's sshd may prove itself with.
    pub host_keys: Vec<[u8; 32]>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OfferError {
    /// Not a lumen pairing link at all.
    NotALink,
    /// A pairing link, cut short or changed on the way.
    Damaged,
    /// Made by a newer lumen than this one.
    Newer(u8),
}

impl std::fmt::Display for OfferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfferError::NotALink => write!(f, "this is not a lumen pairing code"),
            OfferError::Damaged => write!(f, "the pairing code is incomplete"),
            OfferError::Newer(v) => write!(f, "the pairing code is from a newer lumen (format {v}): update this app"),
        }
    }
}

impl std::error::Error for OfferError {}

impl Offer {
    /// Thirty-two bytes nobody can guess, for [`Offer::seed`].
    pub fn new_seed() -> [u8; 32] {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("the system has no source of randomness");
        seed
    }

    /// The link a QR code shows.
    pub fn to_link(&self) -> String {
        let mut out = vec![VERSION];
        out.extend_from_slice(&self.port.to_be_bytes());
        put_text(&mut out, &self.user);
        put_text(&mut out, &self.label);
        out.extend_from_slice(&self.seed);
        let addresses = &self.addresses[..self.addresses.len().min(MAX_LIST)];
        out.push(addresses.len() as u8);
        for address in addresses {
            put_text(&mut out, address);
        }
        let keys = &self.host_keys[..self.host_keys.len().min(MAX_LIST)];
        out.push(keys.len() as u8);
        for key in keys {
            out.extend_from_slice(key);
        }
        format!("{LINK_PREFIX}{}", URL_SAFE_NO_PAD.encode(out))
    }

    pub fn from_link(link: &str) -> Result<Offer, OfferError> {
        let body = link.trim().strip_prefix(LINK_PREFIX).ok_or(OfferError::NotALink)?;
        let bytes = URL_SAFE_NO_PAD.decode(body.trim_end_matches('=')).map_err(|_| OfferError::Damaged)?;
        let mut rest = bytes.as_slice();
        let version = take(&mut rest, 1)?[0];
        if version > VERSION {
            return Err(OfferError::Newer(version));
        }
        let port = u16::from_be_bytes(take(&mut rest, 2)?.try_into().unwrap());
        let user = take_text(&mut rest)?;
        let label = take_text(&mut rest)?;
        let seed: [u8; 32] = take(&mut rest, 32)?.try_into().unwrap();
        let count = take(&mut rest, 1)?[0] as usize;
        let mut addresses = Vec::new();
        for _ in 0..count {
            addresses.push(take_text(&mut rest)?);
        }
        let count = take(&mut rest, 1)?[0] as usize;
        let mut host_keys = Vec::new();
        for _ in 0..count {
            host_keys.push(take(&mut rest, 32)?.try_into().unwrap());
        }
        if user.is_empty() || port == 0 || addresses.is_empty() || host_keys.is_empty() || !rest.is_empty() {
            return Err(OfferError::Damaged);
        }
        Ok(Offer { user, port, label, seed, addresses, host_keys })
    }

    /// The public half of the short-lived key, as a line of `authorized_keys`
    /// without options or comment.
    pub fn public_key_line(&self) -> String {
        let public = ed25519_dalek::SigningKey::from_bytes(&self.seed).verifying_key().to_bytes();
        let mut blob = Vec::new();
        put_string(&mut blob, b"ssh-ed25519");
        put_string(&mut blob, &public);
        format!("ssh-ed25519 {}", STANDARD.encode(blob))
    }

    /// Names this pairing on the PC: in the command the short-lived key runs
    /// and in the name of the socket `lumen pair` waits on. Says nothing
    /// about the key itself.
    pub fn token(&self) -> String {
        let public = ed25519_dalek::SigningKey::from_bytes(&self.seed).verifying_key().to_bytes();
        Sha256::digest(public)[..8].iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn put_text(out: &mut Vec<u8>, text: &str) {
    let mut end = text.len().min(MAX_NAME);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    out.push(end as u8);
    out.extend_from_slice(&text.as_bytes()[..end]);
}

fn put_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn take<'a>(rest: &mut &'a [u8], count: usize) -> Result<&'a [u8], OfferError> {
    if rest.len() < count {
        return Err(OfferError::Damaged);
    }
    let (head, tail) = rest.split_at(count);
    *rest = tail;
    Ok(head)
}

fn take_text(rest: &mut &[u8]) -> Result<String, OfferError> {
    let len = take(rest, 1)?[0] as usize;
    String::from_utf8(take(rest, len)?.to_vec()).map_err(|_| OfferError::Damaged)
}

/// SHA-256 over the key in a public key line (`ssh-ed25519 AAAA... comment`,
/// options in front allowed): what [`Offer::host_keys`] holds.
pub fn fingerprint(line: &str) -> Option<[u8; 32]> {
    let blob = key_blob(line)?;
    Some(Sha256::digest(blob).into())
}

/// A fingerprint the way `ssh-keygen -l` prints it.
pub fn fingerprint_text(fingerprint: &[u8; 32]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(fingerprint))
}

/// The key of a public key line, decoded. The word in front of it has to be
/// the name the key itself starts with, so that options are not mistaken for
/// a key.
pub fn key_blob(line: &str) -> Option<Vec<u8>> {
    let mut words = line.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let Some(next) = words.peek() else { break };
        let Ok(blob) = STANDARD.decode(next) else { continue };
        let named = blob.get(..4).map(|len| u32::from_be_bytes(len.try_into().unwrap()) as usize);
        if named.and_then(|len| blob.get(4..4 + len)) == Some(word.as_bytes()) {
            return Some(blob);
        }
    }
    None
}

/// The phone's first message, as one line of JSON on the standard input of
/// `lumen pair-enroll`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollRequest {
    /// What the phone calls itself, for the PC's user to recognize.
    pub device: String,
    /// The phone's own public key, as a line of `authorized_keys` without
    /// options or comment.
    pub public_key: String,
}

/// The PC's answer, as one line of JSON on the standard output.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollReply {
    pub ok: bool,
    /// Why not, in words for the phone's user.
    #[serde(default)]
    pub error: String,
    /// Where the lumen program is on the PC, for `lumen attach`.
    #[serde(default)]
    pub lumen: String,
    /// The PC's lumen version.
    #[serde(default)]
    pub version: String,
}

/// One terminal the PC keeps running, as `lumen sessions --json` lists it
/// (an array of these, on one line).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub name: String,
    /// The title its program set, or empty.
    #[serde(default)]
    pub title: String,
    /// The folder its shell is in, or empty.
    #[serde(default)]
    pub cwd: String,
    /// How many terminals show it right now.
    #[serde(default)]
    pub attached: u32,
    /// When it was started, in seconds since 1970.
    #[serde(default)]
    pub created: u64,
}

/// A name somebody else chose, made safe to print and to keep in a file of
/// lines: what can be seen stays, at most 48 characters of it.
pub fn clean_name(name: &str) -> String {
    // what can't be seen but turns text around or hides it: a name shown next to
    // a question must read as it is
    let steers =
        |c: char| matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}');
    let cleaned: String = name
        .chars()
        .filter(|&c| !steers(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(48)
        .collect();
    if cleaned.is_empty() { "phone".into() } else { cleaned }
}

/// Whether a session can be called this: letters, digits, `-`, `_` and `.`,
/// at most 64 of them, not starting with a `-` or a `.`. A name goes on a
/// command line and into a file name.
pub fn session_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['-', '.'])
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> Offer {
        Offer {
            user: "ada".into(),
            port: 22,
            label: "desk".into(),
            seed: [7; 32],
            addresses: vec!["100.64.0.7".into(), "192.168.1.20".into(), "desk.example.ts.net".into()],
            host_keys: vec![[1; 32], [2; 32]],
        }
    }

    #[test]
    fn a_link_holds_the_whole_offer() {
        let link = offer().to_link();
        assert!(link.starts_with(LINK_PREFIX));
        assert_eq!(Offer::from_link(&link), Ok(offer()));
        // as it comes out of a QR scanner or a paste: with space around it
        assert_eq!(Offer::from_link(&format!("  {link}\n")), Ok(offer()));
        // short enough for a QR code a terminal can show
        assert!(link.len() < 300, "{} characters", link.len());
    }

    #[test]
    fn what_is_not_an_offer() {
        assert_eq!(Offer::from_link("https://example.com"), Err(OfferError::NotALink));
        assert_eq!(Offer::from_link("lumen://pair/"), Err(OfferError::Damaged));
        assert_eq!(Offer::from_link("lumen://pair/!!!"), Err(OfferError::Damaged));
        let link = offer().to_link();
        assert_eq!(Offer::from_link(&link[..link.len() - 6]), Err(OfferError::Damaged));
        // something after the end
        let longer = format!("{link}AAAA");
        assert_eq!(Offer::from_link(&longer), Err(OfferError::Damaged));
        // a format this lumen doesn't know yet
        let newer = format!("{LINK_PREFIX}{}", URL_SAFE_NO_PAD.encode([9u8, 0, 22]));
        assert_eq!(Offer::from_link(&newer), Err(OfferError::Newer(9)));
        // nowhere to go, nothing to check the PC by
        let mut nowhere = offer();
        nowhere.addresses.clear();
        assert_eq!(Offer::from_link(&nowhere.to_link()), Err(OfferError::Damaged));
        let mut unchecked = offer();
        unchecked.host_keys.clear();
        assert_eq!(Offer::from_link(&unchecked.to_link()), Err(OfferError::Damaged));
    }

    #[test]
    fn the_short_lived_key() {
        let line = offer().public_key_line();
        // the key ssh-keygen derives from the same seed
        assert_eq!(line, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs");
        assert_eq!(offer().token().len(), 16);
        assert_ne!(offer().token(), Offer { seed: [8; 32], ..offer() }.token());
    }

    #[test]
    fn fingerprints() {
        let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs someone@somewhere";
        let print = fingerprint(line).unwrap();
        assert_eq!(fingerprint_text(&print).len(), "SHA256:".len() + 43);
        // options in front, as in authorized_keys, change nothing
        let with_options = format!("restrict,pty,command=\"x y\" {line}");
        assert_eq!(fingerprint(&with_options), Some(print));
        assert_eq!(fingerprint("no key here"), None);
        assert_eq!(fingerprint(""), None);
        // a word that decodes but is not the key the word before it names
        assert_eq!(fingerprint("ssh-rsa AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs"), None);
    }

    #[test]
    fn names() {
        assert_eq!(clean_name("Galaxy S24 Ultra"), "Galaxy S24 Ultra");
        assert_eq!(clean_name("a\nb\u{7}c"), "a b c");
        // a name can't be written backwards to read as another
        assert_eq!(clean_name("phone\u{202e}enohp\u{200b}"), "phoneenohp");
        assert_eq!(clean_name("  \n "), "phone");
        assert_eq!(clean_name(&"x".repeat(200)).len(), 48);
        assert!(session_name_ok("phone-1"));
        assert!(session_name_ok("galaxy_s24.2"));
        for bad in ["", "-rf", ".hidden", "a b", "a/b", "a;b", "ä"] {
            assert!(!session_name_ok(bad), "{bad:?}");
        }
        assert!(!session_name_ok(&"x".repeat(65)));
    }

    #[test]
    fn messages_are_one_line_each() {
        let request =
            EnrollRequest { device: "Galaxy S24 Ultra".into(), public_key: "ecdsa-sha2-nistp256 AAAA".into() };
        let line = serde_json::to_string(&request).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<EnrollRequest>(&line).unwrap(), request);
        // an answer from an older PC: what it doesn't say is empty
        let reply: EnrollReply = serde_json::from_str(r#"{"ok":false,"error":"no"}"#).unwrap();
        assert_eq!(reply, EnrollReply { ok: false, error: "no".into(), ..Default::default() });
    }
}
