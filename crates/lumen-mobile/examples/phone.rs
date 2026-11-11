//! A phone on the command line: what the Android app does with this
//! library, without Android. For trying the PC side and for the tests.
//!
//! The phone's key is a file here instead of secure hardware, and
//! `openssl` signs with it. It hands out the same two things Android's key
//! store does: the public key as X.509 DER and signatures as DER.
//!
//!   phone key <dir>                          make a key in <dir>, print its public line
//!   phone enroll <dir> <link> [device]       pair with a PC (`lumen pair` shows the link)
//!   phone sessions <dir>                     the terminals the PC keeps
//!   phone run <dir> <session> <text> <wait>  open a terminal, type <text> and Enter, print the
//!                                            screen once it shows <wait>
//!   phone kill <dir> <session>               end a kept terminal
//!
//! <dir> holds `key.pem` and, once paired, `host.json`. PHONE_HAD tells
//! `enroll` which pairings this phone has, one to a line: the host key that
//! PC proved (`proven` in host.json), then the phone's public key for it.
//! If the PC that answers is one of them, it is asked to put the new key in
//! the place of the old.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use lumen_mobile::link::{self, Host, Sign, TabEvents};
use lumen_mobile::runtime;
use lumen_mobile::screen::Size;

struct FileKey(PathBuf);

impl FileKey {
    fn make(dir: &Path) -> FileKey {
        let path = dir.join("key.pem");
        if !path.exists() {
            let made = Command::new("openssl")
                .args(["ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out"])
                .arg(&path)
                .status();
            assert!(made.is_ok_and(|status| status.success()), "openssl made no key");
        }
        FileKey(path)
    }

    fn public_line(&self) -> String {
        let spki = Command::new("openssl")
            .args(["ec", "-pubout", "-outform", "DER", "-in"])
            .arg(&self.0)
            .stderr(Stdio::null())
            .output()
            .expect("openssl runs");
        link::public_key_line(&spki.stdout).expect("a P-256 key")
    }
}

impl Sign for FileKey {
    fn sign(&self, data: &[u8]) -> Option<Vec<u8>> {
        use std::io::Write;
        let mut openssl = Command::new("openssl")
            .args(["dgst", "-sha256", "-sign"])
            .arg(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        openssl.stdin.take()?.write_all(data).ok()?;
        let signed = openssl.wait_with_output().ok()?;
        signed.status.success().then_some(signed.stdout)
    }
}

enum Told {
    Dirty,
    Ended(i32),
    Lost(String),
}

struct Events(Sender<Told>);

impl TabEvents for Events {
    fn dirty(&self) {
        let _ = self.0.send(Told::Dirty);
    }

    fn title(&self, _: &str) {}

    fn bell(&self) {}

    fn clipboard(&self, _: &str) {}

    fn ended(&self, status: i32) {
        let _ = self.0.send(Told::Ended(status));
    }

    fn lost(&self, reason: &str) {
        let _ = self.0.send(Told::Lost(reason.to_string()));
    }
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("phone: {message}");
    std::process::exit(1)
}

fn host(dir: &Path) -> Host {
    let stored = std::fs::read_to_string(dir.join("host.json")).unwrap_or_else(|_| fail("not paired: no host.json"));
    serde_json::from_str(&stored).unwrap_or_else(|e| fail(e))
}

fn connect(dir: &Path) -> Arc<link::Link> {
    let key = Arc::new(FileKey::make(dir));
    match runtime().block_on(link::connect(&host(dir), key)) {
        Ok(link) => link,
        Err(e) => fail(format!("{}: {}", e.code, e.message)),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words[..] {
        ["key", dir] => println!("{}", FileKey::make(Path::new(dir)).public_line()),
        ["enroll", dir, link, ref device @ ..] => {
            let dir = Path::new(dir);
            let key = FileKey::make(dir);
            let device = device.first().copied().unwrap_or("phone on the command line");
            // PHONE_HAD: the pairings this phone has, one to a line: the host key
            // that PC proved (in hex), then the phone's public key for it
            let had: Vec<(String, String)> = std::env::var("PHONE_HAD")
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.trim().split_once(' '))
                .map(|(machine, key)| (machine.to_string(), key.to_string()))
                .collect();
            let asked = || eprintln!("phone: the PC is asking");
            match runtime().block_on(link::enroll(link, device, &key.public_line(), &had, asked)) {
                Ok(host) => {
                    let stored = serde_json::to_string_pretty(&host).expect("a host is JSON");
                    std::fs::write(dir.join("host.json"), &stored).unwrap_or_else(|e| fail(e));
                    println!("{stored}");
                },
                Err(e) => fail(format!("{}: {}", e.code, e.message)),
            }
        },
        ["sessions", dir] => {
            let link = connect(Path::new(dir));
            println!("keeps: {}", link.keeps());
            for session in runtime().block_on(link.sessions()) {
                println!("{} attached={} cwd={} title={}", session.name, session.attached, session.cwd, session.title);
            }
        },
        ["kill", dir, session] => runtime().block_on(connect(Path::new(dir)).kill(session)),
        ["run", dir, session, text, wait] => {
            let link = connect(Path::new(dir));
            let (tx, told) = channel();
            let tab = link::open(link.clone(), session, Size::new(80, 24, 10, 20), Arc::new(Events(tx)));
            let shows = |word: &str| tab.screen.text_lines().iter().any(|line| line.contains(word));
            let mut snapshot = vec![0; 1 << 20];
            let mut typed = false;
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut quiet_since = Instant::now();
            loop {
                match told.recv_timeout(Duration::from_millis(100)) {
                    Ok(Told::Dirty) => {
                        // looking at the screen is what lets the next change be told
                        let _ = tab.screen.snapshot(&mut snapshot);
                        quiet_since = Instant::now();
                    },
                    Ok(Told::Ended(status)) => {
                        println!("[ended with {status}]");
                        break;
                    },
                    Ok(Told::Lost(reason)) => fail(format!("lost: {reason}")),
                    Err(_) => {},
                }
                // the shell has said what it had to say: type
                if !typed && quiet_since.elapsed() > Duration::from_millis(700) {
                    tab.send(tab.screen.text(&format!("{text}\n"), 0));
                    typed = true;
                }
                if typed && shows(wait) && quiet_since.elapsed() > Duration::from_millis(300) {
                    break;
                }
                if Instant::now() > deadline {
                    for line in tab.screen.text_lines() {
                        eprintln!("| {line}");
                    }
                    fail(format!("the screen never showed {wait:?}"));
                }
            }
            for line in tab.screen.text_lines() {
                if !line.is_empty() {
                    println!("{line}");
                }
            }
            if std::env::var_os("PHONE_COLORS").is_some() {
                // the screen's own colors, as the app is told them: background, foreground, cursor
                let _ = tab.screen.snapshot(&mut snapshot);
                let word = |at: usize| u32::from_le_bytes(snapshot[at..at + 4].try_into().expect("four bytes"));
                println!("[colors {:08x} {:08x} {:08x}]", word(20), word(24), word(28));
            }
            tab.close();
            runtime().block_on(link.disconnect());
        },
        _ => fail("usage: phone key|enroll|sessions|run|kill ... (see the top of examples/phone.rs)"),
    }
}
