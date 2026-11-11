//! `lumen keeper`: the process that keeps a user's terminals running while
//! nobody looks at them.
//!
//! Each session is a PTY with a shell and an `alacritty_terminal::Term`
//! that follows everything the shell prints, so that a terminal attaching
//! later can be given the whole picture (see [`super::replay`]). Clients
//! (`lumen attach`) connect over a unix socket; what the program prints goes
//! to all of them, what any of them types goes to the program.
//!
//! One lock per session orders everything: output is parsed into the
//! `Term` and queued for the clients under it, and a client is given its
//! replay and added under it, so nothing is lost or shown twice in between.

use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind};
use std::num::NonZeroUsize;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use lumen_link::SessionInfo;
use polling::{Event as Interest, Events as Ready, PollMode, Poller};

use super::frame::{self, Hello, Kill, Refused};
use super::replay::replay;
use super::sys::{self, Winsize};
use super::{runtime_dir, usage_error};

/// How far a client may fall behind before what it hasn't been sent is
/// thrown away and it gets a fresh picture instead.
const CLIENT_QUEUE: usize = 4 << 20;
/// The most columns or rows a terminal is given, whatever a client asks for.
const MAX_SIDE: u16 = 1000;
const SCROLLBACK: usize = 5000;
const READ_CHUNK: usize = 64 * 1024;
/// How much output is taken in one go before input gets a turn.
const MAX_READ: usize = 1 << 20;
const PTY: usize = 0;
const CHILD: usize = 1;

pub fn socket_path(dir: &Path) -> PathBuf {
    dir.join("keeper.sock")
}

pub fn keeper(args: &[String]) -> i32 {
    if let Some(arg) = args.first() {
        return usage_error(format!("unknown option {arg}"));
    }
    let dir = match runtime_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("lumen: {e}");
            return 1;
        },
    };
    // one keeper per user: the lock says so even while the socket is being replaced
    let lock = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join("keeper.lock"))
    {
        Ok(file) => file,
        Err(e) => {
            eprintln!("lumen: {e}");
            return 1;
        },
    };
    // SAFETY: flock on a descriptor this function owns
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        eprintln!("lumen: a keeper is running already");
        return 1;
    }
    let socket = socket_path(&dir);
    if UnixStream::connect(&socket).is_ok() {
        eprintln!("lumen: a keeper is running already");
        return 1;
    }
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("lumen: can't listen on {}: {e}", socket.display());
            return 1;
        },
    };

    // the folder it was started in may be one somebody wants to unmount
    let _ = std::env::set_current_dir("/");
    let signals = [libc::SIGTERM, libc::SIGHUP, libc::SIGINT];
    sys::block_signals(&signals);
    let keeper = Arc::new(Keeper { state: Mutex::new(State::default()), socket, _lock: lock });
    {
        let keeper = keeper.clone();
        std::thread::spawn(move || {
            sys::wait_signal(&signals);
            keeper.shut_down();
        });
    }
    {
        // started for a client that never came: not kept around for nothing
        let keeper = keeper.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(10));
            keeper.exit_if_idle(&keeper.state());
        });
    }

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        keeper.state().connections += 1;
        let keeper = keeper.clone();
        std::thread::spawn(move || {
            keeper.serve(stream);
            let mut state = keeper.state();
            state.connections -= 1;
            keeper.exit_if_idle(&state);
        });
    }
    1
}

struct Keeper {
    state: Mutex<State>,
    socket: PathBuf,
    _lock: File,
}

#[derive(Default)]
struct State {
    sessions: HashMap<String, Arc<Session>>,
    connections: usize,
}

impl Keeper {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Ends the process once there is nothing left to keep and nobody to
    /// keep it for. The socket goes first, so no client connects to a
    /// keeper that is on its way out.
    fn exit_if_idle(&self, state: &State) {
        if state.sessions.is_empty() && state.connections == 0 {
            let _ = std::fs::remove_file(&self.socket);
            std::process::exit(0);
        }
    }

