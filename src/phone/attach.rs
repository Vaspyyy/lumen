//! `lumen attach` and `lumen sessions`: the clients of the keeper.

use std::io::{self, Read, Write};
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lumen_link::SessionInfo;

use super::frame::{self, Hello, Kill, Refused};
use super::keeper::{env_name_ok, socket_path};
use super::sys;
use super::{runtime_dir, this_lumen, usage_error};

/// How long `attach` waits for a keeper it started.
const KEEPER_START: Duration = Duration::from_secs(3);

struct AttachArgs {
    session: String,
    env: Vec<(String, String)>,
    cwd: Option<String>,
    takeover: bool,
}

fn parse_attach_args(args: &[String]) -> Result<AttachArgs, i32> {
    let mut session = None;
    let mut env = Vec::new();
    let mut cwd = None;
    let mut takeover = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().cloned().ok_or_else(|| usage_error(format!("{arg} wants {what}")));
        match arg.as_str() {
            "--session" => session = Some(value("a name")?),
            "--env" => {
                let pair = value("KEY=VALUE")?;
                match pair.split_once('=') {
                    Some((name, val)) if env_name_ok(name) => env.push((name.to_string(), val.to_string())),
                    _ => return Err(usage_error(format!("--env wants KEY=VALUE with KEY in capitals, not {pair}"))),
                }
            },
            "--cwd" => {
                // the keeper runs elsewhere: a relative folder is made whole here
                let dir = std::path::PathBuf::from(value("a folder")?);
                let dir = std::env::current_dir().map(|here| here.join(&dir)).unwrap_or(dir);
                cwd = Some(dir.to_string_lossy().into_owned());
            },
            "--takeover" => takeover = true,
            other => return Err(usage_error(format!("unknown option {other}"))),
        }
    }
    let Some(session) = session else { return Err(usage_error("attach wants --session NAME")) };
    if !lumen_link::session_name_ok(&session) {
        return Err(usage_error(format!(
            "{session} can't be the name of a terminal: letters, digits, '-', '_' and '.' only"
        )));
    }
    Ok(AttachArgs { session, env, cwd, takeover })
}

pub fn attach(args: &[String]) -> i32 {
    let args = match parse_attach_args(args) {
        Ok(args) => args,
        Err(status) => return status,
    };
    if !sys::is_terminal(0) {
        return usage_error("attach needs a terminal to show the session in");
    }
    // signals are taken by a thread of their own, set up before any other thread
    let signals = [libc::SIGWINCH, libc::SIGTERM, libc::SIGHUP, libc::SIGINT, libc::SIGQUIT];
    sys::block_signals(&signals);

    let dir = match runtime_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("lumen: {e}");
            return 1;
        },
    };
    let size = sys::winsize(0).unwrap_or(sys::Winsize { cols: 80, rows: 24, width: 0, height: 0 });
    let hello = Hello {
        session: args.session,
        cols: size.cols,
        rows: size.rows,
        width: size.width,
        height: size.height,
        env: args.env,
        cwd: args.cwd,
        takeover: args.takeover,
    };
    let mut stream = match hello_keeper(&dir, &hello) {
        Ok(stream) => stream,
        Err(status) => return status,
    };

    if let Err(e) = sys::raw_mode() {
        eprintln!("lumen: can't put the terminal in raw mode: {e}");
        return 1;
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        sys::restore_terminal();
        default_hook(info);
    }));

    let Ok(writer) = stream.try_clone() else {
        leave(1, Some("lost the connection to the keeper"));
    };
    let writer = Arc::new(Mutex::new(writer));
    {
        let writer = writer.clone();
        std::thread::spawn(move || {
            loop {
                match sys::wait_signal(&signals) {
                    libc::SIGWINCH => {
                        if let Some(size) = sys::winsize(0) {
                            let payload = frame::encode_size(size.cols, size.rows, size.width, size.height);
                            let mut writer = writer.lock().unwrap_or_else(|e| e.into_inner());
                            let _ = frame::write(&mut *writer, frame::RESIZE, &payload);
                        }
                    },
                    // the terminal is gone: a detach, the session stays
                    libc::SIGHUP => leave(0, None),
                    signal => leave(128 + signal, None),
                }
            }
        });
    }
    std::thread::spawn(move || {
        // SAFETY: standard input stays open for the life of the process; never closed here
        let mut stdin = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(0) });
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => leave(0, None),
                Ok(n) => {
                    let mut writer = writer.lock().unwrap_or_else(|e| e.into_inner());
                    if frame::write(&mut *writer, frame::INPUT, &buf[..n]).is_err() {
                        leave(1, Some("the keeper went away"));
                    }
                },
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {},
                Err(_) => leave(0, None),
            }
        }
    });

    let mut stdout = io::stdout().lock();
    let mut tail = Vec::new();
    loop {
        match frame::read(&mut stream) {
            Ok(Some((frame::OUTPUT, bytes))) => {
                note_screen(&mut tail, &bytes);
                if stdout.write_all(&bytes).and_then(|_| stdout.flush()).is_err() {
                    // nowhere to show it any more: a detach
                    leave(0, None);
                }
            },
            Ok(Some((frame::EXIT, _))) => leave(0, None),
            Ok(Some((frame::MOVED, _))) => leave(0, Some("this terminal is now shown somewhere else")),
            Ok(Some((frame::ERROR, message))) => leave(1, Some(&String::from_utf8_lossy(&message))),
            Ok(Some(_)) => {},
            Ok(None) | Err(_) => leave(1, Some("the keeper went away")),
        }
    }
}

