//! What the integration tests share: scratch folders, a terminal of their
//! own, waiting for things to happen.

#![allow(dead_code)]

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub fn lumen() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lumen"))
}

/// A folder of the test's own, emptied first. Kept short: the sockets in it
/// have to fit in a unix socket's 108 bytes.
pub fn scratch(name: &str) -> PathBuf {
    let base = std::env::var_os("LUMEN_TEST_SCRATCH").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("lt-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// A `lumen` command that sees only the scratch folder: its own runtime
/// folder, `/bin/sh` for the sessions' shell, and no user service manager
/// to start a keeper with (so the test doesn't reach the real one).
pub fn command(scratch: &Path) -> Command {
    let runtime = scratch.join("run");
    std::fs::create_dir_all(&runtime).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = Command::new(lumen());
    command
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("LUMEN_SESSION_SHELL", "/bin/sh")
        .env("HOME", scratch)
        .env("ENV", "/dev/null")
        .env("PS1", "$ ")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY");
    command
}

/// Waits for `check` to be true, at most this long.
pub fn wait_for(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for a process to end, at most this long, and gives its status.
pub fn wait_exit(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("process {} did not end in time", child.id());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A terminal the test owns: a PTY whose program side a `lumen attach`
/// gets, and whose screen side the test reads and types into.
pub struct Pty {
    master: Option<OwnedFd>,
    slave: Option<OwnedFd>,
    pub output: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl Pty {
    pub fn open(cols: u16, rows: u16) -> Pty {
        let (mut master, mut slave) = (-1, -1);
        let ws = libc::winsize { ws_col: cols, ws_row: rows, ws_xpixel: cols * 8, ws_ypixel: rows * 16 };
        // SAFETY: openpty writes two descriptors
        assert_eq!(unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &ws) }, 0);
        // SAFETY: fresh descriptors owned by nobody else
        let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        // SAFETY: fcntl on descriptors owned here
        unsafe {
            libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(slave.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
        }
        let output = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let fd = master.as_raw_fd();
        let reader = {
            let output = output.clone();
            let stop = stop.clone();
            std::thread::spawn(move || read_until_stopped(fd, &output, &stop))
        };
        Pty { master: Some(master), slave: Some(slave), output, stop, reader: Some(reader) }
    }

    /// Starts `command` with this terminal as its controlling terminal.
    pub fn spawn(&mut self, command: &mut Command) -> Child {
        let slave = self.slave.as_ref().unwrap();
        let stdio = || Stdio::from(slave.try_clone().unwrap());
        command.stdin(stdio()).stdout(stdio()).stderr(stdio());
        // SAFETY: only async-signal-safe calls between fork and exec
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY, 0);
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        // the program has its own copies; the terminal hangs up when they are closed
        self.slave = None;
        child
    }

    pub fn type_text(&self, text: &str) {
        let fd = self.master.as_ref().unwrap().as_raw_fd();
        let bytes = text.as_bytes();
        // SAFETY: writes from a slice of this length
        let wrote = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(wrote, bytes.len() as isize);
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }

    pub fn wait_for_text(&self, needle: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while !self.text().contains(needle) {
            if Instant::now() > deadline {
                panic!("timed out waiting for {needle:?}; the terminal shows:\n{:?}", self.text());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Closes the terminal: its program gets a hangup.
    pub fn close(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.master = None;
        self.slave = None;
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.close();
    }
}

fn read_until_stopped(fd: RawFd, output: &Mutex<Vec<u8>>, stop: &AtomicBool) {
    let mut buf = [0u8; 8192];
    while !stop.load(Ordering::SeqCst) {
        let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        // SAFETY: one pollfd
        if unsafe { libc::poll(&mut poll, 1, 50) } <= 0 {
            continue;
        }
        // SAFETY: reads into a buffer of this length
        let got = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if got > 0 {
            output.lock().unwrap().extend_from_slice(&buf[..got as usize]);
        } else {
            // nobody has the program side open right now: wait for someone to
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The pid of whoever listens on a unix socket.
pub fn listener_pid(socket: &Path) -> Option<i32> {
    let stream = std::os::unix::net::UnixStream::connect(socket).ok()?;
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
    (got == 0 && cred.pid > 0).then_some(cred.pid)
}

pub fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks
    unsafe { libc::kill(pid, 0) == 0 }
}