    /// On SIGTERM: every session is ended, then the keeper. A lumen of
    /// another version can always ask for this, whatever it speaks.
    fn shut_down(&self) {
        let sessions: Vec<Arc<Session>> = self.state().sessions.values().cloned().collect();
        for session in &sessions {
            session.signal(libc::SIGHUP);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && sessions.iter().any(|s| !s.ended()) {
            std::thread::sleep(Duration::from_millis(50));
        }
        for session in &sessions {
            if !session.ended() {
                session.signal(libc::SIGKILL);
            }
        }
        let _ = std::fs::remove_file(&self.socket);
        std::process::exit(0);
    }

    fn serve(self: &Arc<Self>, mut stream: UnixStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let Ok(Some((frame::VERSION, payload))) = frame::read(&mut stream) else { return };
        if frame::parse_version(&payload).map(|(protocol, _)| protocol) != Some(frame::PROTOCOL) {
            let refused = Refused { protocol: frame::PROTOCOL, version: env!("CARGO_PKG_VERSION").into() };
            let _ = frame::write(&mut stream, frame::REFUSED, &serde_json::to_vec(&refused).unwrap_or_default());
            return;
        }
        let Ok(Some((kind, payload))) = frame::read(&mut stream) else { return };
        match kind {
            frame::LIST => {
                let list = self.list();
                let _ = frame::write(&mut stream, frame::LISTING, &serde_json::to_vec(&list).unwrap_or_default());
            },
            frame::KILL => {
                let kill: Kill = serde_json::from_slice(&payload).unwrap_or_default();
                match self.kill(&kill) {
                    Ok(ended) => {
                        let _ =
                            frame::write(&mut stream, frame::LISTING, &serde_json::to_vec(&ended).unwrap_or_default());
                    },
                    Err(message) => {
                        let _ = frame::write(&mut stream, frame::ERROR, message.as_bytes());
                    },
                }
            },
            frame::HELLO => match serde_json::from_slice::<Hello>(&payload) {
                Ok(hello) => self.attach(stream, hello),
                Err(_) => {
                    let _ = frame::write(&mut stream, frame::ERROR, b"the keeper didn't understand the request");
                },
            },
            _ => {},
        }
    }

    fn list(&self) -> Vec<SessionInfo> {
        let mut list: Vec<SessionInfo> = self.state().sessions.values().map(|s| s.info()).collect();
        list.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.name.cmp(&b.name)));
        list
    }

    fn kill(&self, kill: &Kill) -> Result<Vec<SessionInfo>, String> {
        let sessions: Vec<Arc<Session>> = {
            let state = self.state();
            match &kill.name {
                Some(name) if !kill.all => match state.sessions.get(name) {
                    Some(session) => vec![session.clone()],
                    None => {
                        let mut names: Vec<&String> = state.sessions.keys().collect();
                        names.sort();
                        let names: Vec<String> = names.into_iter().map(|n| n.to_string()).collect();
                        return Err(if names.is_empty() {
                            format!("no terminal is called {name}: none is running")
                        } else {
                            format!("no terminal is called {name}. Running: {}", names.join(", "))
                        });
                    },
                },
                _ => state.sessions.values().cloned().collect(),
            }
        };
        let infos = sessions.iter().map(|s| s.info()).collect();
        for session in sessions {
            session.signal(libc::SIGHUP);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                if !session.ended() {
                    session.signal(libc::SIGKILL);
                }
            });
        }
        Ok(infos)
    }

    fn attach(self: &Arc<Self>, mut stream: UnixStream, hello: Hello) {
        let _ = stream.set_read_timeout(None);
        if !lumen_link::session_name_ok(&hello.session) {
            let _ = frame::write(&mut stream, frame::ERROR, b"that is not a name a terminal can have");
            return;
        }
        let size = Winsize {
            cols: hello.cols.clamp(1, MAX_SIDE),
            rows: hello.rows.clamp(1, MAX_SIDE),
            width: hello.width,
            height: hello.height,
        };
        let client = Arc::new(Client::default());
        let session = loop {
            let session = {
                let mut state = self.state();
                match state.sessions.get(&hello.session) {
                    Some(session) => session.clone(),
                    None => match Session::start(self.clone(), &hello, size) {
                        Ok(session) => {
                            state.sessions.insert(hello.session.clone(), session.clone());
                            session
                        },
                        Err(e) => {
                            let message = format!("can't start the shell: {e}");
                            let _ = frame::write(&mut stream, frame::ERROR, message.as_bytes());
                            return;
                        },
                    },
                }
            };
            if session.attach(&client, size, hello.takeover) {
                break session;
            }
            // it ended just now: a new one of the same name
            self.forget(&session);
        };

        let Ok(writer) = stream.try_clone() else {
            session.detach(&client);
            return;
        };
        let sender = {
            let client = client.clone();
            std::thread::spawn(move || client.send_all(writer))
        };
        loop {
            match frame::read(&mut stream) {
                Ok(Some((frame::INPUT, bytes))) => session.input(bytes),
                Ok(Some((frame::RESIZE, payload))) => {
                    if let Some((cols, rows, width, height)) = frame::decode_size(&payload) {
                        let (cols, rows) = (cols.clamp(1, MAX_SIDE), rows.clamp(1, MAX_SIDE));
                        session.resize(Winsize { cols, rows, width, height });
                    }
                },
                Ok(Some(_)) => {},
                Ok(None) | Err(_) => break,
            }
        }
        session.detach(&client);
        client.close();
        let _ = sender.join();
    }

    /// Takes an ended session off the list, unless a new one has its name.
    fn forget(&self, session: &Arc<Session>) {
        let mut state = self.state();
        if state.sessions.get(&session.name).is_some_and(|s| Arc::ptr_eq(s, session)) {
            state.sessions.remove(&session.name);
        }
        self.exit_if_idle(&state);
    }
}

