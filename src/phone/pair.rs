//! Pairing a phone: `lumen pair`, the `lumen pair-enroll` its short-lived
//! key runs, and `lumen phones` and `lumen unpair` for afterwards.
//!
//! See the `lumen_link` crate for the design. In short: `lumen pair` puts a
//! short-lived key into `authorized_keys` that may only run
//! `lumen pair-enroll`, and shows it as a QR code. The phone logs in with
//! it; `pair-enroll` hands the phone's request over a unix socket to the
//! waiting `lumen pair`, which asks its user and swaps the short-lived key
//! for the phone's own.

use std::io::{self, BufRead, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lumen_link::{EnrollReply, EnrollRequest, Offer};

use super::authorized::{self, PhoneKey};
use super::sys::{self, Interface};
use super::{runtime_dir, this_lumen, usage_error};

/// How long `lumen pair` waits for a phone, and how long its key is good for.
const PAIRING_TIME: Duration = Duration::from_secs(10 * 60);
const MAX_LINE: usize = 4096;
/// Interfaces of containers and virtual machines: not where a phone is.
const BRIDGES: [&str; 6] = ["docker", "br-", "veth", "virbr", "lxc", "podman"];
/// Interfaces of VPNs: reachable from farther away, but slower than the LAN.
const VPNS: [&str; 2] = ["tailscale", "wg"];
/// Host key types a phone can check, best first.
const HOST_KEY_TYPES: [&str; 5] =
    ["ssh-ed25519", "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521", "ssh-rsa"];

/// What has to be undone if `lumen pair` is stopped: the pairing line and
/// the socket. Edits of the file hold this lock, so a signal never cleans
/// up in the middle of one.
static CLEANUP: Mutex<Option<Cleanup>> = Mutex::new(None);

struct Cleanup {
    keys: PathBuf,
    token: String,
    socket: PathBuf,
}

impl Cleanup {
    fn run(&self) {
        let _ = authorized::edit(&self.keys, |text| authorized::without_pairing(text, &self.token));
        let _ = std::fs::remove_file(&self.socket);
    }
}

#[derive(Default)]
struct PairArgs {
    addresses: Vec<String>,
    port: Option<u16>,
    label: Option<String>,
    keys: Option<PathBuf>,
    host_keys: Vec<PathBuf>,
    yes: bool,
}

fn parse_pair_args(args: &[String]) -> Result<PairArgs, i32> {
    let mut parsed = PairArgs::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().cloned().ok_or_else(|| usage_error(format!("{arg} wants {what}")));
        match arg.as_str() {
            "--address" => parsed.addresses.push(value("an address")?),
            "--port" => {
                let port = value("a port")?;
                match port.parse::<u16>() {
                    Ok(p) if p > 0 => parsed.port = Some(p),
                    _ => return Err(usage_error(format!("{port} is not a port"))),
                }
            },
            "--label" => parsed.label = Some(value("a name")?),
            "--authorized-keys" => parsed.keys = Some(value("a file")?.into()),
            "--host-key" => parsed.host_keys.push(value("a file")?.into()),
            "--yes" | "-y" => parsed.yes = true,
            other => return Err(usage_error(format!("unknown option {other}"))),
        }
    }
    Ok(parsed)
}

pub fn pair(args: &[String]) -> i32 {
    let args = match parse_pair_args(args) {
        Ok(args) => args,
        Err(status) => return status,
    };
    match Pairing::prepare(args).and_then(Pairing::run) {
        Ok(status) => status,
        Err(message) => {
            eprintln!("lumen: {message}");
            1
        },
    }
}

struct Pairing {
    offer: Offer,
    token: String,
    keys: PathBuf,
    user: String,
    yes: bool,
    lumen: PathBuf,
}

