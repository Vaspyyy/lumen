//! One lumen for all windows.
//!
//! The first lumen of a user answers on a socket in lumen's runtime folder,
//! `$XDG_RUNTIME_DIR/lumen/lumen.sock`. A `lumen` started while it runs
//! hands it what it was asked for: the folder, the program, its
//! environment and the launcher's focus token. The running lumen opens a
//! tab in the window that was used last (or a window, as `launch` in the
//! config and `--new-window` say) and brings it to the front. Only when no
//! lumen answers, or it says no, does the new one start from nothing.
//!
//! `lumen --daemon` is a running lumen without a window, started at login,
//! so that the first window of a session doesn't wait for the GPU, the
//! fonts and the config either. When its last window closes it lets go of
//! the display and starts a GPU for the next one.

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use gpui::{AnyWindowHandle, App, Global, GraphicalEnvironment, WindowHandle, WindowingRequest};
use serde::{Deserialize, Serialize};

use crate::config::{Config, Launch};
use crate::terminal::{Env, Spawn};
use crate::view::ViewEvent;
use crate::workspace::Workspace;

const SOCKET: &str = "lumen.sock";
/// Held by the lumen that answers on the socket, for as long as it does: two lumens
/// started at once can't both take it.
const LOCK: &str = "lumen.lock";
/// The version of what a launch and the running lumen say to each other.
const PROTOCOL: u32 = 1;
/// How long a launch waits for the running lumen to answer before it starts on its own.
const PATIENCE: Duration = Duration::from_secs(2);
/// More than any environment: a request this long is not read to its end.
const LONGEST: u64 = 1 << 20;

/// Variables of the moment or of the program a launch came from, not of the environment
/// its shell should have. They are left out, and lumen sets its own `TERM` ones.
const NOT_PASSED: &[&str] = &[
    "XDG_ACTIVATION_TOKEN",
    "DESKTOP_STARTUP_ID",
    "PWD",
    "OLDPWD",
    "SHLVL",
    "_",
    "TERM",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "WINDOWID",
    "ALACRITTY_WINDOW_ID",
];

/// What a launch asks the running lumen for.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Request {
    protocol: u32,
    version: String,
    cwd: Option<PathBuf>,
    command: Option<Vec<String>>,
    env: Vec<(String, String)>,
    /// A window of its own, not a tab.
    window: bool,
    /// The launch waits for the tab's program to end (`-e`).
    wait: bool,
    wayland_display: Option<String>,
    x11_display: Option<String>,
    xdg_runtime_dir: Option<String>,
    activation_token: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "lowercase")]
enum Answer {
    /// The tab or window is open.
    Opened,
    /// Its program has ended, for a launch that waits.
    Ended,
    /// The launch is better off starting a lumen of its own.
    Refused { why: String },
    /// What was asked for can't be done, by a lumen of its own either: the launch says why.
    Failed { why: String },
}

/// An environment as a launch passes it on: valid text only, without [`NOT_PASSED`], sorted.
fn passed(vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vars
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .filter(|(key, _)| !NOT_PASSED.contains(&key.as_str()))
        .collect();
    env.sort();
    env
}

fn socket(dir: &Path) -> PathBuf {
    dir.join(SOCKET)
}

fn send(stream: &mut UnixStream, message: &impl Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_string(message).map_err(std::io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())
}

fn receive<T: for<'de> Deserialize<'de>>(reader: &mut impl BufRead) -> Option<T> {
    let mut line = String::new();
    reader.take(LONGEST).read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

/// Hands this launch to the lumen that runs, if one does and takes it, and returns the
/// status to end with. None when this lumen has to start on its own.
pub fn hand_over(spawn: &Spawn, window: bool) -> Option<i32> {
    let dir = crate::phone::runtime_dir().ok()?;
    let mut stream = UnixStream::connect(socket(&dir)).ok()?;
    stream.set_read_timeout(Some(PATIENCE)).ok()?;
    stream.set_write_timeout(Some(PATIENCE)).ok()?;
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let request = Request {
        protocol: PROTOCOL,
        version: env!("CARGO_PKG_VERSION").into(),
        // the running lumen is somewhere else: a tab starts where `lumen` was typed
        cwd: spawn.cwd.clone().or_else(|| std::env::current_dir().ok()),
        command: spawn.command.clone(),
        env: passed(std::env::vars_os()),
        window,
        wait: spawn.command.is_some(),
        wayland_display: var("WAYLAND_DISPLAY"),
        x11_display: var("DISPLAY"),
        xdg_runtime_dir: var("XDG_RUNTIME_DIR"),
        activation_token: var("XDG_ACTIVATION_TOKEN"),
    };
    send(&mut stream, &request).ok()?;
    let mut reader = BufReader::new(stream);
    match receive::<Answer>(&mut reader)? {
        Answer::Opened => {},
        Answer::Failed { why } => {
            eprintln!("lumen: {why}");
            return Some(1);
        },
        Answer::Refused { .. } | Answer::Ended => return None,
    }
    if !request.wait {
        return Some(0);
    }
    // the program runs as long as it likes; a lumen that went away is no news of its end
    reader.get_ref().set_read_timeout(None).ok()?;
    Some(if receive::<Answer>(&mut reader) == Some(Answer::Ended) { 0 } else { 1 })
}

/// The windows, the one used last first.
#[derive(Default)]
struct Recent(Vec<AnyWindowHandle>);

impl Global for Recent {}

/// A window has come to the front: a launch opens its tab there.
pub fn note_active(window: AnyWindowHandle, cx: &mut App) {
    let recent = &mut cx.default_global::<Recent>().0;
    recent.retain(|other| *other != window);
    recent.insert(0, window);
}

/// The window a launch opens its tab in: the one used last that is still open.
fn last_window(cx: &mut App) -> Option<WindowHandle<Workspace>> {
    let open = cx.windows();
    let recent = cx.default_global::<Recent>().0.clone();
    recent.into_iter().chain(open.iter().copied()).filter(|window| open.contains(window)).find_map(|w| w.downcast())
}

fn lock(file: &File, wait: bool) -> bool {
    let how = if wait { libc::LOCK_EX } else { libc::LOCK_EX | libc::LOCK_NB };
    // SAFETY: flock on a descriptor this function borrows
    unsafe { libc::flock(file.as_raw_fd(), how) == 0 }
}

/// The user a connection comes from.
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: SO_PEERCRED fills a ucred of the size given
    let got = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    (got == 0).then_some(cred.uid)
}

