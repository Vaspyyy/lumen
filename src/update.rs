//! `lumen update`: lumen and the tools made for it, at their newest releases.
//!
//! lumen comes from its release on GitHub: the archive is fetched, checked
//! against the release's SHA256SUMS, and installed by the `install.sh` in it,
//! into the folder the running program is in. A lumen built from source is
//! replaced by the release once a newer one is out; one from a package (in a
//! folder only root can write to) is left to the package manager, and its
//! owner is told once per version.
//!
//! The tools (glint, crux, recap, seek, pulse) are installed with `go
//! install`, and updated the same way, each into the folder it is in. Only
//! the ones that are installed are updated, and only where Go is.
//!
//! Nothing is ever replaced by an older version. `lumen-update.timer` runs
//! this once a day, with `--notify`, at the lowest priority.

use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

pub const USAGE: &str = "\
UPDATES
    lumen update                        bring lumen and its tools (glint, crux, recap, seek,
                                        pulse) to their newest releases; a systemd timer
                                        does it once a day
    lumen update --check                only say what is out
";

const GITHUB: &str = "https://github.com/Vaspyyy";
const API: &str = "https://api.github.com/repos/Vaspyyy";
const DOCS: &str = "https://vaspyyy.github.io/lumen/docs/install/#updates";
const TOOLS: &[&str] = &["glint", "crux", "recap", "seek", "pulse"];
/// The archive of a release, for the one machine releases are built for.
const ARCHIVE: &str = "lumen-x86_64-linux.tar.gz";

/// Runs the command if `args` is one, and gives its exit status.
pub fn run(args: &[String]) -> Option<i32> {
    let (command, rest) = args.split_first()?;
    if command != "update" {
        return None;
    }
    let mut check = false;
    let mut notify = false;
    for arg in rest {
        match arg.as_str() {
            "--check" => check = true,
            // the timer's: says on the desktop what changed
            "--notify" => notify = true,
            other => {
                eprintln!("lumen: update takes --check, not {other}\nTry 'lumen --help'.");
                return Some(2);
            },
        }
    }
    Some(update(check, notify))
}

/// A version as releases are numbered: 0.11.2, or v0.11.2 as a tag.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version(Vec<u64>);

impl Version {
    fn parse(text: &str) -> Option<Version> {
        let text = text.trim().trim_start_matches('v');
        // a pre-release or a build after its number is not a newer version
        let numbers = text.split(['-', '+']).next()?;
        let parts: Option<Vec<u64>> = numbers.split('.').map(|part| part.parse().ok()).collect();
        let mut parts = parts.filter(|parts| !parts.is_empty() && parts.len() <= 3)?;
        parts.resize(3, 0);
        Some(Version(parts))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let parts: Vec<String> = self.0.iter().map(u64::to_string).collect();
        f.write_str(&parts.join("."))
    }
}

/// What one update did, for the summary and the notification.
#[derive(Default)]
struct Report {
    /// "glint 0.11.1 → 0.11.2"
    updated: Vec<String>,
    /// What is out but couldn't be put in place here, with what to do.
    waiting: Vec<String>,
    failed: bool,
}

fn update(check: bool, notify: bool) -> i32 {
    // two at once (the timer, and one by hand) would write over each other
    let _lock = match lock() {
        Ok(lock) => lock,
        Err(why) => {
            eprintln!("lumen: {why}");
            return 1;
        },
    };
    let mut report = Report::default();
    update_lumen(check, &mut report);
    for tool in TOOLS {
        update_tool(tool, check, &mut report);
    }
    if notify {
        tell(&report);
    }
    if report.failed { 1 } else { 0 }
}

/// The lock that keeps a second update from starting, held until it is dropped.
fn lock() -> Result<File, String> {
    let dir = crate::phone::runtime_dir().map_err(|e| format!("no runtime folder for the update's lock: {e}"))?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("update.lock"))
        .map_err(|e| format!("can't open the update's lock: {e}"))?;
    // SAFETY: flock on a descriptor this function owns
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("an update is running already".into());
    }
    Ok(file)
}

/// Fetches a URL over https with curl, or wget where there is no curl.
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let curl = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-fsSL", "--max-time", "300", url])
        .stderr(Stdio::piped())
        .output();
    let output = match curl {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Command::new("wget")
            .args(["-q", "--https-only", "--timeout=300", "-O", "-", url])
            .stderr(Stdio::piped())
            .output()
            .map_err(|_| "curl or wget is needed to fetch updates".to_string())?,
        Err(e) => return Err(format!("can't run curl: {e}")),
    };
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(format!("could not fetch {url}: {}", said.trim()));
    }
    Ok(output.stdout)
}

/// The newest release of one of the repositories: its tag and its version.
fn newest(repository: &str) -> Result<(String, Version), String> {
    let body = fetch(&format!("{API}/{repository}/releases/latest"))?;
    let release: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("GitHub's answer about {repository} is not JSON: {e}"))?;
    let tag = release.get("tag_name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let version =
        Version::parse(&tag).ok_or_else(|| format!("{repository}'s newest release has no version: {tag:?}"))?;
    Ok((tag, version))
}