impl Pairing {
    /// Finds out everything the offer holds, before anything is changed.
    fn prepare(args: PairArgs) -> Result<Pairing, String> {
        let user = sys::user().map_err(|e| e.to_string())?;
        let port = args.port.or_else(sshd_port).unwrap_or(22);
        if !ssh_answers(port) {
            return Err(format!(
                "the phone logs in to this PC over SSH, but no SSH server answers on port {port} here.\n\
                 lumen: start one with 'sudo systemctl enable --now sshd' (on Debian and Ubuntu: 'ssh'), then run this again."
            ));
        }
        let label = args.label.unwrap_or_else(host_name);
        let mut addresses = args.addresses;
        addresses.extend(order_addresses(&sys::interfaces()));
        addresses.extend(magic_dns_name());
        let mut seen = Vec::new();
        addresses.retain(|a| {
            !a.is_empty() && !seen.contains(a) && {
                seen.push(a.clone());
                true
            }
        });
        addresses.truncate(8);
        if addresses.is_empty() {
            return Err("this PC has no network address a phone could reach. Give one with --address.".into());
        }
        let host_keys = host_keys(&args.host_keys, port)?;
        let offer = Offer { user: user.name.clone(), port, label, seed: Offer::new_seed(), addresses, host_keys };
        let token = offer.token();
        let lumen = this_lumen().map_err(|e| format!("can't find where lumen is: {e}"))?;
        Ok(Pairing {
            offer,
            token,
            keys: authorized::path(args.keys, &user.home),
            user: user.name,
            yes: args.yes,
            lumen,
        })
    }

    fn run(self) -> Result<i32, String> {
        let socket = runtime_dir().map_err(|e| e.to_string())?.join(format!("pair-{}.sock", self.token));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).map_err(|e| format!("can't listen on {}: {e}", socket.display()))?;

