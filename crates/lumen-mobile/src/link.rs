//! The connection to the PC: SSH, to the system's own sshd.
//!
//! Pairing logs in once with the short-lived key from the QR code and hands
//! over the phone's public key ([`enroll`]). After that every login is
//! signed by the phone's key, which this code never sees: it asks a
//! [`Sign`] for a signature and the phone's secure hardware makes it.
//!
//! The PC proves itself too: only a server holding one of the host keys
//! whose fingerprints came with the pairing is talked to.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lumen_link::{EnrollReply, EnrollRequest, Offer, SessionInfo};
use russh::ChannelMsg;
use russh::client::{self, Handle};
use russh::keys::agent::AgentIdentity;
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::screen::{Out, Screen, Size};

/// How long one address gets to answer.
const REACH: Duration = Duration::from_secs(6);
/// How long the greeting and the key exchange may take.
const HANDSHAKE: Duration = Duration::from_secs(12);
/// How long a small command on the PC may take.
const COMMAND: Duration = Duration::from_secs(10);
/// How long the PC's user has to say yes to a new phone.
const CONFIRM: Duration = Duration::from_secs(230);
/// The most text a program may put on the phone's clipboard at once.
const CLIPBOARD: usize = 1 << 20;
/// A terminal whose program is gone this soon, and not with a zero, never
/// started: what it said stays on screen.
const STILLBORN: Duration = Duration::from_secs(3);

/// A paired PC, as the app stores it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    /// Names this pairing on the phone.
    pub id: String,
    pub label: String,
    pub user: String,
    pub port: u16,
    pub addresses: Vec<String>,
    /// SHA-256 over each host key the PC may show, in hex.
    pub host_keys: Vec<String>,
    /// Where lumen is on the PC, or empty.
    #[serde(default)]
    pub lumen: String,
    /// The PC's lumen version when it was paired.
    #[serde(default)]
    pub version: String,
    /// The phone's public key for this PC.
    pub public_key: String,
}

/// Why something did not work, for the app to act on (`code`) and for the
/// user to read (`message`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}

impl Failure {
    fn new(code: &'static str, message: impl Into<String>) -> Failure {
        Failure { code, message: message.into() }
    }

    fn failed(error: impl std::fmt::Display) -> Failure {
        Failure::new("failed", error.to_string())
    }
}

/// Signs with the phone's key.
pub trait Sign: Send + Sync + 'static {
    /// The DER-encoded ECDSA signature over SHA-256 of `data`, or `None` if
    /// the phone's owner did not agree.
    fn sign(&self, data: &[u8]) -> Option<Vec<u8>>;
}

/// What a terminal tells the app.
pub trait TabEvents: Send + Sync + 'static {
    fn dirty(&self);
    fn title(&self, title: &str);
    fn bell(&self);
    fn clipboard(&self, text: &str);
    fn ended(&self, status: i32);
    fn lost(&self, reason: &str);
}

/// Lets a server through only if it shows a host key that came with the
/// pairing.
struct Checker {
    host_keys: Vec<[u8; 32]>,
    wrong: Arc<AtomicBool>,
}

impl client::Handler for Checker {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let shown = key.public_key().fingerprint(HashAlg::Sha256);
        let known = self.host_keys.iter().any(|known| known[..] == *shown.as_bytes());
        if !known {
            self.wrong.store(true, Ordering::Release);
        }
        Ok(known)
    }
}

