//! The few system calls the phone side needs that the standard library
//! doesn't offer: the user's passwd entry, terminals, local time, network
//! interfaces, signals.

use std::ffi::CStr;
use std::io;
use std::mem::MaybeUninit;
use std::net::Ipv4Addr;
use std::os::fd::RawFd;
use std::path::PathBuf;
use std::sync::Mutex;

/// Who runs this lumen, from the system's user database rather than from
/// the environment, which an SSH login or a script may have changed.
pub struct User {
    pub name: String,
    pub home: PathBuf,
    pub shell: String,
}

pub fn user() -> io::Result<User> {
    let uid = uid();
    let mut buf = vec![0u8; 4096];
    loop {
        let mut entry = MaybeUninit::<libc::passwd>::uninit();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: the buffer and the entry outlive the call, and its length is the buffer's
        let status =
            unsafe { libc::getpwuid_r(uid, entry.as_mut_ptr(), buf.as_mut_ptr().cast(), buf.len(), &mut found) };
        if status == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 4, 0);
            continue;
        }
        if status != 0 || found.is_null() {
            return Err(io::Error::other(format!("there is no user with the id {uid}")));
        }
        // SAFETY: getpwuid_r filled the entry, and its strings point into the buffer
        let entry = unsafe { entry.assume_init() };
        let text = |ptr: *const libc::c_char| {
            if ptr.is_null() {
                String::new()
            } else {
                // SAFETY: a non-null field of the entry is a C string inside the buffer
                unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
            }
        };
        let shell = text(entry.pw_shell);
        return Ok(User {
            name: text(entry.pw_name),
            home: PathBuf::from(text(entry.pw_dir)),
            shell: if shell.is_empty() { "/bin/sh".into() } else { shell },
        });
    }
}

pub fn uid() -> u32 {
    // SAFETY: getuid can't fail
    unsafe { libc::getuid() }
}

pub fn is_terminal(fd: RawFd) -> bool {
    // SAFETY: isatty only looks at the number
    unsafe { libc::isatty(fd) == 1 }
}

/// A terminal's size: cells, and the whole text area in pixels (0 when the
/// terminal doesn't say).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Winsize {
    pub cols: u16,
    pub rows: u16,
    pub width: u16,
    pub height: u16,
}

pub fn winsize(fd: RawFd) -> Option<Winsize> {
    let mut ws = MaybeUninit::<libc::winsize>::zeroed();
    // SAFETY: TIOCGWINSZ writes one winsize into the pointer
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, ws.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: the ioctl succeeded, and zeroes were a valid start anyway
    let ws = unsafe { ws.assume_init() };
    Some(Winsize { cols: ws.ws_col, rows: ws.ws_row, width: ws.ws_xpixel, height: ws.ws_ypixel })
}