        // from here on, a signal is answered by cleaning up, in a thread of its own
        let signals = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];
        sys::block_signals(&signals);
        {
            let mut cleanup = CLEANUP.lock().unwrap_or_else(|e| e.into_inner());
            *cleanup = Some(Cleanup { keys: self.keys.clone(), token: self.token.clone(), socket: socket.clone() });
            let expiry = sys::local_stamp(sys::now() + PAIRING_TIME.as_secs() as i64);
            let line = authorized::pairing_line(&self.lumen, &self.token, &expiry, &self.offer.public_key_line())?;
            let now = sys::local_stamp(sys::now());
            let added = authorized::edit(&self.keys, |text| {
                authorized::with_line(&authorized::without_stale(text, &now), &line)
            });
            if let Err(e) = added {
                let _ = std::fs::remove_file(&socket);
                *cleanup = None;
                return Err(format!("can't change {}: {e}", self.keys.display()));
            }
        }
        std::thread::spawn(move || {
            let signal = sys::wait_signal(&signals);
            let cleanup = CLEANUP.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cleanup) = cleanup.as_ref() {
                cleanup.run();
            }
            sys::restore_terminal();
            if signal == libc::SIGINT {
                eprintln!();
            }
            std::process::exit(128 + signal);
        });

        self.show();
        let outcome = self.wait(&listener);
        let mut cleanup = CLEANUP.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cleanup) = cleanup.take() {
            cleanup.run();
        }
        drop(cleanup);
        match outcome {
            Some(name) => {
                // whoever started this may have stopped reading: the pairing stands anyway
                let _ = writeln!(
                    io::stdout(),
                    "{name} is paired.\n'lumen phones' lists the paired phones.\n\
                     'lumen unpair \"{name}\"' takes this one off again."
                );
                Ok(0)
            },
            None => {
                eprintln!(
                    "lumen: no phone came in 10 minutes. The code is no longer good; run 'lumen pair' for a new one."
                );
                Ok(1)
            },
        }
    }

    fn show(&self) {
        let link = self.offer.to_link();
        let mut out = io::stdout().lock();
        if !sys::is_terminal(1) {
            let _ = writeln!(out, "{link}");
            let _ = out.flush();
            return;
        }
        let _ = writeln!(out, "Open lumen on your phone and scan this:\n");
        for line in qr_lines(&link) {
            let _ = writeln!(out, "  {line}");
        }
        let _ = writeln!(out, "\nor type this code into the app:\n{link}\n");
        let _ = writeln!(out, "Waiting for the phone, for 10 minutes. Ctrl+C stops.");
        let _ = out.flush();
    }

    /// Takes phones' requests until one is let in (its name) or the time is up.
    fn wait(&self, listener: &UnixListener) -> Option<String> {
        let deadline = Instant::now() + PAIRING_TIME;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            if !readable(listener.as_raw_fd(), left.min(Duration::from_secs(1))) {
                continue;
            }
            let Ok((stream, _)) = listener.accept() else { continue };
            if let Some(name) = self.answer(stream) {
                return Some(name);
            }
        }
    }

    /// Reads one request, asks, and answers it. The phone's name if it was let in.
    fn answer(&self, mut stream: UnixStream) -> Option<String> {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let line = read_line(&mut stream).ok()?;
        let reply = |stream: &mut UnixStream, reply: EnrollReply| {
            if let Ok(text) = serde_json::to_string(&reply) {
                let _ = stream.write_all(format!("{text}\n").as_bytes());
            }
        };
        let refuse = |stream: &mut UnixStream, error: String| {
            reply(stream, EnrollReply { ok: false, error, ..Default::default() });
        };
        let request: EnrollRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(_) => {
                refuse(&mut stream, "the PC didn't understand the request: update lumen on the PC or the app".into());
                return None;
            },
        };
        let key = match PhoneKey::parse(&request.public_key) {
            Ok(key) => key,
            Err(e) => {
                refuse(&mut stream, format!("the PC can't use the phone's key: {e}"));
                return None;
            },
        };
        let device = lumen_link::clean_name(&request.device);
        if !self.confirm(&device, &key) {
            refuse(&mut stream, "The PC's user did not let this phone in.".into());
            return None;
        }

        let mut cleanup = CLEANUP.lock().unwrap_or_else(|e| e.into_inner());
        let enrolled = authorized::edit(&self.keys, |text| authorized::enrolled(text, &self.token, &key, &device));
        match enrolled {
            Ok((_, name)) => {
                // the pairing line went with the edit: only the socket is left to clean up
                if let Some(cleanup) = cleanup.as_mut() {
                    cleanup.token.clear();
                }
                drop(cleanup);
                let lumen = self.lumen.to_string_lossy().into_owned();
                let version = env!("CARGO_PKG_VERSION").to_string();
                reply(&mut stream, EnrollReply { ok: true, error: String::new(), lumen, version });
                Some(name)
            },
            Err(e) => {
                drop(cleanup);
                eprintln!("lumen: can't change {}: {e}", self.keys.display());
                refuse(&mut stream, "the PC could not save the phone's key".into());
                None
            },
        }
    }

    fn confirm(&self, device: &str, key: &PhoneKey) -> bool {
        if self.yes {
            return true;
        }
        if !sys::is_terminal(0) {
            eprintln!("lumen: {device} wants to log in, but there is nobody to ask here (no terminal). Use --yes.");
            return false;
        }
        let question = format!(
            "{device} wants to log in to this PC as {}. Its key: {}. Add it? [Y/n] ",
            self.user,
            key.fingerprint()
        );
        if sys::is_terminal(1) {
            print!("\n{question}");
            let _ = io::stdout().flush();
        } else {
            eprint!("{question}");
        }
        let mut answer = String::new();
        if io::stdin().lock().read_line(&mut answer).unwrap_or(0) == 0 {
            return false;
        }
        matches!(answer.trim().to_lowercase().as_str(), "" | "y" | "yes")
    }
}

/// Waits until a descriptor can be read, at most this long.
fn readable(fd: i32, timeout: Duration) -> bool {
    let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    // SAFETY: one pollfd, and its count
    let ready = unsafe { libc::poll(&mut poll, 1, timeout.as_millis().min(i32::MAX as u128) as i32) };
    ready > 0
}