/// The PC, at the first of its addresses where it answers and proves to be
/// itself. All are tried at once, the earlier ones with a head start: at
/// home the LAN address wins, on the road the VPN's.
///
/// An address may belong to another machine where the phone is right now (a
/// friend's network uses the same numbers as the one at home). Such a machine
/// shows another key and is dropped before anything is said to it, and the
/// other addresses still get their turn.
async fn arrive(addresses: &[String], port: u16, host_keys: &[[u8; 32]]) -> Result<(Handle<Checker>, String), Failure> {
    let mut tries = tokio::task::JoinSet::new();
    for (i, address) in addresses.iter().enumerate() {
        let (address, host_keys) = (address.clone(), host_keys.to_vec());
        tries.spawn(async move {
            tokio::time::sleep(Duration::from_millis(200 * i as u64)).await;
            let stream = timeout(REACH, TcpStream::connect((address.as_str(), port))).await;
            let Some(stream) = stream.ok().and_then(Result::ok) else {
                return Err((Failure::new("unreachable", ""), address));
            };
            let _ = stream.set_nodelay(true);
            match handshake(stream, host_keys).await {
                Ok(handle) => Ok((handle, address)),
                Err(failure) => Err((failure, address)),
            }
        });
    }
    let (mut silent, mut strangers, mut odd) = (0, Vec::new(), None);
    while let Some(tried) = tries.join_next().await {
        match tried {
            Ok(Ok(arrived)) => return Ok(arrived),
            Ok(Err((failure, address))) => match failure.code {
                "wrong-host" => strangers.push(address),
                "unreachable" => silent += 1,
                _ => odd = Some(failure),
            },
            Err(_) => silent += 1,
        }
    }
    let network = "Is the PC on, and is this phone in its network or VPN?";
    Err(match (strangers.as_slice(), silent, odd) {
        // every address is somebody else's: the PC changed its keys, or somebody stands in for it
        ([_, ..], 0, None) => Failure::new(
            "wrong-host",
            "This is not the PC this phone was paired with: it shows another key. Nothing was sent to it.",
        ),
        ([stranger, ..], ..) => Failure::new(
            "unreachable",
            format!("The PC does not answer. Another machine answers at {stranger}; nothing was sent to it. {network}"),
        ),
        ([], _, Some(odd)) => odd,
        ([], ..) => Failure::new("unreachable", format!("The PC does not answer. {network}")),
    })
}

async fn handshake(stream: TcpStream, host_keys: Vec<[u8; 32]>) -> Result<Handle<Checker>, Failure> {
    let config = Arc::new(client::Config {
        // a link that went away without a word is noticed within the minute
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });
    let wrong = Arc::new(AtomicBool::new(false));
    let checker = Checker { host_keys, wrong: wrong.clone() };
    match timeout(HANDSHAKE, client::connect_stream(config, stream, checker)).await {
        Ok(Ok(handle)) => Ok(handle),
        _ if wrong.load(Ordering::Acquire) => Err(Failure::new("wrong-host", "")),
        Ok(Err(e)) => Err(Failure::failed(format!("The PC answered, but not as expected: {e}"))),
        Err(_) => Err(Failure::new("unreachable", "")),
    }
}

