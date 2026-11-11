//! Pairing, end to end: `lumen pair` against an sshd of the test's own,
//! running as the current user on a free port, and the `ssh` program
//! playing the phone. Skipped where there is no sshd or ssh.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use common::{command, lumen, scratch, wait_exit};
use lumen_link::{EnrollReply, EnrollRequest, Offer};

const LONG: Duration = Duration::from_secs(20);

fn find(program: &str, places: &[&str]) -> Option<PathBuf> {
    places.iter().map(PathBuf::from).find(|p| p.exists()).or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|d| d.join(program)).find(|p| p.exists())
    })
}

struct Tools {
    sshd: PathBuf,
    ssh: PathBuf,
    keygen: PathBuf,
}

fn tools() -> Option<Tools> {
    let sshd = find("sshd", &["/usr/sbin/sshd", "/usr/bin/sshd"]);
    let ssh = find("ssh", &[]);
    let keygen = find("ssh-keygen", &[]);
    match (sshd, ssh, keygen) {
        (Some(sshd), Some(ssh), Some(keygen)) => Some(Tools { sshd, ssh, keygen }),
        _ => {
            println!("skipped: this test needs sshd, ssh and ssh-keygen");
            None
        },
    }
}

/// An sshd that only this test logs in to.
struct Sshd {
    child: Child,
    port: u16,
}

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn start_sshd(tools: &Tools, dir: &Path) -> Option<Sshd> {
    let host_key = dir.join("hostkey");
    let made =
        Command::new(&tools.keygen).args(["-q", "-t", "ed25519", "-N", "", "-f"]).arg(&host_key).status().unwrap();
    assert!(made.success());
    std::fs::write(dir.join("authorized_keys"), "").unwrap();
    let port = free_port();
    let option = |text: String| ["-o".to_string(), text];
    let mut command = Command::new(&tools.sshd);
    command.args(["-D", "-e", "-f", "/dev/null", "-p", &port.to_string()]);
    for text in [
        "ListenAddress=127.0.0.1".to_string(),
        format!("HostKey={}", host_key.display()),
        format!("AuthorizedKeysFile={}", dir.join("authorized_keys").display()),
        "PidFile=none".into(),
        "UsePAM=no".into(),
        "StrictModes=no".into(),
        "PasswordAuthentication=no".into(),
        "KbdInteractiveAuthentication=no".into(),
        // the logins' lumen uses the test's runtime folder, as `lumen pair` does
        format!("SetEnv=XDG_RUNTIME_DIR={} LUMEN_SESSION_SHELL=/bin/sh", dir.join("run").display()),
    ] {
        command.args(option(text));
    }
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            use std::io::Read;
            let mut banner = [0u8; 4];
            if stream.read_exact(&mut banner).is_ok() && &banner == b"SSH-" {
                let sshd = Sshd { child, port };
                return logins_work(tools, dir, port).then_some(sshd);
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            let mut err = String::new();
            use std::io::Read;
            let _ = child.stderr.take().unwrap().read_to_string(&mut err);
            println!("skipped: sshd doesn't run as this user ({status}): {err}");
            return None;
        }
        assert!(Instant::now() < deadline, "sshd didn't start");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether this user can log in to the test's sshd at all: a CI machine's
/// account may be locked for logins without PAM, which is no fault of lumen's.
fn logins_work(tools: &Tools, dir: &Path, port: u16) -> bool {
    let probe = dir.join("probe");
    let made = Command::new(&tools.keygen).args(["-q", "-t", "ed25519", "-N", "", "-f"]).arg(&probe).status();
    assert!(made.unwrap().success());
    let public = std::fs::read_to_string(dir.join("probe.pub")).unwrap();
    std::fs::write(dir.join("authorized_keys"), &public).unwrap();
    let output = ssh(tools, dir, port, &probe, "true", "");
    std::fs::write(dir.join("authorized_keys"), "").unwrap();
    if !output.status.success() {
        println!("skipped: can't log in to a test sshd as this user: {}", String::from_utf8_lossy(&output.stderr));
    }
    output.status.success()
}

fn pair_command(dir: &Path, port: u16) -> Command {
    let mut command = command(dir);
    command
        .args(["pair", "--yes", "--port", &port.to_string(), "--address", "127.0.0.1", "--label", "test-pc"])
        .arg("--host-key")
        .arg(dir.join("hostkey.pub"))
        .arg("--authorized-keys")
        .arg(dir.join("authorized_keys"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    command
}

/// Starts `lumen pair` and reads the link it prints.
fn start_pair(dir: &Path, port: u16) -> (Child, BufReader<ChildStdout>, Offer) {
    let mut child = pair_command(dir, port).spawn().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut link = String::new();
    stdout.read_line(&mut link).unwrap();
    let offer = Offer::from_link(&link).unwrap_or_else(|e| panic!("{e}: {link:?}"));
    (child, stdout, offer)
}

/// The short-lived key as a file `ssh` reads: Ed25519 in PKCS#8.
fn write_pairing_key(offer: &Offer, path: &Path) {
    let mut der = vec![0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
    der.extend_from_slice(&offer.seed);
    let pem = format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", STANDARD.encode(der));
    std::fs::write(path, pem).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

/// Runs `ssh` as the phone does, with this key and this command.
fn ssh(tools: &Tools, dir: &Path, port: u16, key: &Path, remote: &str, stdin: &str) -> std::process::Output {
    let mut child = Command::new(&tools.ssh)
        .args(["-F", "/dev/null", "-p", &port.to_string()])
        .arg("-i")
        .arg(key)
        .args(["-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes", "-o", "LogLevel=ERROR"])
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", dir.join("known_hosts").display()))
        .args(["-o", "StrictHostKeyChecking=accept-new", "127.0.0.1", remote])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn lines_of(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("authorized_keys")).unwrap()
}

#[test]
fn a_phone_pairs_logs_in_and_is_unpaired() {
    let Some(tools) = tools() else { return };
    let dir = scratch("pair");
    let Some(sshd) = start_sshd(&tools, &dir) else { return };
    let port = sshd.port;
    // someone's own line, which must stay as it is
    std::fs::write(
        dir.join("authorized_keys"),
        "# mine\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs me",
    )
    .unwrap();

    let (mut pair, _stdout, offer) = start_pair(&dir, port);
    assert_eq!(offer.port, port);
    assert_eq!(offer.label, "test-pc");
    assert_eq!(offer.addresses[0], "127.0.0.1");
    let host_key = std::fs::read_to_string(dir.join("hostkey.pub")).unwrap();
    assert_eq!(offer.host_keys, vec![lumen_link::fingerprint(&host_key).unwrap()]);
    let text = lines_of(&dir);
    assert!(
        text.starts_with(
            "# mine\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs me\n"
        )
    );
    assert!(text.contains(&format!("lumen-pairing:{}", offer.token())), "{text}");

    // the phone: its own key, and the short-lived one from the code
    let pairing_key = dir.join("pairing");
    write_pairing_key(&offer, &pairing_key);
    let phone_key = dir.join("phone");
    let made =
        Command::new(&tools.keygen).args(["-q", "-t", "ecdsa", "-b", "256", "-N", "", "-f"]).arg(&phone_key).status();
    assert!(made.unwrap().success());
    let phone_public = std::fs::read_to_string(dir.join("phone.pub")).unwrap();
    let public_key = phone_public.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    let request = EnrollRequest { device: "Test Phone".into(), public_key: public_key.clone(), ..Default::default() };
    let line = format!("{}\n", serde_json::to_string(&request).unwrap());

    let output = ssh(&tools, &dir, port, &pairing_key, "anything at all", &line);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    let reply: EnrollReply = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("{e}: {stdout:?}"));
    let expected = lumen().canonicalize().unwrap();
    assert!(reply.ok, "{reply:?}");
    assert_eq!(reply.lumen, expected.to_string_lossy());
    assert_eq!(reply.version, env!("CARGO_PKG_VERSION"));
    assert!(wait_exit(&mut pair, LONG).success());

    let text = lines_of(&dir);
    assert!(
        text.starts_with(
            "# mine\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs me\n"
        )
    );
    assert!(text.contains(&format!("restrict,pty {public_key} lumen-phone:Test Phone\n")), "{text}");
    assert!(!text.contains("lumen-pairing:"), "{text}");
    assert!(!dir.join("run").join("lumen").join(format!("pair-{}.sock", offer.token())).exists());

    // the phone logs in with its own key from now on
    let remote = format!("{} sessions --json", reply.lumen);
    let output = ssh(&tools, &dir, port, &phone_key, &remote, "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
    // and the short-lived key is no good any more
    let output = ssh(&tools, &dir, port, &pairing_key, "anything", &line);
    assert!(!output.status.success(), "{output:?}");

    // a terminal the PC keeps, reached over SSH as the phone does; it outlives the connection
    attach_over_ssh(&tools, &dir, port, &phone_key, &reply.lumen);

    let keys = dir.join("authorized_keys");
    let phones = command(&dir).arg("phones").arg("--authorized-keys").arg(&keys).output().unwrap();
    let listed = String::from_utf8_lossy(&phones.stdout);
    assert!(listed.starts_with("Test Phone  SHA256:"), "{listed}");
    let unknown = command(&dir).args(["unpair", "Nobody", "--authorized-keys"]).arg(&keys).output().unwrap();
    assert_eq!(unknown.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("\"Test Phone\""));
    let unpaired = command(&dir).args(["unpair", "Test Phone", "--authorized-keys"]).arg(&keys).output().unwrap();
    assert!(unpaired.status.success(), "{unpaired:?}");
    assert!(String::from_utf8_lossy(&unpaired.stdout).starts_with("Test Phone can no longer log in."));
    let output = ssh(&tools, &dir, port, &phone_key, &remote, "");
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(
        lines_of(&dir),
        "# mine\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs me\n"
    );

    drop(sshd);
    let _ = std::fs::remove_dir_all(&dir);
}

fn attach_over_ssh(tools: &Tools, dir: &Path, port: u16, key: &Path, lumen: &str) {
    use std::io::Read;
    use std::sync::{Arc, Mutex};
    let mut ssh = Command::new(&tools.ssh)
        .args(["-tt", "-F", "/dev/null", "-p", &port.to_string()])
        .arg("-i")
        .arg(key)
        .args(["-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes", "-o", "LogLevel=ERROR"])
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", dir.join("known_hosts").display()))
        .arg("127.0.0.1")
        .arg(format!("{lumen} attach --session phone-1 --env TERM_PROGRAM=lumen"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut stdout = ssh.stdout.take().unwrap();
    let reader = {
        let seen = seen.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 {
                    break;
                }
                seen.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        })
    };
    let mut stdin = ssh.stdin.take().unwrap();
    stdin.write_all(b"echo marker-$((6*7)) $TERM_PROGRAM\r").unwrap();
    common::wait_for("the marker over SSH", LONG, || {
        String::from_utf8_lossy(&seen.lock().unwrap()).contains("marker-42 lumen")
    });
    // the connection is lost: the terminal stays on the PC
    let _ = ssh.kill();
    let _ = ssh.wait();
    drop(stdin);
    let _ = reader.join();
    let listed = || {
        let output = command(dir).args(["sessions", "--json"]).output().unwrap();
        serde_json::from_slice::<Vec<lumen_link::SessionInfo>>(&output.stdout).unwrap()
    };
    common::wait_for("the session to be detached", LONG, || {
        listed().iter().any(|s| s.name == "phone-1" && s.attached == 0)
    });
    let socket = dir.join("run").join("lumen").join("keeper.sock");
    let keeper = common::listener_pid(&socket).expect("a keeper listens");
    let output = command(dir).args(["sessions", "kill", "--all"]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    common::wait_for("the keeper to end", LONG, || !common::alive(keeper) || zombie(keeper));
}

fn zombie(pid: i32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|stat| stat.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z'))
        .unwrap_or(true)
}

#[test]
fn ctrl_c_leaves_no_pairing_line() {
    let Some(tools) = tools() else { return };
    let dir = scratch("pairint");
    let Some(sshd) = start_sshd(&tools, &dir) else { return };
    let (mut pair, _stdout, offer) = start_pair(&dir, sshd.port);
    assert!(lines_of(&dir).contains(&format!("lumen-pairing:{}", offer.token())));
    let socket = dir.join("run").join("lumen").join(format!("pair-{}.sock", offer.token()));
    assert!(socket.exists());
    // SAFETY: kill only sends a signal
    unsafe { libc::kill(pair.id() as i32, libc::SIGINT) };
    assert_eq!(wait_exit(&mut pair, LONG).code(), Some(130));
    assert_eq!(lines_of(&dir), "");
    assert!(!socket.exists());

    // and a pair-enroll that comes too late says so
    let late = command(&dir)
        .args(["pair-enroll", &offer.token()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut late = late;
    late.stdin.take().unwrap().write_all(b"{}\n").unwrap();
    let output = late.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let reply: EnrollReply = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!reply.ok);
    assert!(reply.error.contains("no longer waiting"));
    drop(sshd);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_request_keeps_pair_waiting() {
    let Some(tools) = tools() else { return };
    let dir = scratch("pairno");
    let Some(sshd) = start_sshd(&tools, &dir) else { return };
    // without --yes and without a terminal to ask on, a request is refused
    let command = pair_command(&dir, sshd.port);
    let args: Vec<_> = command.get_args().filter(|a| *a != "--yes").map(|a| a.to_owned()).collect();
    let mut asking = common::command(&dir);
    asking.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut pair = asking.spawn().unwrap();
    let mut stdout = BufReader::new(pair.stdout.take().unwrap());
    let mut link = String::new();
    stdout.read_line(&mut link).unwrap();
    let offer = Offer::from_link(&link).unwrap();

    let send = |request: &str| {
        let mut enroll = common::command(&dir)
            .args(["pair-enroll", &offer.token()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        enroll.stdin.take().unwrap().write_all(format!("{request}\n").as_bytes()).unwrap();
        let output = enroll.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        serde_json::from_slice::<EnrollReply>(&output.stdout).unwrap()
    };
    // a key with options smuggled in
    let smuggled = r#"{"device":"x","public_key":"command=\"sh\" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs"}"#;
    let reply = send(smuggled);
    assert!(!reply.ok && !reply.error.is_empty(), "{reply:?}");
    let good = r#"{"device":"x","public_key":"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs"}"#;
    let reply = send(good);
    assert!(!reply.ok, "{reply:?}");
    // still waiting, the pairing line still there, no phone added
    assert!(pair.try_wait().unwrap().is_none());
    let text = lines_of(&dir);
    assert!(text.contains("lumen-pairing:") && !text.contains("lumen-phone:"), "{text}");
    // SAFETY: kill only sends a signal
    unsafe { libc::kill(pair.id() as i32, libc::SIGTERM) };
    assert_eq!(wait_exit(&mut pair, LONG).code(), Some(128 + libc::SIGTERM));
    assert_eq!(lines_of(&dir), "");
    drop(sshd);
    let _ = std::fs::remove_dir_all(&dir);
}