/// Reads up to a newline, at most [`MAX_LINE`] bytes, byte by byte so that
/// nothing after it is taken from the stream.
fn read_line(stream: &mut impl Read) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8];
    loop {
        match stream.read(&mut byte) {
            Ok(0) if line.is_empty() => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => line.push(byte[0]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
        if line.len() > MAX_LINE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the line is too long"));
        }
    }
    String::from_utf8(line).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "not UTF-8"))
}

/// The first `Port` sshd's configuration names, following its `Include`s.
fn sshd_port() -> Option<u16> {
    let text = std::fs::read_to_string("/etc/ssh/sshd_config").ok()?;
    port_in(&text, &|pattern| {
        let pattern = if pattern.starts_with('/') { pattern.to_string() } else { format!("/etc/ssh/{pattern}") };
        expand(&pattern).into_iter().filter_map(|path| std::fs::read_to_string(path).ok()).collect()
    })
}

fn port_in(text: &str, include: &dyn Fn(&str) -> Vec<String>) -> Option<u16> {
    for line in text.lines() {
        let line = line.trim();
        let (keyword, value) = line.split_once(|c: char| c.is_whitespace() || c == '=').unwrap_or((line, ""));
        let value = value.trim().trim_start_matches('=').trim();
        if keyword.eq_ignore_ascii_case("port") {
            if let Ok(port) = value.parse::<u16>() {
                return Some(port);
            }
        } else if keyword.eq_ignore_ascii_case("include") {
            for pattern in value.split_whitespace() {
                for included in include(pattern) {
                    if let Some(port) = port_in(&included, include) {
                        return Some(port);
                    }
                }
            }
        } else if keyword.eq_ignore_ascii_case("match") {
            // what follows is for some connections only: not the port
            return None;
        }
    }
    None
}

/// A path whose last part may hold a `*`, as sshd's `Include` lines do.
fn expand(pattern: &str) -> Vec<PathBuf> {
    let path = Path::new(pattern);
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Vec::new();
    };
    if !name.contains('*') {
        return vec![path.to_path_buf()];
    }
    let (before, after) = name.split_once('*').unwrap_or((name, ""));
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.len() >= before.len() + after.len() && n.starts_with(before) && n.ends_with(after))
        })
        .collect();
    found.sort();
    found
}

/// Whether an SSH server answers on this port of this machine.
fn ssh_answers(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut banner = [0u8; 4];
    stream.read_exact(&mut banner).is_ok() && &banner == b"SSH-"
}

fn host_name() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: the buffer and its length
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return "PC".into();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let name = String::from_utf8_lossy(&buf[..end]).into_owned();
    if name.is_empty() { "PC".into() } else { name }
}

/// The addresses a phone may reach this PC at: the LAN's first, then the
/// VPNs', never loopback or the bridges of containers and virtual machines.
pub fn order_addresses(interfaces: &[Interface]) -> Vec<String> {
    let usable = interfaces
        .iter()
        .filter(|i| i.up && !i.loopback && !i.address.is_loopback() && !i.address.is_unspecified())
        .filter(|i| !BRIDGES.iter().any(|prefix| i.name.starts_with(prefix)));
    let (vpn, lan): (Vec<&Interface>, Vec<&Interface>) =
        usable.partition(|i| VPNS.iter().any(|prefix| i.name.starts_with(prefix)));
    let mut addresses: Vec<String> = Vec::new();
    for interface in lan.into_iter().chain(vpn) {
        let address = interface.address.to_string();
        if !addresses.contains(&address) {
            addresses.push(address);
        }
    }
    addresses
}

/// The name Tailscale gives this machine, which works from anywhere on the
/// tailnet even when addresses change.
fn magic_dns_name() -> Option<String> {
    let program = sys::on_path("tailscale")?;
    let output = run_briefly(std::process::Command::new(program).args(["status", "--json"]), Duration::from_secs(3))?;
    let status: serde_json::Value = serde_json::from_slice(&output).ok()?;
    let name = status.get("Self")?.get("DNSName")?.as_str()?.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_string())
}

/// Runs a program for its output, giving up after a while.
fn run_briefly(command: &mut std::process::Command, timeout: Duration) -> Option<Vec<u8>> {
    use std::process::Stdio;
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().ok()?;
                return status.success().then_some(out);
            },
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            },
        }
    }
}