fn prints(host_keys: &[String]) -> Vec<[u8; 32]> {
    host_keys
        .iter()
        .filter_map(|hex| {
            let bytes: Option<Vec<u8>> =
                (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect();
            bytes?.try_into().ok()
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A word for a shell's command line, whichever shell: fish, bash and zsh
/// all take single quotes, with a quote itself stepped out of them.
fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// Runs a small command on the PC and takes what it prints.
async fn run(handle: &Handle<Checker>, command: &str) -> Option<(u32, Vec<u8>)> {
    let command = command.to_string();
    let work = async {
        let mut channel = handle.channel_open_session().await.ok()?;
        channel.exec(true, command).await.ok()?;
        let (mut out, mut status) = (Vec::new(), None);
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } if out.len() < 1 << 20 => out.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Failure => return None,
                _ => {},
            }
        }
        Some((status?, out))
    };
    timeout(COMMAND, work).await.ok().flatten()
}

/// Introduces the phone's key to the PC with a pairing code. Waits for the
/// PC's user to say yes: `asked` is called once the PC has the phone's key
/// and its user is being asked.
pub async fn enroll(link: &str, device: &str, public_key: &str, asked: impl Fn()) -> Result<Host, Failure> {
    let offer = Offer::from_link(link).map_err(Failure::failed)?;
    let (mut handle, _) = arrive(&offer.addresses, offer.port, &offer.host_keys).await?;

    let stale = || Failure::new("refused", "The PC no longer takes this code. Run lumen pair there again.");
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&offer.seed));
    let key = PrivateKeyWithHashAlg::new(Arc::new(key), None);
    match timeout(HANDSHAKE, handle.authenticate_publickey(offer.user.as_str(), key)).await {
        Ok(Ok(result)) if result.success() => {},
        Ok(Ok(_)) => return Err(stale()),
        Ok(Err(e)) => return Err(Failure::failed(e)),
        Err(_) => return Err(Failure::new("unreachable", "The PC took too long to answer.")),
    }

    let request = EnrollRequest {
        device: lumen_link::clean_name(device),
        public_key: public_key.trim().to_string(),
        ..Default::default()
    };
    let mut line = serde_json::to_vec(&request).map_err(Failure::failed)?;
    line.push(b'\n');
    let answer = async {
        let mut channel = handle.channel_open_session().await?;
        // whatever is asked for, the short-lived key runs `lumen pair-enroll`
        channel.exec(true, "lumen pair-enroll").await?;
        channel.data_bytes(line).await?;
        asked();
        let mut out = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } if out.len() < 1 << 16 => {
                    out.extend_from_slice(&data);
                    if answer_in(&out).is_some() {
                        break;
                    }
                },
                ChannelMsg::Failure => break,
                _ => {},
            }
        }
        Ok::<_, russh::Error>(out)
    };
    let out = match timeout(CONFIRM, answer).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(Failure::failed(e)),
        Err(_) => return Err(Failure::new("denied", "Nobody said yes on the PC in time. Run lumen pair there again.")),
    };
    let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;

    let reply = answer_in(&out).ok_or_else(|| {
        Failure::failed("The PC did not answer the way lumen does. Is lumen pair still waiting there?")
    })?;
    if !reply.ok {
        let said = reply.error.trim();
        return Err(Failure::new("denied", if said.is_empty() { "The PC said no." } else { said }));
    }
    Ok(Host {
        id: hex(&Offer::new_seed()[..8]),
        label: offer.label,
        user: offer.user,
        port: offer.port,
        addresses: offer.addresses,
        host_keys: offer.host_keys.iter().map(|key| hex(key)).collect(),
        lumen: reply.lumen,
        version: reply.version,
        public_key: request.public_key,
    })
}

#[derive(Debug)]
enum SignFailure {
    /// The connection went away meanwhile.
    Gone,
    /// The phone's owner did not agree.
    Declined,
    /// What came back is not a signature.
    Garbled,
}

impl From<russh::SendError> for SignFailure {
    fn from(_: russh::SendError) -> Self {
        SignFailure::Gone
    }
}

struct Remote(Arc<dyn Sign>);

impl russh::Signer for Remote {
    type Error = SignFailure;

    async fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        _hash: Option<HashAlg>,
        mut to_sign: Vec<u8>,
    ) -> Result<Vec<u8>, SignFailure> {
        let sign = self.0.clone();
        let data = to_sign.clone();
        // signing may wait for a finger on the sensor
        let der = tokio::task::spawn_blocking(move || sign.sign(&data)).await.map_err(|_| SignFailure::Garbled)?;
        let der = der.ok_or(SignFailure::Declined)?;
        append_signature(&mut to_sign, &der).ok_or(SignFailure::Garbled)?;
        Ok(to_sign)
    }
}

/// Adds an ECDSA signature the way SSH wants it to what was signed. The
/// phone's key store gives it as DER: a sequence of the two numbers r and s.
fn append_signature(out: &mut Vec<u8>, der: &[u8]) -> Option<()> {
    let (r, s) = der_numbers(der)?;
    let name = b"ecdsa-sha2-nistp256";
    let mut numbers = Vec::new();
    put_string(&mut numbers, &r);
    put_string(&mut numbers, &s);
    out.extend_from_slice(&((4 + name.len() + 4 + numbers.len()) as u32).to_be_bytes());
    put_string(out, name);
    put_string(out, &numbers);
    Some(())
}