fn update_lumen(check: bool, report: &mut Report) {
    let ours = Version::parse(env!("CARGO_PKG_VERSION")).expect("lumen's own version");
    let (tag, newest) = match newest("lumen") {
        Ok(found) => found,
        Err(why) => {
            eprintln!("lumen: {why}");
            report.failed = true;
            return;
        },
    };
    if newest <= ours {
        println!("lumen {ours} is the newest");
        return;
    }
    let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) else {
        eprintln!("lumen: can't tell where this lumen is");
        report.failed = true;
        return;
    };
    if !writable(&dir) {
        // a package: apt, dnf, zypper or pacman put it where only root writes
        println!("lumen {newest} is out (this is {ours}); your package manager updates it: {DOCS}");
        if told_once(&newest) {
            report.waiting.push(format!("lumen {newest} is out: update it with your package manager"));
        }
        return;
    }
    if std::env::consts::ARCH != "x86_64" {
        println!("lumen {newest} is out (this is {ours}), but not built for {}: {DOCS}", std::env::consts::ARCH);
        return;
    }
    if check {
        println!("lumen {ours} → {newest}");
        return;
    }
    match install_lumen(&tag, &dir) {
        Ok(()) => {
            println!("lumen {ours} → {newest}");
            report.updated.push(format!("lumen {ours} → {newest}"));
        },
        Err(why) => {
            eprintln!("lumen: lumen {newest} is out, and could not be installed: {why}");
            report.failed = true;
        },
    }
}

/// Whether this user may write to a folder.
fn writable(dir: &Path) -> bool {
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return false };
    // SAFETY: access with a string that ends with its NUL
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