/// Starts answering launches, if no other lumen does; says whether it will. A daemon waits
/// its turn instead: the lumen that answers now ends with its last window.
pub fn serve(daemon: bool, cx: &mut App) -> bool {
    let Ok(dir) = crate::phone::runtime_dir() else { return false };
    let Ok(file) = File::options().create(true).truncate(false).write(true).open(dir.join(LOCK)) else {
        return false;
    };
    if !daemon && !lock(&file, false) {
        return false;
    }
    let (tx, mut rx) = unbounded::<(Request, UnixStream)>();
    let path = socket(&dir);
    // SAFETY: getuid can't fail
    let uid = unsafe { libc::getuid() };
    let started = std::thread::Builder::new().name("launches".into()).spawn(move || {
        // the lock is this thread's, for as long as it answers
        if daemon && !lock(&file, true) {
            return;
        }
        // a socket that is left is from a lumen that ended without taking it away
        let _ = std::fs::remove_file(&path);
        let Ok(listener) = UnixListener::bind(&path) else { return };
        for stream in listener.incoming().flatten() {
            if peer_uid(&stream) != Some(uid) {
                continue;
            }
            let _ = stream.set_read_timeout(Some(PATIENCE));
            let mut reader = BufReader::new(stream);
            let Some(request) = receive::<Request>(&mut reader) else { continue };
            let stream = reader.into_inner();
            let _ = stream.set_read_timeout(None);
            if tx.unbounded_send((request, stream)).is_err() {
                return;
            }
        }
    });
    if started.is_err() {
        return false;
    }
    cx.spawn(async move |cx| {
        while let Some((request, stream)) = rx.next().await {
            cx.update(|cx| answer(request, stream, cx));
        }
    })
    .detach();
    true
}

/// Whether the program file was replaced since this lumen started: an update.
fn replaced() -> bool {
    std::fs::read_link("/proc/self/exe").is_ok_and(|exe| exe.to_string_lossy().ends_with(" (deleted)"))
}

/// The environment for a launch's terminals: None where it is lumen's own anyway, so that
/// a shell started ahead of time fits it.
fn launch_env(env: Vec<(String, String)>) -> Option<Env> {
    (env != passed(std::env::vars_os())).then(|| Arc::new(env))
}

fn same_display(display: &GraphicalEnvironment, request: &Request) -> bool {
    let same = |ours: &Option<std::ffi::OsString>, theirs: &Option<String>| {
        ours.as_deref().filter(|v| !v.is_empty()) == theirs.as_deref().map(std::ffi::OsStr::new)
    };
    same(&display.wayland_display, &request.wayland_display) && same(&display.x11_display, &request.x11_display)
}

fn refused(why: impl Into<String>) -> Answer {
    Answer::Refused { why: why.into() }
}

fn answer(mut request: Request, mut stream: UnixStream, cx: &mut App) {
    if request.protocol != PROTOCOL || request.version != env!("CARGO_PKG_VERSION") {
        let _ = send(&mut stream, &refused("another version of lumen"));
        // a daemon from before an update makes way for the new program: its service starts it
        if cx.windows().is_empty() && replaced() {
            cx.quit();
        }
        return;
    }
    if cx.global::<Config>().launch == Launch::Process {
        let _ = send(&mut stream, &refused("launch = \"process\""));
        return;
    }
    let spawn = Spawn {
        cwd: request.cwd.take(),
        command: request.command.take(),
        env: launch_env(std::mem::take(&mut request.env)),
    };
    match cx.graphical_environment() {
        Some(display) if !same_display(&display, &request) => {
            let _ = send(&mut stream, &refused("lumen runs on another display"));
        },
        Some(_) => open(request, spawn, stream, cx),
        // the daemon, with no window: it goes to the launch's display, with its focus token
        None => {
            let display = GraphicalEnvironment {
                wayland_display: request.wayland_display.clone().map(Into::into),
                x11_display: request.x11_display.clone().map(Into::into),
                xdg_runtime_dir: request.xdg_runtime_dir.clone().map(Into::into),
                activation_token: request.activation_token.take(),
            };
            let switch = cx.request_windowing(WindowingRequest::Windowed(display));
            cx.spawn(async move |cx| match switch.await {
                Ok(()) => cx.update(|cx| open(request, spawn, stream, cx)),
                Err(e) => {
                    let _ = send(&mut stream, &refused(format!("can't reach the display: {e}")));
                },
            })
            .detach();
        },
    }
}