fn put_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// The two numbers of a DER-encoded ECDSA signature, each as SSH writes a
/// number: no needless zeros in front, one zero if it would read as negative.
fn der_numbers(der: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    fn length(bytes: &mut &[u8]) -> Option<usize> {
        let (&first, rest) = bytes.split_first()?;
        *bytes = rest;
        if first < 0x80 {
            return Some(first as usize);
        }
        let count = (first & 0x7f) as usize;
        if count == 0 || count > 2 || bytes.len() < count {
            return None;
        }
        let len = bytes[..count].iter().fold(0usize, |len, &b| len << 8 | b as usize);
        *bytes = &bytes[count..];
        Some(len)
    }
    fn number(bytes: &mut &[u8]) -> Option<Vec<u8>> {
        let (&tag, rest) = bytes.split_first()?;
        *bytes = rest;
        let len = length(bytes)?;
        if tag != 0x02 || len == 0 || bytes.len() < len {
            return None;
        }
        let (digits, rest) = bytes.split_at(len);
        *bytes = rest;
        let digits = &digits[digits.iter().position(|&b| b != 0).unwrap_or(len - 1)..];
        let mut number = if digits[0] >= 0x80 { vec![0] } else { Vec::new() };
        number.extend_from_slice(digits);
        Some(number)
    }
    let mut rest = der;
    let (&tag, after) = rest.split_first()?;
    rest = after;
    if tag != 0x30 || length(&mut rest)? != rest.len() {
        return None;
    }
    let r = number(&mut rest)?;
    let s = number(&mut rest)?;
    rest.is_empty().then_some((r, s))
}

/// The `authorized_keys` form of a P-256 public key given the way Java
/// hands it out (X.509 SubjectPublicKeyInfo).
pub fn public_key_line(spki: &[u8]) -> Option<String> {
    use base64::Engine;
    // an EC key on the curve P-256, then the point, uncompressed
    const HEAD: [u8; 26] = [
        0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48,
        0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
    ];
    let point = spki.strip_prefix(&HEAD[..]).filter(|point| point.len() == 65 && point[0] == 0x04)?;
    let mut blob = Vec::new();
    put_string(&mut blob, b"ecdsa-sha2-nistp256");
    put_string(&mut blob, b"nistp256");
    put_string(&mut blob, point);
    Some(format!("ecdsa-sha2-nistp256 {}", base64::engine::general_purpose::STANDARD.encode(blob)))
}

/// A connection to a paired PC that is logged in.
pub struct Link {
    handle: Handle<Checker>,
    /// How to call lumen on the PC, ready for a command line. `None`: the
    /// PC has no lumen (any more), and terminals are plain shells that end
    /// with the connection.
    lumen: Option<String>,
    /// The address that answered.
    pub address: String,
}

/// Connects to a paired PC and logs in with the phone's key.
pub async fn connect(host: &Host, sign: Arc<dyn Sign>) -> Result<Arc<Link>, Failure> {
    let public = PublicKey::from_openssh(&host.public_key)
        .map_err(|_| Failure::failed("This phone's record of its own key is damaged. Pair the PC again."))?;
    let (mut handle, address) = arrive(&host.addresses, host.port, &prints(&host.host_keys)).await?;

    let mut remote = Remote(sign);
    match handle.authenticate_publickey_with(host.user.as_str(), public, None, &mut remote).await {
        Ok(result) if result.success() => {},
        Ok(_) => {
            return Err(Failure::new(
                "refused",
                format!("{} no longer accepts this phone. It may have been unpaired there.", host.label),
            ));
        },
        Err(SignFailure::Declined) => return Err(Failure::new("cancelled", "Not unlocked.")),
        Err(SignFailure::Garbled) => return Err(Failure::failed("This phone's key gave no usable signature.")),
        Err(SignFailure::Gone) => return Err(Failure::new("unreachable", "The connection to the PC was lost.")),
    }

    // Is lumen there, to keep terminals running? Where it was when the phone
    // was paired, or wherever the login shell finds it now.
    let mut lumen = None;
    let usable = !host.lumen.is_empty() && !host.lumen.contains(['\\', '\n']);
    let stored = usable.then(|| quoted(&host.lumen));
    for candidate in stored.into_iter().chain(["lumen".to_string()]) {
        let listed = run(&handle, &format!("{candidate} sessions --json")).await;
        if listed.is_some_and(|(status, out)| status == 0 && sessions_of(&out).is_some()) {
            lumen = Some(candidate);
            break;
        }
    }
    Ok(Arc::new(Link { handle, lumen, address }))
}