/// The fingerprints of the keys sshd proves itself with, from the files
/// given or the system's, else from asking sshd itself.
fn host_keys(given: &[PathBuf], port: u16) -> Result<Vec<[u8; 32]>, String> {
    let files = if given.is_empty() {
        let mut found = expand("/etc/ssh/ssh_host_*_key.pub");
        found.sort();
        found
    } else {
        given.to_vec()
    };
    let mut lines: Vec<String> = files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| text.lines().map(str::to_string).collect::<Vec<_>>())
        .collect();
    if usable_host_keys(&lines).is_empty() {
        let keyscan = sys::on_path("ssh-keyscan").unwrap_or_else(|| "ssh-keyscan".into());
        let mut command = std::process::Command::new(keyscan);
        command.args(["-p", &port.to_string(), "127.0.0.1"]);
        if let Some(output) = run_briefly(&mut command, Duration::from_secs(5)) {
            lines = String::from_utf8_lossy(&output).lines().map(str::to_string).collect();
        }
    }
    let keys = usable_host_keys(&lines);
    if keys.is_empty() {
        return Err("can't find the keys this PC's SSH server proves itself with. Give one with --host-key.".into());
    }
    Ok(keys)
}

/// The fingerprints of the lines a phone can check, the best type first.
/// Types it can't check (post-quantum ones, for now) are left out.
fn usable_host_keys(lines: &[String]) -> Vec<[u8; 32]> {
    let mut keys: Vec<(usize, [u8; 32])> = lines
        .iter()
        .filter_map(|line| {
            let blob = lumen_link::key_blob(line)?;
            let len = u32::from_be_bytes(blob.get(..4)?.try_into().ok()?) as usize;
            let kind = std::str::from_utf8(blob.get(4..4 + len)?).ok()?;
            let rank = HOST_KEY_TYPES.iter().position(|t| *t == kind)?;
            Some((rank, lumen_link::fingerprint(line)?))
        })
        .collect();
    keys.sort_by_key(|(rank, _)| *rank);
    let mut seen = Vec::new();
    keys.into_iter()
        .map(|(_, key)| key)
        .filter(|key| {
            let new = !seen.contains(key);
            seen.push(*key);
            new
        })
        .collect()
}

/// A QR code in half blocks: two modules a character, black on white in
/// exact colors so that it scans whatever the terminal's theme is, with a
/// quiet zone of two modules.
fn qr_lines(link: &str) -> Vec<String> {
    use qrcode::{Color, EcLevel, QrCode};
    let Ok(code) = QrCode::with_error_correction_level(link, EcLevel::L) else { return Vec::new() };
    let width = code.width();
    let colors = code.to_colors();
    let quiet = 2;
    let size = width + 2 * quiet;
    let dark = |x: usize, y: usize| {
        x >= quiet
            && y >= quiet
            && x < quiet + width
            && y < quiet + width
            && colors[(y - quiet) * width + (x - quiet)] == Color::Dark
    };
    let rgb = |dark: bool| if dark { "0;0;0" } else { "255;255;255" };
    (0..size)
        .step_by(2)
        .map(|y| {
            let mut line = String::new();
            let mut pen = None;
            for x in 0..size {
                let colors = (dark(x, y), y + 1 < size && dark(x, y + 1));
                if pen != Some(colors) {
                    line.push_str(&format!("\x1b[38;2;{}m\x1b[48;2;{}m", rgb(colors.0), rgb(colors.1)));
                    pen = Some(colors);
                }
                line.push('\u{2580}');
            }
            line.push_str("\x1b[0m");
            line
        })
        .collect()
}