/// Whether the program has the alternate screen, as far as what was shown
/// here says.
static ALT_SCREEN: AtomicBool = AtomicBool::new(false);

/// Follows the program onto the alternate screen and off it, in what it
/// prints. `tail` holds the end of what came before: a sequence may be cut in
/// two by the pieces it arrives in.
fn note_screen(tail: &mut Vec<u8>, bytes: &[u8]) {
    tail.extend_from_slice(bytes);
    let last = |marks: &[&[u8]]| {
        marks.iter().filter_map(|mark| tail.windows(mark.len()).rposition(|window| window == *mark)).max()
    };
    let entered = last(&[b"\x1b[?1049h", b"\x1b[?1047h", b"\x1b[?47h"]);
    // a full reset leaves it too
    let left = last(&[b"\x1b[?1049l", b"\x1b[?1047l", b"\x1b[?47l", b"\x1bc"]);
    match (entered, left) {
        (Some(entered), Some(left)) => ALT_SCREEN.store(entered > left, Ordering::Relaxed),
        (Some(_), None) => ALT_SCREEN.store(true, Ordering::Relaxed),
        (None, Some(_)) => ALT_SCREEN.store(false, Ordering::Relaxed),
        (None, None) => {},
    }
    let keep = tail.len().saturating_sub(8);
    tail.drain(..keep);
}

/// Puts the terminal back as it was and ends the program.
fn leave(status: i32, message: Option<&str>) -> ! {
    if sys::is_terminal(1) {
        // What the program asked of the terminal and may not have taken back: the
        // alternate screen, the mouse, focus reports, bracketed paste, application
        // keys, a hidden cursor, its shape, the kitty keyboard flags, colors.
        let mut reset = String::new();
        if ALT_SCREEN.load(Ordering::Relaxed) {
            reset.push_str("\x1b[?1049l");
        }
        reset.push_str("\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1004l\x1b[?2004l");
        reset.push_str("\x1b[?1l\x1b>\x1b[?25h\x1b[0 q\x1b[<99u\x1b[0m");
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(reset.as_bytes()).and_then(|_| stdout.flush());
    }
    sys::restore_terminal();
    if let Some(message) = message {
        eprintln!("\r\nlumen: {message}");
    }
    std::process::exit(status);
}