/// Fetches the release `tag`, checks it, and has its own install.sh put it into `dir`.
fn install_lumen(tag: &str, dir: &Path) -> Result<(), String> {
    let base = format!("{GITHUB}/lumen/releases/download/{tag}");
    let sums = String::from_utf8(fetch(&format!("{base}/SHA256SUMS"))?).map_err(|_| "SHA256SUMS is not text")?;
    let want = checksum_of(&sums, ARCHIVE).ok_or_else(|| format!("the release's SHA256SUMS doesn't list {ARCHIVE}"))?;
    let archive = fetch(&format!("{base}/{ARCHIVE}"))?;
    let have = hex(&Sha256::digest(&archive));
    if have != want {
        return Err(format!("what was fetched is damaged: its checksum is {have}, the release says {want}"));
    }

    // unpacked next to the program, as install.sh does: /tmp may not let it start
    let work = dir.join(format!(".lumen-update.{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir(&work).map_err(|e| format!("can't make {}: {e}", work.display()))?;
    let result = (|| {
        std::fs::write(work.join(ARCHIVE), &archive).map_err(|e| format!("can't write the archive: {e}"))?;
        let unpacked = Command::new("tar")
            .arg("-xzf")
            .arg(work.join(ARCHIVE))
            .arg("-C")
            .arg(&work)
            .status()
            .map_err(|e| format!("can't run tar: {e}"))?;
        if !unpacked.success() {
            return Err("tar could not unpack the archive".to_string());
        }
        let folder = std::fs::read_dir(&work)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.is_dir() && path.join("install.sh").is_file())
            .ok_or("the archive doesn't hold install.sh")?;
        let mut install = Command::new("sh");
        install.arg(folder.join("install.sh")).env("LUMEN_BIN_DIR", dir);
        // what was left out or turned off stays so: only services that are there are kept up
        let units = units_dir();
        if !units.join("lumen.service").exists() {
            install.env("LUMEN_DAEMON", "0");
        }
        if !units.join("lumen-update.timer").exists() {
            install.env("LUMEN_AUTO_UPDATE", "0");
        }
        let output = install.output().map_err(|e| format!("can't run install.sh: {e}"))?;
        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr);
            return Err(format!("install.sh failed: {}", said.trim()));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Where systemd looks for a user's own services.
fn units_dir() -> PathBuf {
    match std::env::var_os("XDG_CONFIG_HOME").filter(|dir| Path::new(dir).is_absolute()) {
        Some(dir) => PathBuf::from(dir).join("systemd/user"),
        None => dirs::home_dir().unwrap_or_default().join(".config/systemd/user"),
    }
}

/// The checksum SHA256SUMS gives for a file.
fn checksum_of(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (sum, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim_start();
        (name == file || name.strip_prefix('*') == Some(file)).then(|| sum.to_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Whether the owner of a packaged lumen is yet to hear of this version: true once.
fn told_once(version: &Version) -> bool {
    let Some(dir) = dirs::state_dir().map(|dir| dir.join("lumen")) else { return true };
    let file = dir.join("update-told");
    if std::fs::read_to_string(&file).is_ok_and(|told| told.trim() == version.to_string()) {
        return false;
    }
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&file, version.to_string());
    true
}

fn update_tool(tool: &str, check: bool, report: &mut Report) {
    // a tool that isn't installed stays so
    let Some(program) = find(tool, &tool_folders()) else { return };
    let Some(ours) = version_of(&program) else {
        println!("{tool}: {} doesn't say its version, so it stays as it is", program.display());
        return;
    };
    let (tag, newest) = match newest(tool) {
        Ok(found) => found,
        Err(why) => {
            eprintln!("lumen: {why}");
            report.failed = true;
            return;
        },
    };
    if newest <= ours {
        println!("{tool} {ours} is the newest");
        return;
    }
    if check {
        println!("{tool} {ours} → {newest}");
        return;
    }
    let Some(go) = find("go", &go_folders()) else {
        println!("{tool} {newest} is out (this is {ours}); updating it needs Go, which isn't installed");
        report.waiting.push(format!("{tool} {newest} is out: updating it needs Go"));
        return;
    };
    let Some(folder) = program.parent() else { return };
    let installed = Command::new(go)
        .args(["install", &format!("github.com/Vaspyyy/{tool}@{tag}")])
        // into the folder the tool is in, not wherever Go puts programs by default
        .env("GOBIN", folder)
        .stdout(Stdio::null())
        .output();
    match installed {
        Ok(output) if output.status.success() && version_of(&program).is_some_and(|now| now >= newest) => {
            println!("{tool} {ours} → {newest}");
            report.updated.push(format!("{tool} {ours} → {newest}"));
        },
        Ok(output) => {
            let said = String::from_utf8_lossy(&output.stderr);
            eprintln!("lumen: {tool} {newest} is out, and go install failed: {}", said.trim());
            report.failed = true;
        },
        Err(e) => {
            eprintln!("lumen: can't run go: {e}");
            report.failed = true;
        },
    }
}

/// The version a tool says it is, in `tool --version`: "glint 0.11.2".
fn version_of(program: &Path) -> Option<Version> {
    let output = Command::new(program).arg("--version").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace().find_map(Version::parse)
}

/// The folders a program is looked for in: `PATH`, then the ones named after it. The timer
/// runs with systemd's `PATH`, which has no folder of the user's own.
fn find(program: &str, extra: &[PathBuf]) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .chain(extra.iter().cloned())
        .map(|dir| dir.join(program))
        .find(|file| file.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0))
}

/// Where `go install` puts programs: `GOBIN`, `GOPATH/bin`, `~/go/bin`. And
/// `~/.local/bin`, where a tool may have been copied.
fn tool_folders() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut folders = Vec::new();
    if let Some(bin) = std::env::var_os("GOBIN") {
        folders.push(PathBuf::from(bin));
    }
    if let Some(gopath) = std::env::var_os("GOPATH") {
        folders.extend(std::env::split_paths(&gopath).map(|dir| dir.join("bin")));
    }
    folders.push(home.join("go/bin"));
    folders.push(home.join(".local/bin"));
    folders.into_iter().filter(|dir| dir.is_absolute()).collect()
}

/// Where Go itself is, off `PATH`: as its own archive unpacks it, or in `~/go`.
fn go_folders() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![PathBuf::from("/usr/local/go/bin"), PathBuf::from("/usr/lib/go/bin"), home.join("go/bin")]
}

/// Says on the desktop what the update did, if it did or found anything worth saying.
fn tell(report: &Report) {
    let mut lines = report.updated.clone();
    lines.extend(report.waiting.iter().cloned());
    if lines.is_empty() {
        return;
    }
    let title = if report.updated.is_empty() { "lumen: updates are out" } else { "lumen and its tools are updated" };
    // the desktop's notifications, where notify-send is there to reach them
    let _ = Command::new("notify-send")
        .args(["--app-name=lumen", "--icon=lumen", title, &lines.join("\n")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_compared_by_their_numbers() {
        let v = |text| Version::parse(text).unwrap();
        assert!(v("v0.11.2") > v("0.11.1"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert_eq!(v("v1.2"), v("1.2.0"));
        assert_eq!(v("0.6.0-rc.1"), v("0.6.0"));
        assert_eq!(Version::parse("(devel)"), None);
        assert_eq!(Version::parse(""), None);
        assert_eq!(v("0.5.2").to_string(), "0.5.2");
    }

    #[test]
    fn a_tool_says_its_version_after_its_name() {
        let text = "glint 0.11.2\n";
        assert_eq!(text.split_whitespace().find_map(Version::parse), Version::parse("0.11.2"));
    }

    #[test]
    fn the_checksum_is_found_by_the_files_name() {
        let sums = "aaaa  lumen-terminal_amd64.deb\nBBBB *lumen-x86_64-linux.tar.gz\n";
        assert_eq!(checksum_of(sums, ARCHIVE).as_deref(), Some("bbbb"));
        assert_eq!(checksum_of(sums, "other"), None);
    }
}