/// `lumen pair-enroll <token>`: the only thing the short-lived key may run.
/// The phone is on standard input and output; nothing but the reply goes
/// to standard output.
pub fn enroll(args: &[String]) -> i32 {
    let fail = || {
        let reply = EnrollReply {
            ok: false,
            error: "lumen pair is no longer waiting on the PC. Run it again.".into(),
            ..Default::default()
        };
        println!("{}", serde_json::to_string(&reply).unwrap_or_default());
        1
    };
    let [token] = args else { return fail() };
    if token.len() != 16 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return fail();
    }
    if !readable(0, Duration::from_secs(30)) {
        return fail();
    }
    let Ok(line) = read_line(&mut io::stdin().lock()) else { return fail() };
    let Ok(dir) = runtime_dir() else { return fail() };
    let Ok(mut stream) = UnixStream::connect(dir.join(format!("pair-{token}.sock"))) else { return fail() };
    if stream.write_all(format!("{line}\n").as_bytes()).is_err() {
        return fail();
    }
    // the PC's user may take a while to answer
    let _ = stream.set_read_timeout(Some(Duration::from_secs(180)));
    match read_line(&mut stream) {
        Ok(reply) if !reply.trim().is_empty() => {
            println!("{reply}");
            0
        },
        _ => fail(),
    }
}

fn keys_arg(args: &[String], positional: usize) -> Result<(Vec<String>, Option<PathBuf>), i32> {
    let mut rest = Vec::new();
    let mut keys = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--authorized-keys" => match args.next() {
                Some(file) => keys = Some(PathBuf::from(file)),
                None => return Err(usage_error(format!("{arg} wants a file"))),
            },
            other if other.starts_with("--") => return Err(usage_error(format!("unknown option {other}"))),
            other => rest.push(other.to_string()),
        }
    }
    if rest.len() != positional {
        return Err(usage_error(if positional == 0 {
            format!("unexpected {}", rest[0])
        } else {
            "unpair wants the name of a phone (see 'lumen phones')".to_string()
        }));
    }
    Ok((rest, keys))
}

/// The file of keys a command works on.
fn keys_file(given: Option<PathBuf>) -> Result<PathBuf, i32> {
    match sys::user() {
        Ok(user) => Ok(authorized::path(given, &user.home)),
        Err(e) => {
            eprintln!("lumen: {e}");
            Err(1)
        },
    }
}

pub fn phones(args: &[String]) -> i32 {
    let (_, keys) = match keys_arg(args, 0) {
        Ok(parsed) => parsed,
        Err(status) => return status,
    };
    let path = match keys_file(keys) {
        Ok(path) => path,
        Err(status) => return status,
    };
    let text = match authorized::read(&path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("lumen: can't read {}: {e}", path.display());
            return 1;
        },
    };
    let phones = authorized::phones(&text);
    if phones.is_empty() {
        println!("No phone is paired. 'lumen pair' pairs one.");
        return 0;
    }
    let width = phones.iter().map(|p| p.name.chars().count()).max().unwrap_or(0);
    for phone in phones {
        let pad = width - phone.name.chars().count();
        println!("{}{}  {}", phone.name, " ".repeat(pad), phone.fingerprint);
    }
    0
}