/// Connects to the keeper, starting one if there is none, and attaches.
fn hello_keeper(dir: &Path, hello: &Hello) -> Result<UnixStream, i32> {
    let mut started = false;
    let deadline = Instant::now() + KEEPER_START;
    loop {
        match try_hello(dir, hello) {
            Ok(Some(stream)) => return Ok(stream),
            Ok(None) => {},
            Err(status) => return Err(status),
        }
        if !started {
            started = true;
            if let Err(e) = start_keeper(dir) {
                eprintln!("lumen: can't start the keeper: {e}");
                return Err(1);
            }
            continue;
        }
        if Instant::now() > deadline {
            eprintln!("lumen: the keeper didn't start");
            return Err(1);
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// One try: `Ok(None)` when no keeper answered (or one was on its way out).
fn try_hello(dir: &Path, hello: &Hello) -> Result<Option<UnixStream>, i32> {
    let Ok(mut stream) = UnixStream::connect(socket_path(dir)) else { return Ok(None) };
    let payload = serde_json::to_vec(hello).unwrap_or_default();
    if frame::write(&mut stream, frame::VERSION, &frame::version_payload()).is_err()
        || frame::write(&mut stream, frame::HELLO, &payload).is_err()
    {
        return Ok(None);
    }
    match frame::read(&mut stream) {
        Ok(Some((frame::WELCOME, _))) => Ok(Some(stream)),
        Ok(Some((frame::REFUSED, payload))) => {
            refused(&payload);
            Err(3)
        },
        Ok(Some((frame::ERROR, message))) => {
            eprintln!("lumen: {}", String::from_utf8_lossy(&message));
            Err(1)
        },
        _ => Ok(None),
    }
}

fn refused(payload: &[u8]) {
    let version = serde_json::from_slice::<Refused>(payload).map(|r| r.version).unwrap_or_else(|_| "another".into());
    eprintln!(
        "lumen: terminals started by lumen {version} are still running here, and this lumen ({}) can't reach them.\n\
         lumen: they have to end first: lumen sessions kill --all",
        env!("CARGO_PKG_VERSION")
    );
}

/// Starts a keeper that outlives this process and the login it runs in.
///
/// Under systemd, as a scope of the user's service manager: logind may end
/// everything of an SSH login when it ends, but not that. Otherwise as a
/// grandchild in a session of its own.
fn start_keeper(dir: &Path) -> io::Result<()> {
    let lumen = this_lumen()?;
    if let Some(systemd_run) = sys::on_path("systemd-run") {
        let mut command = Command::new(systemd_run);
        command.args(["--user", "--scope", "--quiet", "--collect", "--"]).arg(&lumen).arg("keeper");
        // it finds the service manager through XDG_RUNTIME_DIR, which a login without PAM lacks
        if std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|v| v.is_empty())
            && let Some(base) = dir.parent()
        {
            command.env("XDG_RUNTIME_DIR", base);
        }
        detached(&mut command, false);
        if let Ok(mut child) = command.spawn() {
            // it becomes the keeper, or ends at once when there is no service manager to ask
            let deadline = Instant::now() + KEEPER_START;
            while Instant::now() < deadline {
                if socket_path(dir).exists() && UnixStream::connect(socket_path(dir)).is_ok() {
                    return Ok(());
                }
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            if UnixStream::connect(socket_path(dir)).is_ok() {
                return Ok(());
            }
        }
    }
    let mut command = Command::new(&lumen);
    command.arg("keeper");
    detached(&mut command, true);
    let mut child = command.spawn()?;
    // the middle process ends at once; its child is the keeper
    let _ = child.wait();
    Ok(())
}

/// No terminal, no standard streams, a session of its own; and with
/// `fork`, not even a child of this process.
fn detached(command: &mut Command, fork: bool) {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: only async-signal-safe calls between fork and exec
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if fork {
                match libc::fork() {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => {},
                    _ => libc::_exit(0),
                }
            }
            let mut none = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            libc::sigemptyset(none.as_mut_ptr());
            libc::pthread_sigmask(libc::SIG_SETMASK, none.as_ptr(), std::ptr::null_mut());
            Ok(())
        });
    }
}