pub fn set_winsize(fd: RawFd, size: Winsize) -> io::Result<()> {
    let ws = libc::winsize { ws_col: size.cols, ws_row: size.rows, ws_xpixel: size.width, ws_ypixel: size.height };
    // SAFETY: TIOCSWINSZ reads one winsize from the pointer
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// How the terminal on standard input was before raw mode, to put back on
/// every way out: a normal end, a signal, a panic.
static SAVED_TERMIOS: Mutex<Option<libc::termios>> = Mutex::new(None);

/// Puts the terminal on standard input into raw mode: every key goes
/// through as it is, nothing is echoed.
pub fn raw_mode() -> io::Result<()> {
    let mut termios = MaybeUninit::<libc::termios>::uninit();
    // SAFETY: tcgetattr fills the struct on success
    if unsafe { libc::tcgetattr(0, termios.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: tcgetattr succeeded
    let saved = unsafe { termios.assume_init() };
    let mut raw = saved;
    // SAFETY: cfmakeraw changes the struct it is given and nothing else
    unsafe { libc::cfmakeraw(&mut raw) };
    *SAVED_TERMIOS.lock().unwrap_or_else(|e| e.into_inner()) = Some(saved);
    // SAFETY: a valid termios for the terminal it came from
    if unsafe { libc::tcsetattr(0, libc::TCSANOW, &raw) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Throws away what was typed on a terminal and not read yet.
pub fn forget_typed(fd: i32) {
    // SAFETY: tcflush drops pending input of a descriptor and touches nothing else
    unsafe { libc::tcflush(fd, libc::TCIFLUSH) };
}

/// Undoes [`raw_mode`], if it was done. Safe to call more than once.
pub fn restore_terminal() {
    let saved = SAVED_TERMIOS.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(saved) = saved {
        // SAFETY: the termios tcgetattr gave for this terminal
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
    }
}

/// A time as `YYYYMMDDHHMMSS` in the system's local time: what sshd's
/// `expiry-time` option takes.
pub fn local_stamp(unix_seconds: i64) -> String {
    let time = unix_seconds as libc::time_t;
    let mut tm = MaybeUninit::<libc::tm>::zeroed();
    // SAFETY: localtime_r writes one tm, and zeroes are a valid start
    let tm = unsafe {
        libc::localtime_r(&time, tm.as_mut_ptr());
        tm.assume_init()
    };
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// One network interface with an IPv4 address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub up: bool,
    pub loopback: bool,
    pub address: Ipv4Addr,
}

pub fn interfaces() -> Vec<Interface> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs allocates the list, which is freed below
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut next = list;
    while !next.is_null() {
        // SAFETY: a node of the list getifaddrs made
        let entry = unsafe { &*next };
        next = entry.ifa_next;
        if entry.ifa_addr.is_null() || entry.ifa_name.is_null() {
            continue;
        }
        // SAFETY: ifa_addr points to a sockaddr whose family says what it is
        if i32::from(unsafe { (*entry.ifa_addr).sa_family }) != libc::AF_INET {
            continue;
        }
        // SAFETY: an AF_INET address is a sockaddr_in
        let addr = unsafe { &*(entry.ifa_addr as *const libc::sockaddr_in) };
        let flags = entry.ifa_flags as i32;
        found.push(Interface {
            // SAFETY: the name is a C string inside the list
            name: unsafe { CStr::from_ptr(entry.ifa_name) }.to_string_lossy().into_owned(),
            up: flags & libc::IFF_UP != 0,
            loopback: flags & libc::IFF_LOOPBACK != 0,
            address: Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr)),
        });
    }
    // SAFETY: the list getifaddrs made, not used after this
    unsafe { libc::freeifaddrs(list) };
    found
}

/// Blocks these signals in this thread and every thread it starts from now
/// on, so that [`wait_signal`] can take them one by one, in a thread of its
/// own, where anything may be done about them.
pub fn block_signals(signals: &[i32]) {
    let set = signal_set(signals);
    // SAFETY: a valid, filled set
    unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) };
}

/// Waits for one of these (blocked) signals and says which.
pub fn wait_signal(signals: &[i32]) -> i32 {
    let set = signal_set(signals);
    let mut signal = 0;
    loop {
        // SAFETY: a valid set and somewhere to put the answer
        if unsafe { libc::sigwait(&set, &mut signal) } == 0 {
            return signal;
        }
    }
}

fn signal_set(signals: &[i32]) -> libc::sigset_t {
    let mut set = MaybeUninit::<libc::sigset_t>::uninit();
    // SAFETY: sigemptyset makes the set valid, sigaddset adds to it
    unsafe {
        libc::sigemptyset(set.as_mut_ptr());
        for &signal in signals {
            libc::sigaddset(set.as_mut_ptr(), signal);
        }
        set.assume_init()
    }
}

/// Whether a program of this name is on the `PATH`.
pub fn on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|candidate| {
        use std::os::unix::fs::PermissionsExt;
        candidate.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamp_has_fourteen_digits() {
        let stamp = local_stamp(1_700_000_000);
        assert_eq!(stamp.len(), 14);
        assert!(stamp.bytes().all(|b| b.is_ascii_digit()), "{stamp}");
        // later is larger, so that stamps compare as text
        assert!(local_stamp(1_700_000_600) > stamp);
        assert!(stamp.starts_with("2023111"), "{stamp}");
    }
}
