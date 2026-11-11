//! lumen for phones, the PC's side: pairing a phone, and the terminals the
//! PC keeps running while the phone is away.
//!
//! These are subcommands of the `lumen` program that need no window: they
//! run over SSH, with no display, and touch nothing of GPUI or the config.

mod attach;
mod authorized;
mod frame;
mod keeper;
mod pair;
mod replay;
mod sys;

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Runs the subcommand the command line names, if it names one, and says
/// what the program's exit status is to be.
pub fn run(args: &[String]) -> Option<i32> {
    let (command, rest) = args.split_first()?;
    let status = match command.as_str() {
        "pair" => pair::pair(rest),
        "pair-enroll" => pair::enroll(rest),
        "phones" => pair::phones(rest),
        "unpair" => pair::unpair(rest),
        "attach" => attach::attach(rest),
        "sessions" => attach::sessions(rest),
        "keeper" => keeper::keeper(rest),
        _ => return None,
    };
    Some(status)
}

/// The text `lumen --help` shows about these commands.
pub const USAGE: &str = "\
PHONES
    lumen pair [--label NAME] [--address A]... [--port N] [--yes]
                                        pair a phone: shows a code for the lumen app to scan
    lumen phones                        list the paired phones
    lumen unpair <name>                 a paired phone can no longer log in
    lumen attach --session NAME         show a terminal the PC keeps running (made if missing)
    lumen sessions [--json]             list the terminals the PC keeps running
    lumen sessions kill <name>|--all    end them
";

/// A usage error in the way `lumen` reports them, and its exit status.
fn usage_error(message: impl std::fmt::Display) -> i32 {
    eprintln!("lumen: {message}\nTry 'lumen --help'.");
    2
}

/// The folder where lumen's sockets are: `$XDG_RUNTIME_DIR/lumen`.
///
/// Without `XDG_RUNTIME_DIR` (an SSH login without PAM has none), the
/// folder systemd makes for the user is taken if it is there, so that
/// `lumen pair` in a desktop terminal and `lumen pair-enroll` under sshd
/// find each other; else a folder of the user's own in `/tmp`.
fn runtime_dir() -> io::Result<PathBuf> {
    let uid = sys::uid();
    let base = match std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => {
            let systemd = PathBuf::from(format!("/run/user/{uid}"));
            if private_to_user(&systemd, uid) {
                systemd
            } else {
                let tmp = PathBuf::from(format!("/tmp/lumen-{uid}"));
                ensure_private_dir(&tmp, uid)?;
                tmp
            }
        },
    };
    let dir = base.join("lumen");
    ensure_private_dir(&dir, uid)?;
    Ok(dir)
}

fn private_to_user(dir: &Path, uid: u32) -> bool {
    fs::symlink_metadata(dir).map(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0).unwrap_or(false)
}

/// Makes a folder only this user can enter, or checks that the one there
/// is one: someone else's would let them listen in place of lumen.
fn ensure_private_dir(dir: &Path, uid: u32) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {},
        Err(e) => return Err(io::Error::new(e.kind(), format!("can't make {}: {e}", dir.display()))),
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != uid {
        return Err(io::Error::other(format!("{} is not a folder of this user's", dir.display())));
    }
    if meta.mode() & 0o077 != 0 {
        // ours, but open to others: lumen's own folder is closed, a base it didn't make is refused
        if dir.file_name().is_some_and(|n| n == "lumen") {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        } else {
            return Err(io::Error::other(format!("{} can be entered by other users", dir.display())));
        }
    }
    Ok(())
}

/// The absolute path of this lumen, for commands that run it again.
fn this_lumen() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(fs::canonicalize(&exe).unwrap_or(exe))
}
