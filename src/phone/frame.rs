//! What `lumen attach` (and `lumen sessions`) and `lumen keeper` say to
//! each other on the keeper's socket.
//!
//! Every message is a frame: one byte that says what it is, four bytes of
//! length (big-endian), then that many bytes. Every connection starts with
//! a [`VERSION`] frame from the client, and the keeper answers one of
//! another protocol with [`REFUSED`] and nothing else. The frame layout and
//! these two kinds stay the same in every version, so that two lumens of
//! different versions can always tell each other so.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Changes whenever anything below changes in a way the other side of an
/// older lumen would misread.
pub const PROTOCOL: u32 = 1;

/// The largest frame either side takes: a replay of a long scrollback fits.
const MAX_FRAME: usize = 64 << 20;

// Client to keeper.
/// The protocol (u32) and the lumen version (the rest, text).
pub const VERSION: u8 = 0;
/// [`Hello`], as JSON: attach to a session.
pub const HELLO: u8 = 1;
/// Bytes typed, for the session's program.
pub const INPUT: u8 = 2;
/// The client's terminal has a new size: [`encode_size`].
pub const RESIZE: u8 = 3;
/// List the sessions: answered with [`LISTING`].
pub const LIST: u8 = 4;
/// [`Kill`], as JSON: end sessions. Answered with [`LISTING`] of the ones ended.
pub const KILL: u8 = 5;

// Keeper to client.
/// [`Refused`], as JSON: this keeper speaks another protocol.
pub const REFUSED: u8 = 16;
/// The session is there and the client is attached to it.
pub const WELCOME: u8 = 17;
/// What the session's program printed (the first one is the replay).
pub const OUTPUT: u8 = 18;
/// The session's program ended: its exit status (i32).
pub const EXIT: u8 = 19;
/// A JSON array of `lumen_link::SessionInfo`.
pub const LISTING: u8 = 20;
/// Something went wrong, in words.
pub const ERROR: u8 = 21;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub session: String,
    pub cols: u16,
    pub rows: u16,
    /// The text area in pixels, or 0.
    pub width: u16,
    pub height: u16,
    /// Only for a session that is made by this hello.
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kill {
    pub name: Option<String>,
    pub all: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    pub protocol: u32,
    pub version: String,
}

pub fn write(out: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(kind);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    out.write_all(&frame)
}

/// The next frame, or `None` when the other side has closed the
/// connection between frames.
pub fn read(input: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0u8; 5];
    let mut got = 0;
    while got < header.len() {
        match input.read(&mut header[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {},
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "a frame is too large"));
    }
    let mut payload = vec![0u8; len];
    input.read_exact(&mut payload)?;
    Ok(Some((header[0], payload)))
}

pub fn version_payload() -> Vec<u8> {
    let mut payload = PROTOCOL.to_be_bytes().to_vec();
    payload.extend_from_slice(env!("CARGO_PKG_VERSION").as_bytes());
    payload
}

/// The protocol and version a [`VERSION`] frame names.
pub fn parse_version(payload: &[u8]) -> Option<(u32, String)> {
    let protocol = u32::from_be_bytes(payload.get(..4)?.try_into().ok()?);
    Some((protocol, String::from_utf8_lossy(&payload[4..]).into_owned()))
}

pub fn encode_size(cols: u16, rows: u16, width: u16, height: u16) -> Vec<u8> {
    [cols, rows, width, height].iter().flat_map(|v| v.to_be_bytes()).collect()
}

pub fn decode_size(payload: &[u8]) -> Option<(u16, u16, u16, u16)> {
    if payload.len() != 8 {
        return None;
    }
    let at = |i: usize| u16::from_be_bytes([payload[i], payload[i + 1]]);
    Some((at(0), at(2), at(4), at(6)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_come_out_as_they_went_in() {
        let mut wire = Vec::new();
        write(&mut wire, VERSION, &version_payload()).unwrap();
        write(&mut wire, INPUT, b"echo hi\r").unwrap();
        write(&mut wire, RESIZE, &encode_size(120, 40, 960, 640)).unwrap();
        write(&mut wire, OUTPUT, &[]).unwrap();
        let mut input = wire.as_slice();
        let (kind, payload) = read(&mut input).unwrap().unwrap();
        assert_eq!(kind, VERSION);
        assert_eq!(parse_version(&payload), Some((PROTOCOL, env!("CARGO_PKG_VERSION").to_string())));
        assert_eq!(read(&mut input).unwrap(), Some((INPUT, b"echo hi\r".to_vec())));
        let (kind, payload) = read(&mut input).unwrap().unwrap();
        assert_eq!((kind, decode_size(&payload)), (RESIZE, Some((120, 40, 960, 640))));
        assert_eq!(read(&mut input).unwrap(), Some((OUTPUT, Vec::new())));
        // closed between frames, and in the middle of one
        assert_eq!(read(&mut input).unwrap(), None);
        assert!(read(&mut &wire[..3]).is_err());
        assert!(read(&mut &wire[..7]).is_err());
    }

    #[test]
    fn a_frame_too_large_is_refused() {
        let mut header = vec![OUTPUT];
        header.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(read(&mut header.as_slice()).is_err());
    }

    #[test]
    fn a_hello_is_json() {
        let hello = Hello {
            session: "phone".into(),
            cols: 80,
            rows: 24,
            width: 0,
            height: 0,
            env: vec![("TERM_PROGRAM".into(), "lumen".into())],
            cwd: None,
        };
        let text = serde_json::to_vec(&hello).unwrap();
        assert_eq!(serde_json::from_slice::<Hello>(&text).unwrap(), hello);
        assert_eq!(decode_size(b"short"), None);
    }
}