/// Whether this is the answer to "which kitty keyboard flags are on?".
fn kitty_answer(text: &str) -> bool {
    text.strip_prefix("\x1b[?")
        .and_then(|rest| rest.strip_suffix('u'))
        .is_some_and(|flags| !flags.is_empty() && flags.bytes().all(|b| b.is_ascii_digit()))
}

/// What the keeper has yet to send one client.
#[derive(Default)]
struct Client {
    queue: Mutex<Queue>,
    ready: Condvar,
}

#[derive(Default)]
struct Queue {
    frames: VecDeque<(u8, Vec<u8>, bool)>,
    /// Bytes of ordinary output waiting (a replay doesn't count).
    output: usize,
    closed: bool,
}

impl Client {
    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, kind: u8, payload: Vec<u8>) {
        self.queue().frames.push_back((kind, payload, false));
        self.ready.notify_one();
    }

    /// Queues output, unless the client is too far behind: then `false`,
    /// and the caller gives it a replay instead.
    fn push_output(&self, bytes: &[u8]) -> bool {
        let mut queue = self.queue();
        if queue.output + bytes.len() > CLIENT_QUEUE {
            return false;
        }
        queue.output += bytes.len();
        queue.frames.push_back((frame::OUTPUT, bytes.to_vec(), true));
        drop(queue);
        self.ready.notify_one();
        true
    }

    /// Throws away the output it hasn't been sent and queues a replay.
    fn replace_output(&self, replay: Vec<u8>) {
        let mut queue = self.queue();
        queue.frames.retain(|(kind, _, _)| *kind != frame::OUTPUT);
        queue.output = 0;
        queue.frames.push_back((frame::OUTPUT, replay, false));
        drop(queue);
        self.ready.notify_one();
    }

    fn close(&self) {
        self.queue().closed = true;
        self.ready.notify_all();
    }

    /// Sends what is queued until the client is closed or the session's end
    /// has been sent.
    fn send_all(&self, mut socket: UnixStream) {
        loop {
            let (kind, payload) = {
                let mut queue = self.queue();
                loop {
                    if let Some((kind, payload, counted)) = queue.frames.pop_front() {
                        if counted {
                            queue.output -= payload.len();
                        }
                        break (kind, payload);
                    }
                    if queue.closed {
                        return;
                    }
                    queue = self.ready.wait(queue).unwrap_or_else(|e| e.into_inner());
                }
            };
            if frame::write(&mut socket, kind, &payload).is_err() {
                self.close();
                let _ = socket.shutdown(std::net::Shutdown::Both);
                return;
            }
            if kind == frame::EXIT {
                // the reading side notices the end and lets go of the client
                let _ = socket.shutdown(std::net::Shutdown::Both);
                return;
            }
        }
    }
}

/// Collects what the `Term` has to say while it parses, to be dealt with
/// once the parse is done.
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<Event>>>);

impl EventListener for Events {
    fn send_event(&self, event: Event) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    }
}

struct Session {
    name: String,
    created: u64,
    shell_pid: i32,
    master: OwnedFd,
    inner: Mutex<Inner>,
    input: mpsc::Sender<Vec<u8>>,
    poller: Arc<Poller>,
}