/// The list in what `lumen sessions --json` printed. A login shell may say
/// things of its own before a command runs (a greeting in its startup file):
/// the list is the last line that is one.
fn sessions_of(out: &[u8]) -> Option<Vec<SessionInfo>> {
    out.split(|&b| b == b'\n').rev().find_map(|line| serde_json::from_slice(line).ok())
}

/// The answer in what `lumen pair-enroll` printed so far: the first whole
/// line that is one, whatever a login shell said before it.
fn answer_in(out: &[u8]) -> Option<EnrollReply> {
    let whole = &out[..out.iter().rposition(|&b| b == b'\n')?];
    whole.split(|&b| b == b'\n').find_map(|line| serde_json::from_slice(line).ok())
}

impl Link {
    /// Whether the PC keeps terminals running while the phone is away.
    pub fn keeps(&self) -> bool {
        self.lumen.is_some()
    }

    /// Whether the PC still answers.
    pub async fn alive(&self, within: Duration) -> bool {
        if self.handle.is_closed() {
            return false;
        }
        // opening a channel is something every server answers
        match timeout(within, self.handle.channel_open_session()).await {
            Ok(Ok(channel)) => {
                let _ = channel.close().await;
                true
            },
            _ => false,
        }
    }

    /// The terminals the PC keeps.
    pub async fn sessions(&self) -> Vec<SessionInfo> {
        let Some(lumen) = &self.lumen else { return Vec::new() };
        let listed = run(&self.handle, &format!("{lumen} sessions --json")).await;
        listed.and_then(|(_, out)| sessions_of(&out)).unwrap_or_default()
    }

    /// Ends a terminal the PC keeps.
    pub async fn kill(&self, name: &str) {
        if let Some(lumen) = self.lumen.as_ref().filter(|_| lumen_link::session_name_ok(name)) {
            run(&self.handle, &format!("{lumen} sessions kill {name}")).await;
        }
    }

    pub async fn disconnect(&self) {
        let _ = self.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;
    }
}

enum Input {
    Data(Vec<u8>),
    Resize(Size),
    Close,
}

/// A terminal: its screen here, its program on the PC.
pub struct Tab {
    pub screen: Screen,
    input: mpsc::UnboundedSender<Input>,
    events: Arc<dyn TabEvents>,
    /// Set when the app let go of the tab: nothing is told about it after.
    closed: AtomicBool,
}

impl Tab {
    fn new(size: Size, events: Arc<dyn TabEvents>) -> (Arc<Tab>, mpsc::UnboundedReceiver<Input>) {
        let (input, rx) = mpsc::unbounded_channel();
        (Arc::new(Tab { screen: Screen::new(size), input, events, closed: AtomicBool::new(false) }), rx)
    }

    /// Sends bytes to the program, as if typed.
    pub fn send(&self, bytes: Vec<u8>) {
        if !bytes.is_empty() {
            let _ = self.input.send(Input::Data(bytes));
        }
    }

    pub fn resize(&self, size: Size) {
        if self.screen.resize(size) {
            let _ = self.input.send(Input::Resize(size));
            self.changed();
        }
    }

    /// Lets go of the tab. The terminal on the PC stays.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let _ = self.input.send(Input::Close);
    }

    /// Tells the app to draw, unless it was told and has not drawn yet.
    pub fn changed(&self) {
        if self.screen.touch() && !self.closed.load(Ordering::Acquire) {
            self.events.dirty();
        }
    }

    /// Takes in output and passes on what the terminal has to say.
    fn output(&self, bytes: &[u8]) {
        for out in self.screen.advance(bytes) {
            if self.closed.load(Ordering::Acquire) {
                break;
            }
            match out {
                Out::Reply(bytes) => self.send(bytes),
                // a program names its window, it does not fill the phone's memory
                Out::Title(title) => self.events.title(&title.chars().take(200).collect::<String>()),
                Out::Bell => self.events.bell(),
                Out::Clipboard(text) if text.len() <= CLIPBOARD => self.events.clipboard(&text),
                Out::Clipboard(_) => {},
            }
        }
        self.changed();
    }

    /// Says something of lumen's own on the terminal.
    fn notice(&self, text: &str) {
        self.output(format!("\r\n\x1b[0;38;5;245m{text}\x1b[0m\r\n").as_bytes());
    }
}

