//! The terminals the PC keeps: `lumen attach`, `lumen sessions` and the
//! keeper, run as they are, each test in a runtime folder of its own.

mod common;

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use common::{Pty, command, listener_pid, scratch, wait_exit, wait_for};
use lumen_link::SessionInfo;

const LONG: Duration = Duration::from_secs(15);

fn sessions(scratch: &Path) -> Vec<SessionInfo> {
    let output = command(scratch).args(["sessions", "--json"]).stdin(Stdio::null()).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1, "{text:?}");
    serde_json::from_str(text.trim()).unwrap()
}

fn attached(scratch: &Path, name: &str) -> Option<u32> {
    sessions(scratch).into_iter().find(|s| s.name == name).map(|s| s.attached)
}

fn socket(scratch: &Path) -> std::path::PathBuf {
    scratch.join("run").join("lumen").join("keeper.sock")
}

/// Whether a process is there and not just waiting to be reaped.
fn running(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z'),
        Err(_) => false,
    }
}

/// Ends a keeper a failed test left behind.
struct KeeperGuard<'a>(&'a Path);

impl Drop for KeeperGuard<'_> {
    fn drop(&mut self) {
        if let Some(pid) = listener_pid(&socket(self.0)) {
            // SAFETY: kill only sends a signal
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
}

#[test]
fn no_keeper_no_sessions_and_none_started() {
    let dir = scratch("none");
    assert!(sessions(&dir).is_empty());
    let output = command(&dir).arg("sessions").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout), "No terminal is running here.\n");
    assert!(!socket(&dir).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn attach_without_a_terminal_is_refused() {
    let dir = scratch("noterm");
    let output = command(&dir).args(["attach", "--session", "x"]).stdin(Stdio::null()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let output = command(&dir).args(["attach", "--session", "-rf"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_session_outlives_its_terminal() {
    let dir = scratch("keep");
    let _guard = KeeperGuard(&dir);

    // the first terminal starts the keeper and the session
    let mut first = Pty::open(80, 24);
    let mut attach = first.spawn(command(&dir).args(["attach", "--session", "work", "--cwd"]).arg(&dir));
    first.wait_for_text("$ ", LONG);
    first.type_text("echo marker-$((6*7))\r");
    first.wait_for_text("marker-42", LONG);
    assert_eq!(attached(&dir, "work"), Some(1));
    let keeper = listener_pid(&socket(&dir)).expect("a keeper listens");
    let info = sessions(&dir).pop().unwrap();
    assert_eq!(Path::new(&info.cwd).canonicalize().unwrap(), dir.canonicalize().unwrap());
    assert!(info.created > 0);

    // the terminal goes away: a detach, the session stays
    first.close();
    assert!(wait_exit(&mut attach, LONG).success());
    wait_for("the client to be gone", LONG, || attached(&dir, "work") == Some(0));
    assert!(running(keeper), "the keeper ended with the attach that started it");

    // another terminal, another size: it gets the whole picture
    let mut second = Pty::open(100, 30);
    let mut attach = second.spawn(command(&dir).args(["attach", "--session", "work"]));
    second.wait_for_text("marker-42", LONG);
    assert_eq!(attached(&dir, "work"), Some(1));
    second.type_text("stty size\r");
    second.wait_for_text("30 100", LONG);

    // the program ends: so do the session, the attach and the keeper
    second.type_text("exit\r");
    assert_eq!(wait_exit(&mut attach, LONG).code(), Some(0));
    wait_for("the keeper to end", LONG, || !running(keeper));
    assert!(!socket(&dir).exists());
    assert!(sessions(&dir).is_empty());
    drop(second);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sessions_kill_ends_a_session() {
    let dir = scratch("kill");
    let _guard = KeeperGuard(&dir);
    let mut pty = Pty::open(80, 24);
    let mut attach = pty.spawn(command(&dir).args(["attach", "--session", "doomed", "--env", "LUMEN_TEST=yes-it-is"]));
    pty.wait_for_text("$ ", LONG);
    pty.type_text("echo $LUMEN_TEST $LUMEN_SESSION $TERM\r");
    pty.wait_for_text("yes-it-is doomed xterm-256color", LONG);
    let keeper = listener_pid(&socket(&dir)).expect("a keeper listens");

    let output = command(&dir).args(["sessions", "kill", "nothing-here"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let output = command(&dir).args(["sessions", "kill", "doomed"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(wait_exit(&mut attach, LONG).code(), Some(0));
    wait_for("the keeper to end", LONG, || !running(keeper));
    assert!(sessions(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_full_screen_program_is_shown_again() {
    let dir = scratch("alt");
    let _guard = KeeperGuard(&dir);
    let mut pty = Pty::open(80, 24);
    let mut attach = pty.spawn(command(&dir).args(["attach", "--session", "full"]));
    pty.wait_for_text("$ ", LONG);
    // a program on the alternate screen that draws itself again on every size change
    let program = dir.join("full.sh");
    std::fs::write(
        &program,
        "trap 'printf \"\\033[H\\033[2Jdrawn at %s\" \"$(stty size)\"' WINCH\n\
         printf '\\033[?1049h\\033[Hfirst draw'\n\
         while :; do sleep 0.1; done\n",
    )
    .unwrap();
    pty.type_text(&format!("sh {}\r", program.display()));
    pty.wait_for_text("first draw", LONG);
    pty.close();
    assert!(wait_exit(&mut attach, LONG).success());
    wait_for("the client to be gone", LONG, || attached(&dir, "full") == Some(0));

    let mut again = Pty::open(90, 20);
    let mut attach = again.spawn(command(&dir).args(["attach", "--session", "full"]));
    // the replay switches to the alternate screen, and the program draws itself for the new size
    again.wait_for_text("\x1b[?1049h", LONG);
    again.wait_for_text("drawn at 20 90", LONG);
    let output = command(&dir).args(["sessions", "kill", "--all"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(wait_exit(&mut attach, LONG).code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