pub fn unpair(args: &[String]) -> i32 {
    let (rest, keys) = match keys_arg(args, 1) {
        Ok(parsed) => parsed,
        Err(status) => return status,
    };
    let name = &rest[0];
    let path = match keys_file(keys) {
        Ok(path) => path,
        Err(status) => return status,
    };
    if !path.exists() {
        eprintln!("lumen: no phone is called {name}: no phone is paired.");
        return 1;
    }
    match authorized::edit(&path, |text| authorized::without_phone(text, name).ok_or_else(|| authorized::phones(text)))
    {
        Ok(Ok(_)) => {
            println!("{name} can no longer log in.");
            println!("A connection it has open right now stays until it ends.");
            0
        },
        Ok(Err(phones)) if phones.is_empty() => {
            eprintln!("lumen: no phone is called {name}: no phone is paired.");
            1
        },
        Ok(Err(phones)) => {
            let names: Vec<String> = phones.into_iter().map(|p| format!("\"{}\"", p.name)).collect();
            eprintln!("lumen: no phone is called {name}. The paired phones are: {}.", names.join(", "));
            1
        },
        Err(e) => {
            eprintln!("lumen: can't change {}: {e}", path.display());
            1
        },
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn interface(name: &str, address: [u8; 4]) -> Interface {
        Interface { name: name.into(), up: true, loopback: false, address: Ipv4Addr::from(address) }
    }

    #[test]
    fn addresses_lan_first_without_bridges() {
        let interfaces = vec![
            Interface { loopback: true, ..interface("lo", [127, 0, 0, 1]) },
            interface("tailscale0", [100, 64, 0, 7]),
            interface("docker0", [172, 17, 0, 1]),
            interface("br-1a2b", [172, 18, 0, 1]),
            interface("veth12", [169, 254, 1, 1]),
            interface("virbr0", [192, 168, 122, 1]),
            interface("lxcbr0", [10, 0, 3, 1]),
            interface("podman1", [10, 88, 0, 1]),
            interface("wg0", [10, 9, 0, 2]),
            interface("enp5s0", [192, 168, 1, 20]),
            Interface { up: false, ..interface("wlan0", [192, 168, 1, 21]) },
            interface("wlan1", [10, 0, 0, 5]),
            // the same address twice counts once
            interface("enp5s0", [192, 168, 1, 20]),
        ];
        assert_eq!(order_addresses(&interfaces), ["192.168.1.20", "10.0.0.5", "100.64.0.7", "10.9.0.2"]);
        assert!(order_addresses(&[]).is_empty());
    }

    #[test]
    fn the_port_sshd_listens_on() {
        let none = |_: &str| Vec::new();
        assert_eq!(port_in("#Port 22\nPermitRootLogin no\n", &none), None);
        assert_eq!(port_in("Port 2222\nPort 22\n", &none), Some(2222));
        assert_eq!(port_in("  port=2200\n", &none), Some(2200));
        let included = |pattern: &str| {
            assert_eq!(pattern, "/etc/ssh/sshd_config.d/*.conf");
            vec!["# nothing\n".to_string(), "Port 443\n".to_string()]
        };
        assert_eq!(port_in("Include /etc/ssh/sshd_config.d/*.conf\nPort 22\n", &included), Some(443));
        assert_eq!(port_in("Match User git\n  Port 2\n", &none), None);
    }

    #[test]
    fn host_keys_the_phone_can_check_best_first() {
        let ed = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs root@desk";
        let mut unknown = Vec::new();
        for part in [&b"ssh-mldsa44-ed25519@openssh.com"[..], &[1u8; 16]] {
            unknown.extend_from_slice(&(part.len() as u32).to_be_bytes());
            unknown.extend_from_slice(part);
        }
        use base64::Engine;
        let unknown =
            format!("ssh-mldsa44-ed25519@openssh.com {}", base64::engine::general_purpose::STANDARD.encode(unknown));
        let lines = vec![unknown, "garbage".into(), format!("127.0.0.1 {ed}"), ed.to_string()];
        assert_eq!(usable_host_keys(&lines), vec![lumen_link::fingerprint(ed).unwrap()]);
    }

    #[test]
    fn a_qr_code_is_square_with_a_quiet_zone() {
        let lines = qr_lines("lumen://pair/AQAW");
        assert!(!lines.is_empty());
        let cells = |line: &str| line.matches('\u{2580}').count();
        let width = cells(&lines[0]);
        assert!(lines.iter().all(|l| cells(l) == width));
        assert_eq!(lines.len(), width.div_ceil(2));
        // the first row is all quiet zone: white on white
        assert!(!lines[0].contains("38;2;0;0;0m"));
    }

    #[test]
    fn a_line_at_most_so_long() {
        let mut short: &[u8] = b"{\"a\":1}\nrest";
        assert_eq!(read_line(&mut short).unwrap(), "{\"a\":1}");
        assert_eq!(short, b"rest");
        let long = vec![b'x'; MAX_LINE + 10];
        assert!(read_line(&mut long.as_slice()).is_err());
        assert!(read_line(&mut &b""[..]).is_err());
    }
}