/// Opens the tab or window a launch asked for, and tells the launch.
fn open(request: Request, spawn: Spawn, mut stream: UnixStream, cx: &mut App) {
    if let Some(token) = request.activation_token {
        gpui_linux::give_activation_token(token);
    }
    let tab_in = if !request.window && cx.global::<Config>().launch == Launch::Tab { last_window(cx) } else { None };
    let pane = match tab_in {
        Some(window) => window
            .update(cx, |workspace, window, cx| {
                let pane = workspace.open_launched(spawn, window, cx);
                window.activate_window();
                pane
            })
            .map_err(|e| e.to_string())
            .and_then(|pane| pane.map_err(|e| format!("can't start the shell: {e}"))),
        None => crate::open_window(spawn, cx).and_then(|window| {
            window
                .read_with(cx, |workspace, _| workspace.active_pane())
                .ok()
                .flatten()
                .ok_or_else(|| "the window has no terminal".to_string())
        }),
    };
    let pane = match pane {
        Ok(pane) => pane,
        Err(e) => {
            let _ = send(&mut stream, &Answer::Failed { why: e });
            return;
        },
    };
    if send(&mut stream, &Answer::Opened).is_err() || !request.wait {
        return;
    }
    // the tab's program ended, or the tab was closed and took it along: whichever is heard first
    let waiting = Rc::new(RefCell::new(Some(stream)));
    let ended = |waiting: &Rc<RefCell<Option<UnixStream>>>| {
        if let Some(mut stream) = waiting.borrow_mut().take() {
            let _ = send(&mut stream, &Answer::Ended);
        }
    };
    cx.subscribe(&pane, {
        let waiting = waiting.clone();
        move |_, event: &ViewEvent, _| {
            if matches!(event, ViewEvent::Closed) {
                ended(&waiting);
            }
        }
    })
    .detach();
    cx.observe_release(&pane, move |_, _| ended(&waiting)).detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_passes_its_environment_without_what_belongs_to_the_moment() {
        let vars = [("PATH", "/usr/bin"), ("XDG_ACTIVATION_TOKEN", "t"), ("TERM", "xterm"), ("HOME", "/home/a")];
        let env = passed(vars.iter().map(|(k, v)| ((*k).into(), (*v).into())));
        assert_eq!(env, vec![("HOME".into(), "/home/a".into()), ("PATH".into(), "/usr/bin".into())]);
    }

    #[test]
    fn requests_and_answers_are_one_line_each() {
        let request = Request {
            protocol: PROTOCOL,
            version: "1.0.0".into(),
            cwd: Some("/tmp".into()),
            command: Some(vec!["htop".into()]),
            env: vec![("A".into(), "b\nc".into())],
            window: false,
            wait: true,
            wayland_display: Some("wayland-1".into()),
            x11_display: None,
            xdg_runtime_dir: Some("/run/user/1000".into()),
            activation_token: Some("token".into()),
        };
        let line = serde_json::to_string(&request).unwrap();
        assert!(!line.contains('\n'));
        let back: Request = receive(&mut format!("{line}\n").as_bytes()).unwrap();
        assert_eq!(back, request);
        for answer in [Answer::Opened, Answer::Ended, refused("no"), Answer::Failed { why: "gone".into() }] {
            let line = serde_json::to_string(&answer).unwrap();
            assert_eq!(receive::<Answer>(&mut format!("{line}\n").as_bytes()), Some(answer));
        }
    }

    #[test]
    fn a_launch_on_another_display_is_not_taken() {
        let request = |wayland: Option<&str>, x11: Option<&str>| Request {
            protocol: PROTOCOL,
            version: String::new(),
            cwd: None,
            command: None,
            env: Vec::new(),
            window: false,
            wait: false,
            wayland_display: wayland.map(Into::into),
            x11_display: x11.map(Into::into),
            xdg_runtime_dir: None,
            activation_token: None,
        };
        let ours = GraphicalEnvironment {
            wayland_display: Some("wayland-1".into()),
            x11_display: Some(":1".into()),
            ..Default::default()
        };
        assert!(same_display(&ours, &request(Some("wayland-1"), Some(":1"))));
        assert!(!same_display(&ours, &request(Some("wayland-2"), Some(":1"))));
        assert!(!same_display(&ours, &request(None, Some(":1"))));
    }
}