/// Opens a terminal on the link: the kept terminal `name` if the PC keeps
/// terminals (started if there is none of that name), a plain shell
/// otherwise. Returns at once; the screen fills in as the PC answers.
pub fn open(link: Arc<Link>, name: &str, size: Size, events: Arc<dyn TabEvents>) -> Arc<Tab> {
    let (tab, rx) = Tab::new(size, events);
    let name = name.to_string();
    crate::runtime().spawn(serve(link, tab.clone(), name, size, rx));
    tab
}

async fn serve(link: Arc<Link>, tab: Arc<Tab>, name: String, size: Size, mut rx: mpsc::UnboundedReceiver<Input>) {
    let lost = |reason: &str| {
        if !tab.closed.load(Ordering::Acquire) {
            tab.events.lost(reason);
        }
    };
    let started = async {
        let channel = link.handle.channel_open_session().await?;
        let (width, height) = (size.cols as u32 * size.cell_width as u32, size.rows as u32 * size.cell_height as u32);
        channel.request_pty(false, "xterm-256color", size.cols as u32, size.rows as u32, width, height, &[]).await?;
        match link.lumen.as_ref().filter(|_| lumen_link::session_name_ok(&name)) {
            Some(lumen) => channel.exec(true, format!("{lumen} attach --session {name}")).await?,
            None => channel.request_shell(true).await?,
        }
        Ok::<_, russh::Error>(channel)
    };
    let channel = match timeout(COMMAND, started).await {
        Ok(Ok(channel)) => channel,
        _ => return lost("The connection to the PC was lost."),
    };
    let (mut read, write) = channel.split();

    // what is typed goes out on its own, so that a flood of output and a
    // large paste can't wait on each other
    let typing = tokio::spawn(async move {
        while let Some(input) = rx.recv().await {
            let sent = match input {
                Input::Data(bytes) => write.data_bytes(bytes).await,
                Input::Resize(size) => {
                    let (width, height) =
                        (size.cols as u32 * size.cell_width as u32, size.rows as u32 * size.cell_height as u32);
                    write.window_change(size.cols as u32, size.rows as u32, width, height).await
                },
                Input::Close => {
                    let _ = write.close().await;
                    break;
                },
            };
            if sent.is_err() {
                break;
            }
        }
    });

    let began = tokio::time::Instant::now();
    let mut status = None;
    while let Some(msg) = read.wait().await {
        match msg {
            ChannelMsg::Data { data } => tab.output(&data),
            ChannelMsg::ExtendedData { data, .. } => tab.output(&data),
            ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status as i32),
            ChannelMsg::ExitSignal { .. } => status = status.or(Some(1)),
            ChannelMsg::Failure => status = status.or(Some(127)),
            _ => {},
        }
    }
    typing.abort();
    if tab.closed.load(Ordering::Acquire) {
        return;
    }
    match status {
        Some(status) if status != 0 && began.elapsed() < STILLBORN => {
            // what the PC said about it is on screen: leave it there to be read
            tab.notice(&format!("This terminal could not be started (status {status})."));
        },
        Some(status) => tab.events.ended(status),
        None => lost("The connection to the PC was lost."),
    }
}