/// `lumen sessions [--json]`, `lumen sessions kill NAME|--all`.
pub fn sessions(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("kill") => kill(&args[1..]),
        Some("--json") if args.len() == 1 => list(true),
        None => list(false),
        Some(other) if other.starts_with('-') => usage_error(format!("unknown option {other}")),
        Some(other) => usage_error(format!("unexpected {other}")),
    }
}

/// Connects to a running keeper, never starting one. `None` if none runs.
fn request(kind: u8, payload: &[u8]) -> Result<Option<(u8, Vec<u8>, UnixStream)>, String> {
    let dir = runtime_dir().map_err(|e| e.to_string())?;
    let Ok(mut stream) = UnixStream::connect(socket_path(&dir)) else { return Ok(None) };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    frame::write(&mut stream, frame::VERSION, &frame::version_payload()).map_err(|e| e.to_string())?;
    frame::write(&mut stream, kind, payload).map_err(|e| e.to_string())?;
    match frame::read(&mut stream) {
        Ok(Some((kind, payload))) => Ok(Some((kind, payload, stream))),
        // on its way out: as good as none
        _ => Ok(None),
    }
}

fn list(json: bool) -> i32 {
    let sessions: Vec<SessionInfo> = match request(frame::LIST, &[]) {
        Ok(Some((frame::LISTING, payload, _))) => serde_json::from_slice(&payload).unwrap_or_default(),
        Ok(Some((frame::REFUSED, payload, _))) => {
            refused(&payload);
            return 3;
        },
        Ok(_) => Vec::new(),
        Err(e) => {
            eprintln!("lumen: {e}");
            return 1;
        },
    };
    if json {
        println!("{}", serde_json::to_string(&sessions).unwrap_or_else(|_| "[]".into()));
        return 0;
    }
    if sessions.is_empty() {
        println!("No terminal is running here.");
        return 0;
    }
    let home = sys::user().map(|u| u.home.to_string_lossy().into_owned()).unwrap_or_default();
    let rows: Vec<[String; 5]> = sessions
        .iter()
        .map(|s| {
            let cwd = match s.cwd.strip_prefix(&home) {
                Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
                _ => s.cwd.clone(),
            };
            let attached = match s.attached {
                0 => "no".to_string(),
                1 => "yes".to_string(),
                n => format!("yes ({n})"),
            };
            [printable(&s.name), attached, started(s.created), printable(&cwd), printable(&s.title)]
        })
        .collect();
    let header = ["NAME", "ATTACHED", "STARTED", "FOLDER", "TITLE"].map(String::from);
    let mut widths = [0usize; 5];
    for row in std::iter::once(&header).chain(&rows) {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    for row in std::iter::once(&header).chain(&rows) {
        let mut line = String::new();
        for (i, cell) in row.iter().enumerate() {
            line.push_str(cell);
            if i + 1 < row.len() {
                line.push_str(&" ".repeat(widths[i] - cell.chars().count() + 2));
            }
        }
        println!("{}", line.trim_end());
    }
    0
}

/// A folder's name or a title as a program chose it, safe to print: what
/// would steer the terminal it is printed on shows as a question mark.
fn printable(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { '?' } else { c }).collect()
}

/// When a session was started: the time today, else the date and time.
fn started(created: u64) -> String {
    let stamp = sys::local_stamp(created as i64);
    let today = sys::local_stamp(sys::now());
    let (date, time) = stamp.split_at(8);
    let time = format!("{}:{}", &time[..2], &time[2..4]);
    if today.starts_with(date) { time } else { format!("{}-{}-{} {time}", &date[..4], &date[4..6], &date[6..]) }
}