struct Inner {
    term: Term<Events>,
    events: Events,
    parser: Processor,
    clients: Vec<Arc<Client>>,
    size: Winsize,
    title: Option<String>,
    ended: bool,
    /// Answers for the program, written by the I/O thread.
    answers: Vec<u8>,
}

impl Session {
    fn start(keeper: Arc<Keeper>, hello: &Hello, size: Winsize) -> io::Result<Arc<Session>> {
        let (master, slave) = open_pty(size)?;
        let child = spawn_shell(hello, &slave)?;
        drop(slave);
        set_nonblocking(master.as_raw_fd());
        let shell_pid = child.id() as i32;

        let events = Events::default();
        let config = TermConfig { scrolling_history: SCROLLBACK, kitty_keyboard: true, ..TermConfig::default() };
        let term = Term::new(config, &term_size(size), events.clone());
        let (tx, rx) = mpsc::channel();
        let poller = Arc::new(Poller::new()?);
        let session = Arc::new(Session {
            name: hello.session.clone(),
            created: sys::now().max(0) as u64,
            shell_pid,
            master,
            inner: Mutex::new(Inner {
                term,
                events,
                parser: Processor::new(),
                clients: Vec::new(),
                size,
                title: None,
                ended: false,
                answers: Vec::new(),
            }),
            input: tx,
            poller,
        });
        let io = session.clone();
        std::thread::Builder::new().name(format!("session {}", hello.session)).spawn(move || {
            let status = io.run(child, rx);
            io.end(status);
            keeper.forget(&io);
        })?;
        Ok(session)
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn ended(&self) -> bool {
        self.inner().ended
    }

    fn info(&self) -> SessionInfo {
        let inner = self.inner();
        SessionInfo {
            name: self.name.clone(),
            title: inner.title.clone().unwrap_or_default(),
            cwd: std::fs::read_link(format!("/proc/{}/cwd", self.shell_pid))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            attached: inner.clients.len() as u32,
            created: self.created,
        }
    }

    /// A signal for the shell's process group.
    fn signal(&self, signal: i32) {
        // SAFETY: killpg only sends a signal
        unsafe { libc::killpg(self.shell_pid, signal) };
    }

    /// Gives the client the picture and adds it, or `false` if the session
    /// has ended.
    fn attach(&self, client: &Arc<Client>, size: Winsize, takeover: bool) -> bool {
        let mut inner = self.inner();
        if inner.ended {
            return false;
        }
        if takeover {
            for other in inner.clients.drain(..) {
                other.push(frame::MOVED, Vec::new());
            }
        }
        self.resize_locked(&mut inner, size);
        let inner = &mut *inner;
        // output held back for a synchronized update belongs to the picture
        if inner.parser.sync_bytes_count() > 0 {
            inner.parser.stop_sync(&mut inner.term);
            self.handle_events(inner);
        }
        // A sequence the program is in the middle of printing is given up, here
        // and so for the client too: its rest would come after the picture, where
        // only this side would still take it for the end of a sequence.
        inner.parser.advance(&mut inner.term, &[0x18]);
        let bytes = replay(&mut inner.term, inner.title.as_deref());
        client.push(frame::WELCOME, Vec::new());
        client.replace_output(bytes);
        inner.clients.push(client.clone());
        if inner.term.mode().contains(TermMode::ALT_SCREEN) {
            self.repaint();
        }
        true
    }

    fn detach(&self, client: &Arc<Client>) {
        self.inner().clients.retain(|c| !Arc::ptr_eq(c, client));
    }

    /// Makes the program on the alternate screen draw itself again: what it
    /// showed there is not part of the replay's picture for sure.
    fn repaint(&self) {
        // SAFETY: tcgetpgrp only reads, kill only sends a signal
        unsafe {
            let group = libc::tcgetpgrp(self.master.as_raw_fd());
            if group > 0 {
                libc::kill(-group, libc::SIGWINCH);
            }
        }
    }

    fn input(&self, bytes: Vec<u8>) {
        if self.input.send(bytes).is_ok() {
            let _ = self.poller.notify();
        }
    }

    fn resize(&self, size: Winsize) {
        let mut inner = self.inner();
        self.resize_locked(&mut inner, size);
    }

    fn resize_locked(&self, inner: &mut Inner, size: Winsize) {
        if inner.size == size {
            return;
        }
        inner.size = size;
        inner.term.resize(term_size(size));
        let _ = sys::set_winsize(self.master.as_raw_fd(), size);
    }

    /// Takes what the program printed: into the model, out to every client.
    fn output(&self, bytes: &[u8]) {
        let mut inner = self.inner();
        let inner = &mut *inner;
        inner.parser.advance(&mut inner.term, bytes);
        let behind: Vec<Arc<Client>> = inner.clients.iter().filter(|c| !c.push_output(bytes)).cloned().collect();
        if !behind.is_empty() {
            let picture = replay(&mut inner.term, inner.title.as_deref());
            for client in behind {
                client.replace_output(picture.clone());
            }
            if inner.term.mode().contains(TermMode::ALT_SCREEN) {
                self.repaint();
            }
        }
        self.handle_events(inner);
    }

    /// What the `Term` said while parsing. Questions the program asked the
    /// terminal are answered here only while nobody is attached: an attached
    /// client's own terminal sees the question too and answers it.
    fn handle_events(&self, inner: &mut Inner) {
        let events = std::mem::take(&mut *inner.events.0.lock().unwrap_or_else(|e| e.into_inner()));
        let alone = inner.clients.is_empty();
        for event in events {
            match event {
                Event::Title(title) => inner.title = Some(title),
                Event::ResetTitle => inner.title = None,
                // Whether the kitty keyboard protocol is spoken is for the terminal
                // that attaches next to say, and a phone does not speak it: the
                // question stays open, which a program takes for a no.
                Event::PtyWrite(text) if alone && kitty_answer(&text) => {},
                Event::PtyWrite(text) if alone => {
                    // the same answer lumen's own window gives (see `crate::terminal`)
                    let text = if text == "\x1b[?6c" { "\x1b[?62;22c".to_string() } else { text };
                    inner.answers.extend_from_slice(text.as_bytes());
                },
                Event::TextAreaSizeRequest(format) if alone => {
                    let size = inner.size;
                    let window = WindowSize {
                        num_lines: size.rows,
                        num_cols: size.cols,
                        cell_width: size.width / size.cols.max(1),
                        cell_height: size.height / size.rows.max(1),
                    };
                    inner.answers.extend_from_slice(format(window).as_bytes());
                },
                _ => {},
            }
        }
    }

    /// The I/O loop: reads the PTY, writes to it, waits for the program to
    /// end. Returns its exit status.
    fn run(&self, mut child: Child, input: Receiver<Vec<u8>>) -> i32 {
        let fd = self.master.as_raw_fd();
        // SAFETY: the descriptor stays open as long as the session, which outlives this loop
        let pty = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
        let mut interest = Interest::readable(PTY);
        // SAFETY: deregistered before the descriptor could close
        if unsafe { self.poller.add_with_mode(&pty, interest, PollMode::Level) }.is_err() {
            return wait_status(&mut child);
        }
        let pidfd = pidfd(child.id() as i32);
        if let Some(pidfd) = &pidfd {
            // SAFETY: the pidfd outlives its registration, taken back below
            let _ = unsafe { self.poller.add_with_mode(pidfd, Interest::readable(CHILD), PollMode::Level) };
        }
        let mut events = Ready::with_capacity(NonZeroUsize::new(8).unwrap_or(NonZeroUsize::MIN));
        let mut pending: VecDeque<u8> = VecDeque::new();
        let mut buf = vec![0u8; READ_CHUNK];
        let mut hung_up = false;
        let status = loop {
            events.clear();
            let timeout = if pidfd.is_some() { None } else { Some(Duration::from_millis(250)) };
            if let Err(e) = self.poller.wait(&mut events, timeout)
                && e.kind() != ErrorKind::Interrupted
            {
                break wait_status(&mut child);
            }
            pending.extend(input.try_iter().flatten());
            let mut readable = false;
            let mut writable = false;
            for event in events.iter() {
                match event.key {
                    PTY => {
                        readable |= event.readable;
                        writable |= event.writable;
                    },
                    CHILD => {},
                    _ => {},
                }
            }
            if readable && !hung_up {
                hung_up = self.read_all(&mut buf).is_err();
            }
            pending.extend(std::mem::take(&mut self.inner().answers));
            if writable || !pending.is_empty() {
                write_pending(fd, &mut pending);
            }
            if let Ok(Some(status)) = child.try_wait() {
                // what the program printed last is still to be read
                let _ = self.read_all(&mut buf);
                break exit_code(status);
            }
            let wanted = match (!hung_up, !pending.is_empty()) {
                (true, true) => Interest::all(PTY),
                (true, false) => Interest::readable(PTY),
                (false, true) => Interest::writable(PTY),
                (false, false) => Interest::none(PTY),
            };
            if (wanted.readable, wanted.writable) != (interest.readable, interest.writable) {
                interest = wanted;
                let _ = self.poller.modify_with_mode(pty, interest, PollMode::Level);
            }
        };
        let _ = self.poller.delete(pty);
        if let Some(pidfd) = &pidfd {
            let _ = self.poller.delete(pidfd);
        }
        status
    }

    /// Reads until there is nothing more for now. An error means the
    /// program's side of the PTY is closed.
    fn read_all(&self, buf: &mut [u8]) -> io::Result<()> {
        let mut total = 0;
        loop {
            // SAFETY: reads into a buffer of this length
            let got = unsafe { libc::read(self.master.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if got > 0 {
                self.output(&buf[..got as usize]);
                total += got as usize;
                if total >= MAX_READ {
                    return Ok(());
                }
                continue;
            }
            if got == 0 {
                return Err(ErrorKind::UnexpectedEof.into());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                ErrorKind::WouldBlock => return Ok(()),
                ErrorKind::Interrupted => continue,
                _ => return Err(error),
            }
        }
    }

    /// The program ended: the clients are told, and let go of.
    fn end(&self, status: i32) {
        let mut inner = self.inner();
        inner.ended = true;
        for client in inner.clients.drain(..) {
            client.push(frame::EXIT, status.to_be_bytes().to_vec());
        }
    }
}

fn term_size(size: Winsize) -> crate::terminal::TermSize {
    let cols = size.cols.max(1) as usize;
    let rows = size.rows.max(1) as usize;
    let cell = |pixels: u16, cells: usize| if pixels == 0 { 0. } else { pixels as f32 / cells as f32 };
    crate::terminal::TermSize { cols, rows, cell_width: cell(size.width, cols), cell_height: cell(size.height, rows) }
}

fn write_pending(fd: RawFd, pending: &mut VecDeque<u8>) {
    while !pending.is_empty() {
        let (front, _) = pending.as_slices();
        // SAFETY: writes from a slice of this length
        let wrote = unsafe { libc::write(fd, front.as_ptr().cast(), front.len()) };
        if wrote > 0 {
            pending.drain(..wrote as usize);
            continue;
        }
        let error = io::Error::last_os_error();
        if wrote < 0 && error.kind() == ErrorKind::Interrupted {
            continue;
        }
        if wrote < 0 && error.kind() != ErrorKind::WouldBlock {
            // the program is gone: nobody to write to
            pending.clear();
        }
        return;
    }
}

fn wait_status(child: &mut Child) -> i32 {
    child.wait().map(exit_code).unwrap_or(1)
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    status.code().or_else(|| status.signal().map(|s| 128 + s)).unwrap_or(1)
}

/// A descriptor that becomes readable when the process ends.
fn pidfd(pid: i32) -> Option<OwnedFd> {
    // SAFETY: pidfd_open takes a pid and flags and returns a new descriptor
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    // SAFETY: a descriptor the call just made, owned by nobody else
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
}

fn set_nonblocking(fd: RawFd) {
    // SAFETY: fcntl on a descriptor this process owns
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

fn open_pty(size: Winsize) -> io::Result<(OwnedFd, OwnedFd)> {
    let (mut master, mut slave) = (-1, -1);
    let ws = libc::winsize { ws_col: size.cols, ws_row: size.rows, ws_xpixel: size.width, ws_ypixel: size.height };
    // SAFETY: openpty writes two descriptors; no name buffer, no termios
    if unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &ws) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descriptors openpty just made, owned by nobody else
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    // SAFETY: fcntl and tcgetattr/tcsetattr on descriptors owned here
    unsafe {
        // not for the shell or anything else this process starts
        libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        if libc::tcgetattr(master.as_raw_fd(), termios.as_mut_ptr()) == 0 {
            let mut termios = termios.assume_init();
            termios.c_iflag |= libc::IUTF8;
            libc::tcsetattr(master.as_raw_fd(), libc::TCSANOW, &termios);
        }
    }
    Ok((master, slave))
}

/// Whether an environment variable's name is one `--env` may set.
pub fn env_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// What the session's shell gets for an environment: the keeper's own,
/// without what describes the SSH login it may have been started from or
/// the terminal it was started in, plus what a terminal says about itself.
fn shell_env(hello: &Hello) -> Vec<(String, String)> {
    const DROPPED: [&str; 5] = ["SSH_TTY", "SSH_CONNECTION", "SSH_CLIENT", "TERM_PROGRAM", "TERM_PROGRAM_VERSION"];
    let mut env: Vec<(String, String)> =
        std::env::vars().filter(|(name, _)| !DROPPED.contains(&name.as_str())).collect();
    let mut set = |name: &str, value: &str| {
        env.retain(|(n, _)| n != name);
        env.push((name.to_string(), value.to_string()));
    };
    set("TERM", "xterm-256color");
    set("COLORTERM", "truecolor");
    set("LUMEN_SESSION", &hello.session);
    for (name, value) in &hello.env {
        if env_name_ok(name) && !value.contains('\0') {
            set(name, value);
        }
    }
    env
}

fn spawn_shell(hello: &Hello, slave: &OwnedFd) -> io::Result<Child> {
    let user = sys::user()?;
    // for tests: a shell that is the same on every machine, whatever the user's is
    let shell = std::env::var("LUMEN_SESSION_SHELL").ok().filter(|s| !s.is_empty()).unwrap_or(user.shell);
    let cwd = hello.cwd.as_ref().map(PathBuf::from).filter(|dir| dir.is_dir()).unwrap_or_else(|| user.home.clone());
    let stdio = || slave.try_clone().map(Stdio::from);
    let mut command = Command::new(&shell);
    command
        .arg("-l")
        .env_clear()
        .envs(shell_env(hello))
        .current_dir(cwd)
        .stdin(stdio()?)
        .stdout(stdio()?)
        .stderr(stdio()?);
    // SAFETY: only async-signal-safe calls between fork and exec
    unsafe {
        command.pre_exec(|| {
            // a session of its own, with the PTY as its terminal
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            let mut none = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            libc::sigemptyset(none.as_mut_ptr());
            libc::pthread_sigmask(libc::SIG_SETMASK, none.as_ptr(), std::ptr::null_mut());
            Ok(())
        });
    }
    command.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_env_may_set() {
        for good in ["TERM_PROGRAM", "_X", "A1", "LC_ALL"] {
            assert!(env_name_ok(good), "{good}");
        }
        for bad in ["", "1A", "term", "A-B", "A=B", "Ä"] {
            assert!(!env_name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn the_kitty_keyboard_question_is_left_open() {
        assert!(kitty_answer("\x1b[?0u"));
        assert!(kitty_answer("\x1b[?31u"));
        // what a terminal is, where the cursor is: answered
        for other in ["\x1b[?62;22c", "\x1b[1;3R", "\x1b[?u", "\x1b[?1;2u", "\x1b[0n"] {
            assert!(!kitty_answer(other), "{other:?}");
        }
    }

    #[test]
    fn the_shells_environment() {
        let hello = Hello {
            session: "phone".into(),
            cols: 80,
            rows: 24,
            width: 0,
            height: 0,
            env: vec![("TERM_PROGRAM".into(), "lumen".into()), ("bad name".into(), "x".into())],
            cwd: None,
            takeover: false,
        };
        let env = shell_env(&hello);
        let get = |name: &str| env.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
        assert_eq!(get("TERM"), Some("xterm-256color"));
        assert_eq!(get("COLORTERM"), Some("truecolor"));
        assert_eq!(get("LUMEN_SESSION"), Some("phone"));
        assert_eq!(get("TERM_PROGRAM"), Some("lumen"));
        assert_eq!(get("bad name"), None);
        assert_eq!(get("SSH_TTY"), None);
        assert_eq!(env.iter().filter(|(n, _)| n == "TERM").count(), 1);
    }

    #[test]
    fn a_slow_client_gets_a_picture_instead() {
        let client = Client::default();
        client.push(frame::WELCOME, Vec::new());
        assert!(client.push_output(&vec![b'x'; CLIENT_QUEUE - 10]));
        assert!(!client.push_output(&[b'y'; 11]));
        client.replace_output(b"picture".to_vec());
        let queue = client.queue();
        let kinds: Vec<u8> = queue.frames.iter().map(|(k, _, _)| *k).collect();
        assert_eq!(kinds, [frame::WELCOME, frame::OUTPUT]);
        assert_eq!(queue.frames[1].1, b"picture");
        assert_eq!(queue.output, 0);
    }
}