/// A terminal with no PC behind it: a sample screen that echoes what is
/// typed. For building and showing the app.
pub fn demo(size: Size, events: Arc<dyn TabEvents>) -> Arc<Tab> {
    let (tab, mut rx) = Tab::new(size, events);
    let shown = tab.clone();
    crate::runtime().spawn(async move {
        const PROMPT: &str = "\x1b[38;2;255;201;122m❯\x1b[0m ";
        shown.output(include_str!("demo.txt").replace('\n', "\r\n").as_bytes());
        shown.output(PROMPT.as_bytes());
        while let Some(input) = rx.recv().await {
            match input {
                Input::Data(bytes) => {
                    for &byte in &bytes {
                        match byte {
                            b'\r' => shown.output(format!("\r\n{PROMPT}").as_bytes()),
                            0x7f => shown.output(b"\x08 \x08"),
                            0x04 => {
                                if !shown.closed.load(Ordering::Acquire) {
                                    shown.events.ended(0);
                                }
                                return;
                            },
                            0x20.. => shown.output(&[byte]),
                            _ => {},
                        }
                    }
                },
                Input::Resize(_) => {},
                Input::Close => return,
            }
        }
    });
    tab
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_the_way_ssh_wants_it() {
        // r has its top bit set and so keeps its zero, s has none to spare
        let der = [0x30, 0x08, 0x02, 0x02, 0x00, 0x81, 0x02, 0x02, 0x12, 0x34];
        assert_eq!(der_numbers(&der), Some((vec![0x00, 0x81], vec![0x12, 0x34])));
        let mut out = b"signed".to_vec();
        append_signature(&mut out, &der).unwrap();
        let mut want = b"signed".to_vec();
        want.extend_from_slice(&[0, 0, 0, 39]);
        want.extend_from_slice(&[0, 0, 0, 19]);
        want.extend_from_slice(b"ecdsa-sha2-nistp256");
        want.extend_from_slice(&[0, 0, 0, 12, 0, 0, 0, 2, 0x00, 0x81, 0, 0, 0, 2, 0x12, 0x34]);
        assert_eq!(out, want);
        // a needless zero in front goes, a missing one is put there
        let der = [0x30, 0x09, 0x02, 0x03, 0x00, 0x00, 0x7f, 0x02, 0x02, 0x80, 0x01];
        assert_eq!(der_numbers(&der), Some((vec![0x7f], vec![0x00, 0x80, 0x01])));
    }

    #[test]
    fn what_is_not_a_signature() {
        for der in [
            &[][..],
            &[0x30],
            &[0x30, 0x02, 0x02, 0x00],
            // shorter than it says
            &[0x30, 0x08, 0x02, 0x02, 0x00, 0x81, 0x02, 0x02, 0x12],
            // something after the end
            &[0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01, 0x00],
            // not numbers
            &[0x30, 0x06, 0x04, 0x01, 0x01, 0x02, 0x01, 0x01],
        ] {
            assert_eq!(der_numbers(der), None, "{der:x?}");
        }
    }

    #[test]
    fn the_public_key_line() {
        let mut spki = vec![
            0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48,
            0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
        ];
        spki.extend_from_slice(&[7; 64]);
        let line = public_key_line(&spki).unwrap();
        assert!(line.starts_with("ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBAcH"));
        // the pairing side reads it as a key
        assert!(lumen_link::key_blob(&line).is_some());
        // another curve, a cut key
        assert_eq!(public_key_line(&spki[..spki.len() - 1]), None);
        spki[20] = 0x04;
        assert_eq!(public_key_line(&spki), None);
    }

    #[test]
    fn answers_among_what_a_shell_says() {
        let listed = br#"Welcome back!
[{"name":"t1","attached":1}]
"#;
        assert_eq!(sessions_of(listed).unwrap()[0].name, "t1");
        assert_eq!(sessions_of(b"[]\n"), Some(Vec::new()));
        assert_eq!(sessions_of(b"lumen: unknown option sessions\n"), None);
        // an answer counts once its line is whole
        assert_eq!(answer_in(br#"{"ok":true,"lumen":"/usr/bin/lumen""#), None);
        assert_eq!(answer_in(b"hello\n"), None);
        let reply = answer_in(b"hello\n{\"ok\":true,\"lumen\":\"/usr/bin/lumen\"}\nmore").unwrap();
        assert!(reply.ok);
        assert_eq!(reply.lumen, "/usr/bin/lumen");
    }

    #[test]
    fn words_for_a_shell() {
        assert_eq!(quoted("/usr/bin/lumen"), "'/usr/bin/lumen'");
        assert_eq!(quoted("/home/u/my bin/lumen"), "'/home/u/my bin/lumen'");
        assert_eq!(quoted("it's"), "'it'\\''s'");
    }

    #[test]
    fn fingerprints_from_hex() {
        let hex_key = hex(&[0xab; 32]);
        assert_eq!(prints(&[hex_key, "zz".into(), "abcd".into()]), [[0xab; 32]]);
    }
}