fn kill(args: &[String]) -> i32 {
    let request_body = match args {
        [all] if all == "--all" => Kill { name: None, all: true },
        [name] if !name.starts_with('-') => Kill { name: Some(name.clone()), all: false },
        [] => return usage_error("sessions kill wants a name, or --all"),
        [other] => return usage_error(format!("unknown option {other}")),
        _ => return usage_error("sessions kill takes one name, or --all"),
    };
    let payload = serde_json::to_vec(&request_body).unwrap_or_default();
    match request(frame::KILL, &payload) {
        Ok(Some((frame::LISTING, payload, _))) => {
            let ended: Vec<SessionInfo> = serde_json::from_slice(&payload).unwrap_or_default();
            if ended.is_empty() {
                println!("No terminal is running here.");
            }
            for session in ended {
                println!("Ended {}.", session.name);
            }
            0
        },
        Ok(Some((frame::ERROR, message, _))) => {
            eprintln!("lumen: {}", String::from_utf8_lossy(&message));
            1
        },
        Ok(Some((frame::REFUSED, payload, stream))) if request_body.all => end_other_keeper(&payload, &stream),
        Ok(Some((frame::REFUSED, payload, _))) => {
            refused(&payload);
            3
        },
        Ok(_) if request_body.all => {
            println!("No terminal is running here.");
            0
        },
        Ok(_) => {
            eprintln!("lumen: no terminal is called {}: none is running", request_body.name.unwrap_or_default());
            1
        },
        Err(e) => {
            eprintln!("lumen: {e}");
            1
        },
    }
}

/// A keeper of another lumen can't be asked in words, but every keeper ends
/// its sessions and itself on SIGTERM. Which process it is, the socket says.
fn end_other_keeper(payload: &[u8], stream: &UnixStream) -> i32 {
    let version = serde_json::from_slice::<Refused>(payload).map(|r| r.version).unwrap_or_else(|_| "another".into());
    let Some(pid) = peer_pid(stream) else {
        eprintln!("lumen: can't tell which process the keeper of lumen {version} is");
        return 1;
    };
    // SAFETY: kill only sends a signal
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        eprintln!("lumen: can't stop the keeper of lumen {version}: {}", io::Error::last_os_error());
        return 1;
    }
    println!("Ended the terminals of lumen {version}.");
    0
}

fn peer_pid(stream: &UnixStream) -> Option<i32> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: SO_PEERCRED writes one ucred of this length
    let got = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    (got == 0 && cred.pid > 0 && cred.uid == sys::uid()).then_some(cred.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_steers_no_terminal() {
        assert_eq!(printable("~/work"), "~/work");
        // a folder named to set the title of whoever lists it, and a C1 control
        assert_eq!(printable("x\x1b]0;gotcha\x07y\u{9b}z"), "x?]0;gotcha?y?z");
    }

    #[test]
    fn the_alternate_screen_is_followed() {
        let mut tail = Vec::new();
        note_screen(&mut tail, b"plain output");
        assert!(!ALT_SCREEN.load(Ordering::Relaxed));
        // cut in two by the pieces it came in
        note_screen(&mut tail, b"text\x1b[?10");
        note_screen(&mut tail, b"49h more");
        assert!(ALT_SCREEN.load(Ordering::Relaxed));
        note_screen(&mut tail, b"still there");
        assert!(ALT_SCREEN.load(Ordering::Relaxed));
        // in and out again within one piece: the later one counts
        note_screen(&mut tail, b"\x1b[?1049l\x1b[?1049h\x1b[?1049l");
        assert!(!ALT_SCREEN.load(Ordering::Relaxed));
        note_screen(&mut tail, b"\x1b[?1049h");
        assert!(ALT_SCREEN.load(Ordering::Relaxed));
        // a full reset, as a new picture starts with
        note_screen(&mut tail, b"\x1bc\x1b[3J");
        assert!(!ALT_SCREEN.load(Ordering::Relaxed));
        assert!(tail.len() <= 8);
    }
}
