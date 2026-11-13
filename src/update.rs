//! `lumen update`: lumen and the tools made for it, at their newest releases.
//!
//! lumen comes from its release on GitHub: the archive is fetched, checked
//! against the release's SHA256SUMS, and installed by the `install.sh` in it,
//! into the folder the running program is in. A lumen built from source is
//! replaced by the release once a newer one is out. One from a package (under
//! /usr, which root owns) is left to the package manager, and its owner is
//! told once per version. A build in a cargo target folder is left alone.
//!
//! The tools (glint, crux, recap, seek, pulse) are installed with `go
//! install`, and updated the same way, each into the folder its program file
//! is in. Only the ones that are installed, that Go built from their module,
//! are updated, and only where Go is. Their newest version is the one `go
//! list` names, through the user's own GOPROXY, which is what `go install`
//! takes.
//!
//! Nothing is ever replaced by an older version. `lumen-update.timer` runs
//! this once a day, with `--notify`, at the lowest priority. When only the
//! network failed it ends with [`EX_TEMPFAIL`], and the service tries again;
//! a failure that would come again the same way is noted, said once, and
//! not tried again by the timer for that version.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

pub const USAGE: &str = "\
UPDATES
    lumen update                        bring lumen, the tools made for it (glint, crux,
                                        recap, seek, pulse and any later ones) and their
                                        agent skills to their newest releases; a systemd
                                        timer does it once a day
    lumen update --check                only say what is out
";

const GITHUB: &str = "https://github.com/Vaspyyy";
/// Go's module proxy, for a tool's newest version where there is no Go to ask: the
/// module path's capitals as `!` and the letter.
const PROXY: &str = "https://proxy.golang.org/github.com/!vaspyyy";
const DOCS: &str = "https://vaspyyy.github.io/lumen/docs/install/#updates";
/// The tools made for lumen, as this lumen knows them: the list in lumen's repository
/// ([`TOOL_LIST`]) has the say, so that a new tool is updated without a new lumen.
const TOOLS: &[&str] = &["glint", "crux", "recap", "seek", "pulse", "splice"];
const TOOL_LIST: &str = "https://raw.githubusercontent.com/Vaspyyy/lumen/main/packaging/tools.txt";
/// How the list of tools starts.
const LIST_HEADER: &str = "# The tools made for lumen";
/// The tools' files by version, for their agent skills: `<RAW>/<tool>/<tag>/skill/SKILL.md`.
const RAW: &str = "https://raw.githubusercontent.com/Vaspyyy";
/// The archive of a release, for the one machine releases are built for.
const ARCHIVE: &str = "lumen-x86_64-linux.tar.gz";
/// Where an update unpacks a release, next to the program: one left by an update that
/// was stopped is cleared by the next.
const WORK: &str = ".lumen-update";
/// The exit status when only the network failed: lumen-update.service tries again.
pub const EX_TEMPFAIL: i32 = 75;

/// Runs the command if `args` is one, and gives its exit status.
pub fn run(args: &[String]) -> Option<i32> {
    let (command, rest) = args.split_first()?;
    if command != "update" {
        return None;
    }
    let mut check = false;
    let mut timer = false;
    for arg in rest {
        match arg.as_str() {
            "--check" => check = true,
            // the timer's: says on the desktop what changed, and doesn't try again what
            // failed the same way before
            "--notify" => timer = true,
            other => {
                eprintln!("lumen: update takes --check, not {other}\nTry 'lumen --help'.");
                return Some(2);
            },
        }
    }
    // SAFETY: geteuid can't fail
    if unsafe { libc::geteuid() } == 0 {
        eprintln!(
            "lumen: update runs as the user lumen belongs to, not as root: a package is updated by its package manager"
        );
        return Some(1);
    }
    Some(update(if check {
        How::Check
    } else if timer {
        How::Timer
    } else {
        How::ByHand
    }))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum How {
    /// Only says what is out.
    Check,
    /// `lumen update`: tries everything, also what failed before.
    ByHand,
    /// The timer: tells on the desktop, and leaves alone what failed before.
    Timer,
}

/// A version as releases are numbered: 0.11.2, or v0.11.2 as a tag. A pre-release
/// (0.6.0-rc.1) comes before its release, and rc.10 after rc.9.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    numbers: Vec<u64>,
    /// false for a pre-release: false sorts first
    released: bool,
    pre: Vec<Part>,
}

/// A part of a pre-release's name: numbers count as numbers, and come before words.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Number(u64),
    Word(String),
}

impl Version {
    fn parse(text: &str) -> Option<Version> {
        let text = text.trim().trim_start_matches('v');
        // what comes after + is a build's name, not part of the version
        let text = text.split('+').next()?;
        let (numbers, pre) = text.split_once('-').unwrap_or((text, ""));
        let parts: Option<Vec<u64>> = numbers.split('.').map(|part| part.parse().ok()).collect();
        let mut numbers = parts.filter(|parts| !parts.is_empty() && parts.len() <= 3)?;
        numbers.resize(3, 0);
        let pre = if pre.is_empty() {
            Vec::new()
        } else {
            pre.split('.').map(|part| part.parse().map_or_else(|_| Part::Word(part.into()), Part::Number)).collect()
        };
        Some(Version { numbers, released: pre.is_empty(), pre })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let numbers: Vec<String> = self.numbers.iter().map(u64::to_string).collect();
        f.write_str(&numbers.join("."))?;
        let pre: Vec<String> = self
            .pre
            .iter()
            .map(|part| match part {
                Part::Number(number) => number.to_string(),
                Part::Word(word) => word.clone(),
            })
            .collect();
        if !pre.is_empty() {
            write!(f, "-{}", pre.join("."))?;
        }
        Ok(())
    }
}

/// Why something couldn't be put in place.
enum Failure {
    /// The network, or what came over it (a login page in place of the release, say), or
    /// a full disk: tried again.
    Again(String),
    /// What would fail the same way next time: said once, and not tried again by the
    /// timer for this version.
    Lasting(String),
}

/// What one update did.
#[derive(Default)]
struct Report {
    /// "glint 0.11.1 → 0.11.2"
    updated: Vec<String>,
    /// What is out but waits for the owner, keyed so that each is told once.
    waiting: Vec<(String, String)>,
    /// Something failed that will fail again the same way.
    failed: bool,
    /// Something failed that may work in a while.
    again: bool,
}

impl Report {
    /// An install that failed. One that would fail again the same way (`failed`, as
    /// `failed seek 0.5.3`) isn't tried again by the timer for this version, and the owner
    /// hears of it once: by notification when the timer ran, in the terminal by hand. The
    /// notice has its own key, so that noting the failure doesn't count as telling of it.
    fn failed(&mut self, failure: &Failure, failed: String, how: How, told: &mut Told) {
        if let Failure::Lasting(why) = failure {
            let notice = format!("told {failed}");
            if how == How::ByHand {
                told.note(notice.clone());
            }
            told.note(failed);
            self.waiting.push((notice, why.clone()));
        }
        self.failure(failure);
    }

    /// The timer skipped what failed before (`failed`) for good: the owner is still told
    /// of it if the notice couldn't be shown when it failed.
    fn still_failing(&mut self, failed: &str, what: &str) {
        self.waiting
            .push((format!("told {failed}"), format!("{what} could not be installed; `lumen update` says why")));
    }

    /// What waits for the owner and wasn't told yet.
    fn news(&self, told: &Told) -> Vec<&(String, String)> {
        self.waiting.iter().filter(|(key, _)| !told.knows(key)).collect()
    }

    fn failure(&mut self, failure: &Failure) {
        match failure {
            Failure::Again(why) => {
                eprintln!("lumen: {why}");
                self.again = true;
            },
            Failure::Lasting(why) => {
                eprintln!("lumen: {why}");
                self.failed = true;
            },
        }
    }
}

fn update(how: How) -> i32 {
    // two at once (the timer, and one by hand) would write over each other
    let _lock = match lock() {
        Ok(lock) => lock,
        Err(why) => {
            eprintln!("lumen: {why}");
            return 1;
        },
    };
    let mut told = Told::load();
    let mut report = Report::default();
    update_lumen(how, &mut told, &mut report);
    let go = find("go", &go_folders()).first().cloned();
    for tool in tools() {
        if let Some((before, now)) = update_tool(&tool, go.as_deref(), how, &mut told, &mut report) {
            update_skill(&tool, &before, &now, go.as_deref(), how, &mut told, &mut report);
        }
    }
    if how == How::Timer {
        tell(&report, &mut told);
    }
    told.save();
    match (report.failed, report.again) {
        (true, _) => 1,
        (false, true) => EX_TEMPFAIL,
        (false, false) => 0,
    }
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

/// What the owner was told of, and what failed for good, so that each is said once and
/// tried once per version: `lumen 0.6.0`, `failed seek 0.5.3`, `seek 0.5.3 needs Go`.
struct Told {
    file: Option<PathBuf>,
    lines: BTreeSet<String>,
    changed: bool,
}

impl Told {
    fn load() -> Told {
        let file = dirs::state_dir().map(|dir| dir.join("lumen/update-told"));
        let lines = file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default();
        Told { file, lines, changed: false }
    }

    fn knows(&self, line: &str) -> bool {
        self.lines.contains(line)
    }

    fn note(&mut self, line: String) {
        self.changed |= self.lines.insert(line);
    }

    fn save(&self) {
        let Some(file) = self.file.as_ref().filter(|_| self.changed) else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self.lines.iter().map(|line| format!("{line}\n")).collect();
        let _ = std::fs::write(file, text);
    }
}

/// curl, told to read no ~/.curlrc (whose `location` would follow the redirect that
/// [`newest_lumen`] reads), over https only, a few times when the network isn't there yet.
fn curl() -> Command {
    let mut curl = Command::new("curl");
    curl.args(["-q", "--proto", "=https", "--tlsv1.2", "-fsS", "--retry", "3"]);
    curl
}

/// Fetches a URL with curl, or with wget where there is no curl.
fn fetch(url: &str) -> Result<Vec<u8>, Failure> {
    fetch_within(url, 300, 3)
}

/// A small file, the list of tools or a skill: given up on soon, so that a network
/// that loses what is sent doesn't hold the update for long.
fn fetch_small(url: &str) -> Result<Vec<u8>, Failure> {
    fetch_within(url, 20, 1)
}

fn fetch_within(url: &str, seconds: u32, tries: u32) -> Result<Vec<u8>, Failure> {
    let (seconds, retries) = (seconds.to_string(), (tries - 1).to_string());
    let fetched = curl().args(["--retry", &retries, "-L", "--max-time", &seconds, url]).stderr(Stdio::piped()).output();
    let output = match fetched {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Command::new("wget")
            .args(["-q", "--https-only", &format!("--tries={tries}"), &format!("--timeout={seconds}"), "-O", "-", url])
            .stderr(Stdio::piped())
            .output()
            .map_err(|_| Failure::Lasting("curl or wget is needed to fetch updates".into()))?,
        Err(e) => return Err(Failure::Again(format!("can't run curl: {e}"))),
    };
    if !output.status.success() {
        return Err(Failure::Again(format!("could not fetch {url}: {}", last_lines(&output.stderr))));
    }
    Ok(output.stdout)
}

/// The tag of lumen's newest release, from where GitHub's page for it leads: unlike
/// GitHub's API, that has no limit of 60 questions an hour.
fn newest_lumen() -> Result<(String, Version), Failure> {
    let url = format!("{GITHUB}/lumen/releases/latest");
    let asked = curl().args(["--max-time", "60", "-o", "/dev/null", "-w", "%{redirect_url}", &url]).output();
    let to = match asked {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
        Ok(output) => return Err(Failure::Again(format!("could not reach {url}: {}", last_lines(&output.stderr)))),
        // wget says where it leads in the headers it prints
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let output = Command::new("wget")
                .args(["--https-only", "--tries=3", "--timeout=60", "--max-redirect=0", "-S", "-O", "/dev/null", &url])
                .output()
                .map_err(|_| Failure::Lasting("curl or wget is needed to look for updates".into()))?;
            let headers = String::from_utf8_lossy(&output.stderr);
            let location = headers.lines().find_map(|line| {
                let (name, value) = line.trim().split_once(':')?;
                name.eq_ignore_ascii_case("location").then(|| value.trim().to_string())
            });
            location.ok_or_else(|| Failure::Again(format!("could not reach {url}")))?
        },
        Err(e) => return Err(Failure::Again(format!("can't run curl: {e}"))),
    };
    let tag = to.rsplit_once("/tag/").map(|(_, tag)| tag.trim().to_string()).unwrap_or_default();
    let version = Version::parse(&tag).ok_or_else(|| Failure::Again(format!("{url} leads to no release: {to:?}")))?;
    Ok((tag, version))
}

/// A tool's newest version: as `go list` finds it, through the user's GOPROXY, as `go
/// install` will; without Go, as Go's module proxy has it.
fn newest_tool(tool: &str, go: Option<&Path>) -> Result<(String, Version), Failure> {
    let module = format!("github.com/Vaspyyy/{tool}@latest");
    let tag = match go {
        Some(go) => {
            let output = Command::new(go)
                .args(["list", "-m", "-f", "{{.Version}}", &module])
                // outside any module, whose go.mod would have a say
                .current_dir("/")
                .stdin(Stdio::null())
                .output()
                .map_err(|e| Failure::Again(format!("can't run go: {e}")))?;
            if !output.status.success() {
                return Err(Failure::Again(format!(
                    "go can't find {tool}'s newest version: {}",
                    last_lines(&output.stderr)
                )));
            }
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        },
        None => {
            let body = fetch(&format!("{PROXY}/{tool}/@latest"))?;
            let latest: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| Failure::Again(format!("the Go proxy's answer about {tool} is not JSON: {e}")))?;
            latest.get("Version").and_then(serde_json::Value::as_str).unwrap_or_default().to_string()
        },
    };
    let version =
        Version::parse(&tag).ok_or_else(|| Failure::Again(format!("{tool} has no newest version: {tag:?}")))?;
    Ok((tag, version))
}

/// Whether a version is one Go made up for a commit without a tag (as @latest of a tool
/// without releases is): `v0.0.0-20261113214351-05cf82c0ff12`. Only releases are
/// installed, never the work on main.
fn pseudo(tag: &str) -> bool {
    // "+incompatible" after a v2 or later without /v2 in the module path
    let tag = tag.split('+').next().unwrap_or_default();
    let mut parts = tag.rsplitn(2, '-');
    let (Some(commit), Some(rest)) = (parts.next(), parts.next()) else { return false };
    let time = rest.rsplit(['-', '.']).next().unwrap_or_default();
    commit.len() == 12
        && commit.bytes().all(|b| b.is_ascii_hexdigit())
        && time.len() == 14
        && time.bytes().all(|b| b.is_ascii_digit())
}

fn update_lumen(how: How, told: &mut Told, report: &mut Report) {
    let ours = Version::parse(env!("CARGO_PKG_VERSION")).expect("lumen's own version");
    let (tag, newest) = match newest_lumen() {
        Ok(found) => found,
        Err(failure) => return report.failure(&failure),
    };
    if newest <= ours {
        println!("lumen {ours} is the newest");
        return;
    }
    let Some(exe) = std::env::current_exe().ok() else {
        return report.failure(&Failure::Lasting("can't tell where this lumen is".into()));
    };
    let dir = match kind(&exe) {
        Kind::Ours(dir) => dir,
        Kind::Package => {
            println!("lumen {newest} is out (this is {ours}); install its new package: {DOCS}");
            report.waiting.push((format!("lumen {newest}"), format!("lumen {newest} is out: install its new package")));
            return;
        },
        Kind::Build => {
            println!("lumen {newest} is out (this is {ours}); this is a build in a cargo target folder, so it stays");
            return;
        },
    };
    if std::env::consts::ARCH != "x86_64" {
        println!("lumen {newest} is out (this is {ours}), but not built for {}: {DOCS}", std::env::consts::ARCH);
        return;
    }
    let failed = format!("failed lumen {newest}");
    if how == How::Timer && told.knows(&failed) {
        println!("lumen {newest} is out, and could not be installed here; `lumen update` tries again");
        return report.still_failing(&failed, &format!("lumen {newest}"));
    }
    if how == How::Check {
        println!("lumen {ours} → {newest}");
        return;
    }
    match install_lumen(&tag, &dir) {
        Ok(()) => {
            println!("lumen {ours} → {newest}");
            report.updated.push(format!("lumen {ours} → {newest}"));
        },
        Err(Failure::Lasting(why)) => {
            let why = format!("lumen {newest} could not be installed: {why}");
            report.failed(&Failure::Lasting(why), failed, how, told);
        },
        Err(failure) => report.failed(&failure, failed, how, told),
    }
}

/// How the running lumen was installed.
enum Kind {
    /// By install.sh or from source, into this folder of the user's: the update installs here.
    Ours(PathBuf),
    /// By a package manager, under /usr: only it may change it.
    Package,
    /// A build in a cargo target folder, not installed at all.
    Build,
}

fn kind(exe: &Path) -> Kind {
    // a program that was replaced since it started is named "lumen (deleted)" here
    let exe = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let Some(dir) = exe.parent() else { return Kind::Package };
    let system = ["/usr", "/bin", "/sbin", "/opt", "/nix"].iter().any(|root| dir.starts_with(root));
    if system || !writable(dir) {
        return Kind::Package;
    }
    // cargo marks every target folder with this file: target/release, target/<triple>/debug
    if dir.ancestors().take(3).any(|folder| folder.join("CACHEDIR.TAG").is_file()) {
        return Kind::Build;
    }
    Kind::Ours(dir.to_path_buf())
}

/// Whether this user may write to a folder.
fn writable(dir: &Path) -> bool {
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return false };
    // SAFETY: access with a string that ends with its NUL
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

/// Fetches the release `tag`, checks it, and has its own install.sh put it into `dir`.
fn install_lumen(tag: &str, dir: &Path) -> Result<(), Failure> {
    // what comes over the network may be a login page in its place: tried again
    let base = format!("{GITHUB}/lumen/releases/download/{tag}");
    let sums = String::from_utf8(fetch(&format!("{base}/SHA256SUMS"))?)
        .map_err(|_| Failure::Again("SHA256SUMS is not text".into()))?;
    let want = checksum_of(&sums, ARCHIVE)
        .ok_or_else(|| Failure::Again(format!("the release's SHA256SUMS doesn't list {ARCHIVE}")))?;
    let archive = fetch(&format!("{base}/{ARCHIVE}"))?;
    let have = hex(&Sha256::digest(&archive));
    if have != want {
        return Err(Failure::Again(format!(
            "what was fetched is damaged: its checksum is {have}, the release says {want}"
        )));
    }

    // unpacked next to the program, as install.sh does: /tmp may not let it start
    let work = dir.join(WORK);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir(&work).map_err(|e| Failure::Again(format!("can't make {}: {e}", work.display())))?;
    let result = (|| {
        std::fs::write(work.join(ARCHIVE), &archive)
            .map_err(|e| Failure::Again(format!("can't write the archive: {e}")))?;
        let unpacked = Command::new("tar")
            .arg("-xzf")
            .arg(work.join(ARCHIVE))
            .arg("-C")
            .arg(&work)
            .status()
            .map_err(|e| Failure::Lasting(format!("can't run tar: {e}")))?;
        if !unpacked.success() {
            return Err(Failure::Again("tar could not unpack the archive".into()));
        }
        let folder = std::fs::read_dir(&work)
            .map_err(|e| Failure::Again(e.to_string()))?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.is_dir() && path.join("install.sh").is_file())
            .ok_or_else(|| Failure::Lasting("the archive doesn't hold install.sh".into()))?;
        let mut install = Command::new("sh");
        install.arg(folder.join("install.sh")).env("LUMEN_BIN_DIR", dir).stdin(Stdio::null());
        // what was left out or turned off stays so: only services that are there are kept up
        let units = units_dir();
        if !units.join("lumen.service").exists() {
            install.env("LUMEN_DAEMON", "0");
        }
        if !units.join("lumen-update.timer").exists() {
            install.env("LUMEN_AUTO_UPDATE", "0");
        }
        let output = install.output().map_err(|e| Failure::Lasting(format!("can't run install.sh: {e}")))?;
        // what it said goes where this update's output goes: the journal, for the timer
        print!("{}", String::from_utf8_lossy(&output.stdout));
        if !output.status.success() {
            return Err(Failure::Lasting(format!("install.sh failed: {}", last_lines(&output.stderr))));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// The last lines a program said: what went wrong, without everything before it.
fn last_lines(said: &[u8]) -> String {
    let said = String::from_utf8_lossy(said);
    let lines: Vec<&str> = said.lines().filter(|line| !line.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join(" / ")
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

/// Updates a tool where it is installed, and gives the version it was at and the one it
/// is at now.
fn update_tool(
    tool: &str,
    go: Option<&Path>,
    how: How,
    told: &mut Told,
    report: &mut Report,
) -> Option<(Version, Version)> {
    // a tool that isn't installed stays so, and so does another program of its name
    let (program, ours) = find_tool(tool, go)?;
    let (tag, newest) = match newest_tool(tool, go) {
        Ok(found) => found,
        Err(failure) => {
            report.failure(&failure);
            return Some((ours.clone(), ours));
        },
    };
    if pseudo(&tag) {
        println!("{tool} has no release yet, so it stays as it is");
        return Some((ours.clone(), ours));
    }
    if newest <= ours {
        println!("{tool} {ours} is the newest");
        return Some((ours.clone(), ours));
    }
    let Some(go) = go else {
        println!("{tool} {newest} is out (this is {ours}); updating it needs Go, which isn't installed");
        report
            .waiting
            .push((format!("{tool} {newest} needs Go"), format!("{tool} {newest} is out: updating it needs Go")));
        return Some((ours.clone(), ours));
    };
    let folder = program.parent().unwrap_or(Path::new("/")).to_path_buf();
    if !writable(&folder) {
        println!("{tool} {newest} is out (this is {ours}); {} is not yours to write to", folder.display());
        report.waiting.push((
            format!("{tool} {newest} not writable"),
            format!("{tool} {newest} is out: {} is not yours to write to", folder.display()),
        ));
        return Some((ours.clone(), ours));
    }
    let failed = format!("failed {tool} {newest}");
    if how == How::Timer && told.knows(&failed) {
        println!("{tool} {newest} is out, and could not be installed here; `lumen update` tries again");
        report.still_failing(&failed, &format!("{tool} {newest}"));
        return Some((ours.clone(), ours));
    }
    if how == How::Check {
        println!("{tool} {ours} → {newest}");
        return Some((ours.clone(), ours));
    }
    let installed = Command::new(go)
        .args(["install", &format!("github.com/Vaspyyy/{tool}@{tag}")])
        // into the folder the tool's file is in, not wherever Go puts programs by default
        .env("GOBIN", &folder)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output();
    let failure = match installed {
        Ok(output)
            if output.status.success() && tool_version(tool, &program, Some(go)).is_some_and(|now| now >= newest) =>
        {
            println!("{tool} {ours} → {newest}");
            report.updated.push(format!("{tool} {ours} → {newest}"));
            return Some((ours, newest));
        },
        Ok(output) => {
            let why = format!("{tool} {newest} is out, and go install failed: {}", last_lines(&output.stderr));
            let network =
                ["dial tcp", "i/o timeout", "TLS handshake", "no such host", "connection re", "Temporary failure"];
            let said = String::from_utf8_lossy(&output.stderr);
            if network.iter().any(|sign| said.contains(sign)) { Failure::Again(why) } else { Failure::Lasting(why) }
        },
        Err(e) => Failure::Again(format!("can't run go: {e}")),
    };
    report.failed(&failure, failed, how, told);
    Some((ours.clone(), ours))
}

/// The tools made for lumen: the list in lumen's repository, or where it can't be read,
/// the ones this lumen knows.
fn tools() -> Vec<String> {
    fetch_small(TOOL_LIST)
        .ok()
        .map(|list| tool_names(&String::from_utf8_lossy(&list)))
        .filter(|names| !names.is_empty())
        .unwrap_or_else(|| TOOLS.iter().map(|name| name.to_string()).collect())
}

/// The names in the list of tools: one a line, `#` starts a remark. Only a plain name
/// counts, since it becomes `github.com/Vaspyyy/<name>`, and lumen is updated apart. A
/// page that doesn't start as the list does (a network's login page, say) has none.
fn tool_names(list: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if !list.starts_with(LIST_HEADER) {
        return names;
    }
    for line in list.lines() {
        let name = line.split('#').next().unwrap_or_default().trim();
        let plain = name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if plain && !name.is_empty() && !name.starts_with('-') && name != "lumen" && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

/// The folders agents keep their skills in: a tool's is `<folder>/<tool>/SKILL.md`.
fn skill_folders() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
    vec![
        home.join(".claude/skills"),
        config.join("opencode/skills"),
        home.join(".agents/skills"),
        home.join(".codex/skills"),
    ]
    .into_iter()
    .filter(|dir| dir.is_absolute())
    .collect()
}

/// Brings a tool's agent skill, wherever an agent has one, from the skill of version
/// `before` (or of another release) to the one of version `now`. Only a skill that is
/// the one of a release is replaced: one changed by hand stays as it is.
fn update_skill(
    tool: &str,
    before: &Version,
    now: &Version,
    go: Option<&Path>,
    how: How,
    told: &mut Told,
    report: &mut Report,
) {
    let home = dirs::home_dir().unwrap_or_default();
    let mut seen = BTreeSet::new();
    let files: Vec<PathBuf> = skill_folders()
        .into_iter()
        .map(|dir| dir.join(tool).join("SKILL.md"))
        .filter(|file| file.is_file())
        // links followed, so that a skill linked from another agent's folder is one file
        .map(|file| std::fs::canonicalize(&file).unwrap_or(file))
        // one in a git checkout (the tool's own, where it is worked on) is the checkout's;
        // a home that is a checkout of its own (of dotfiles) doesn't count
        .filter(|file| !in_checkout(file, &home))
        .filter(|file| seen.insert(file.clone()))
        .collect();
    if files.is_empty() {
        return;
    }
    let cache = module_cache(go);
    // a version without a skill, or one that can't be fetched now: next time
    let Some(wanted) = skill_of(tool, now, cache.as_deref()) else { return };
    let mut released = cached_skills(tool, cache.as_deref());
    // the module cache may have lost the version that was installed: fetched then
    if before != now
        && before.released
        && let Some(skill) = skill_of(tool, before, cache.as_deref())
    {
        released.push(skill);
    }
    let mut updated = false;
    for file in files {
        match put_skill(&file, &wanted, &released, how) {
            Skill::Current => {},
            Skill::Changed => {
                println!("{tool}'s skill in {} is none that a release had, so it stays as it is", file.display())
            },
            Skill::Pinned => println!("{tool}'s skill in {} is read-only, so it stays as it is", file.display()),
            Skill::Old => println!("{tool}'s skill in {} → {now}", file.display()),
            Skill::Replaced => {
                println!("{tool}'s skill in {} → {now}", file.display());
                updated = true;
            },
            Skill::Failed(why) => {
                eprintln!("lumen: can't write {tool}'s skill in {}: {why}", file.display());
                let key = format!("{tool}'s skill {now} not written: {}", file.display());
                if how == How::ByHand {
                    told.note(key.clone());
                }
                report.waiting.push((key, format!("{tool}'s skill in {} can't be updated: {why}", file.display())));
            },
        }
    }
    if updated {
        report.updated.push(format!("{tool}'s skill → {now}"));
    }
}

/// Whether a file is in a git checkout below `home`: a folder above it, but not `home`
/// itself or what is above that, has a `.git` (a folder, or a worktree's file).
fn in_checkout(file: &Path, home: &Path) -> bool {
    file.ancestors().skip(1).take_while(|dir| !home.starts_with(dir)).any(|dir| dir.join(".git").exists())
}

/// What became of one skill file.
#[derive(Debug, PartialEq)]
enum Skill {
    /// It is the skill of the tool's version already.
    Current,
    /// It is no released skill: changed by hand, so it stays.
    Changed,
    /// It is read-only: kept as it is on purpose.
    Pinned,
    /// It is an older release's, and only checked.
    Old,
    /// It was an older release's, and is the new one now.
    Replaced,
    Failed(String),
}

/// Puts `wanted` in `file` if what is there is one of the `released` skills, through a
/// file beside it that takes its place at once, so that no agent reads half a skill.
fn put_skill(file: &Path, wanted: &[u8], released: &[Vec<u8>], how: How) -> Skill {
    let Ok(have) = std::fs::read(file) else { return Skill::Failed("can't read it".into()) };
    if have == wanted {
        return Skill::Current;
    }
    if !released.contains(&have) {
        return Skill::Changed;
    }
    let Ok(meta) = std::fs::metadata(file) else { return Skill::Failed("can't read it".into()) };
    if meta.permissions().readonly() {
        return Skill::Pinned;
    }
    if how == How::Check {
        return Skill::Old;
    }
    let next = file.with_file_name(".SKILL.md.lumen-update");
    // made anew, never through a link that someone left in its place
    let _ = std::fs::remove_file(&next);
    let put = File::options()
        .write(true)
        .create_new(true)
        .mode(meta.permissions().mode() & 0o7777)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&next)
        .and_then(|mut new| {
            new.write_all(wanted)?;
            new.set_permissions(meta.permissions())?;
            new.sync_all()
        })
        .and_then(|()| std::fs::rename(&next, file));
    match put {
        Ok(()) => Skill::Replaced,
        Err(e) => {
            let _ = std::fs::remove_file(&next);
            Skill::Failed(e.to_string())
        },
    }
}

/// Go's module cache, where `go install` left the tools' files, each version's apart.
fn module_cache(go: Option<&Path>) -> Option<PathBuf> {
    if let Some(go) = go
        && let Ok(output) = Command::new(go).args(["env", "GOMODCACHE"]).current_dir("/").stderr(Stdio::null()).output()
    {
        let said = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !said.is_empty() {
            return Some(PathBuf::from(said));
        }
    }
    std::env::var_os("GOMODCACHE")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join("go/pkg/mod")))
        .filter(|dir| dir.is_absolute())
}

/// The skill of a tool's version: from Go's module cache, or fetched by its tag. None
/// for a version without one.
fn skill_of(tool: &str, version: &Version, cache: Option<&Path>) -> Option<Vec<u8>> {
    let tag = format!("v{version}");
    if let Some(cache) = cache
        && let Ok(skill) = std::fs::read(cache.join(format!("github.com/!vaspyyy/{tool}@{tag}/skill/SKILL.md")))
    {
        return Some(skill);
    }
    fetch_small(&format!("{RAW}/{tool}/{tag}/skill/SKILL.md")).ok()
}

/// The skills of every release of a tool in Go's module cache (not of commits without a
/// tag, which `go install …@main` leaves there too).
fn cached_skills(tool: &str, cache: Option<&Path>) -> Vec<Vec<u8>> {
    let Some(entries) = cache.and_then(|cache| std::fs::read_dir(cache.join("github.com/!vaspyyy")).ok()) else {
        return Vec::new();
    };
    let prefix = format!("{tool}@");
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_prefix(&prefix).is_some_and(|tag| !pseudo(tag))
        })
        .filter_map(|entry| std::fs::read(entry.path().join("skill/SKILL.md")).ok())
        .collect()
}

/// The program file of a tool and its version: the first on `PATH` or in Go's folders
/// that is the tool, with links followed, so that `go install` replaces the file itself
/// and not a link to it.
fn find_tool(tool: &str, go: Option<&Path>) -> Option<(PathBuf, Version)> {
    let mut seen = BTreeSet::new();
    find(tool, &tool_folders(go))
        .into_iter()
        .map(|found| std::fs::canonicalize(&found).unwrap_or(found))
        .filter(|program| seen.insert(program.clone()))
        .find_map(|program| tool_version(tool, &program, go).map(|version| (program, version)))
}

/// The version of a tool that Go built from its module, github.com/Vaspyyy/<tool>; None
/// for another program of the same name. A build from a checkout has no version in its
/// module information, and says it with `--version`.
fn tool_version(tool: &str, program: &Path, go: Option<&Path>) -> Option<Version> {
    let said = |program: &Path| -> Option<Version> {
        let output = Command::new(program).arg("--version").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut words = text.split_whitespace();
        // "glint 0.11.2": the tool's name first
        (words.next()? == tool).then_some(())?;
        words.find_map(Version::parse)
    };
    let Some(go) = go else { return said(program) };
    let output = Command::new(go).args(["version", "-m"]).arg(program).stderr(Stdio::null()).output().ok()?;
    let info = String::from_utf8_lossy(&output.stdout);
    let module = format!("github.com/Vaspyyy/{tool}");
    let field = |name: &str| {
        info.lines().find_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next() == Some(name)).then(|| fields.collect::<Vec<_>>())
        })
    };
    if field("path")?.first() != Some(&module.as_str()) {
        return None;
    }
    field("mod").and_then(|fields| fields.get(1).and_then(|version| Version::parse(version))).or_else(|| said(program))
}

/// Every program of this name on `PATH` and in the folders named after it, in that order.
/// The timer runs with systemd's `PATH`, which has no folder of the user's own.
fn find(program: &str, extra: &[PathBuf]) -> Vec<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .chain(extra.iter().cloned())
        .map(|dir| dir.join(program))
        .filter(|file| file.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0))
        .collect()
}

/// Where `go install` puts programs, as Go says (`go env -w` may have set it) and as the
/// environment says, then `~/go/bin`. And `~/.local/bin`, where a tool may have been copied.
fn tool_folders(go: Option<&Path>) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut folders = Vec::new();
    if let Some(go) = go
        && let Ok(output) = Command::new(go).args(["env", "GOBIN", "GOPATH"]).stderr(Stdio::null()).output()
    {
        let said = String::from_utf8_lossy(&output.stdout);
        let mut lines = said.lines();
        if let Some(bin) = lines.next().filter(|bin| !bin.is_empty()) {
            folders.push(PathBuf::from(bin));
        }
        if let Some(gopath) = lines.next() {
            folders.extend(std::env::split_paths(gopath).map(|dir| dir.join("bin")));
        }
    }
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

/// Says on the desktop what the update did, and what is out but waits for the owner, each
/// once. A notice that couldn't be shown is shown next time.
fn tell(report: &Report, told: &mut Told) {
    let news = report.news(told);
    let mut lines = report.updated.clone();
    lines.extend(news.iter().map(|(_, line)| line.clone()));
    if lines.is_empty() {
        return;
    }
    let title = if report.updated.is_empty() { "lumen: updates are out" } else { "lumen and its tools are updated" };
    // the desktop's notifications, where notify-send is there to reach them
    let shown = Command::new("notify-send")
        .args(["--app-name=lumen", "--icon=lumen", title, &lines.join("\n")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if shown {
        for (key, _) in news {
            told.note(key.clone());
        }
    }
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
        assert_eq!(v("0.6.0+build.7"), v("0.6.0"));
        assert_eq!(Version::parse("(devel)"), None);
        assert_eq!(Version::parse(""), None);
        assert_eq!(v("0.5.2").to_string(), "0.5.2");
    }

    #[test]
    fn a_pre_release_comes_before_its_release() {
        let v = |text| Version::parse(text).unwrap();
        assert!(v("0.6.0-rc.1") < v("0.6.0"));
        assert!(v("0.6.0-rc.1") > v("0.5.2"));
        assert!(v("0.6.0-rc.10") > v("0.6.0-rc.9"));
        assert!(v("0.6.0-rc.1") > v("0.6.0-beta.2"));
        assert_eq!(v("0.6.0-rc.1").to_string(), "0.6.0-rc.1");
    }

    #[test]
    fn the_checksum_is_found_by_the_files_name() {
        let sums = "aaaa  lumen-terminal_amd64.deb\nBBBB *lumen-x86_64-linux.tar.gz\n";
        assert_eq!(checksum_of(sums, ARCHIVE).as_deref(), Some("bbbb"));
        assert_eq!(checksum_of(sums, "other"), None);
    }

    #[test]
    fn a_lumen_in_a_system_folder_or_a_cargo_build_is_not_replaced() {
        assert!(matches!(kind(Path::new("/usr/bin/lumen")), Kind::Package));
        let root = std::env::temp_dir().join(format!("lumen-kind-{}", std::process::id()));
        let file = |path: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
            path
        };
        file("cache/CACHEDIR.TAG");
        assert!(matches!(kind(&file("cache/release/lumen")), Kind::Build));
        assert!(matches!(kind(&file("cache/x86_64-unknown-linux-gnu/debug/lumen")), Kind::Build));
        // a folder named target is no build without cargo's mark
        assert!(matches!(kind(&file("target/bin/lumen")), Kind::Ours(dir) if dir.ends_with("target/bin")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_lasting_failure_is_told_once_and_not_tried_again_by_the_timer() {
        let mut told = Told { file: None, lines: BTreeSet::new(), changed: false };
        let lasting = || Failure::Lasting("go install failed".into());

        // the timer's run: the notice is news, and the version is skipped from now on
        let mut report = Report::default();
        report.failed(&lasting(), "failed seek 0.5.3".into(), How::Timer, &mut told);
        assert!(told.knows("failed seek 0.5.3"));
        assert_eq!(report.news(&told).len(), 1);

        // shown: the next run skips the version and says nothing
        let shown: Vec<String> = report.news(&told).iter().map(|(key, _)| key.clone()).collect();
        shown.into_iter().for_each(|key| told.note(key));
        let mut report = Report::default();
        report.still_failing("failed seek 0.5.3", "seek 0.5.3");
        assert!(report.news(&told).is_empty());

        // by hand, the terminal said it: no notification follows
        let mut report = Report::default();
        report.failed(&lasting(), "failed glint 1.0.0".into(), How::ByHand, &mut told);
        assert!(report.news(&told).is_empty());
        let mut report = Report::default();
        report.still_failing("failed glint 1.0.0", "glint 1.0.0");
        assert!(report.news(&told).is_empty());
    }

    #[test]
    fn a_commit_without_a_tag_is_no_release() {
        assert!(pseudo("v0.0.0-20261113214351-05cf82c0ff12"));
        assert!(pseudo("v0.5.3-0.20261113214351-05cf82c0ff12"));
        assert!(pseudo("v1.2.0-rc.1.0.20261113214351-05cf82c0ff12"));
        assert!(!pseudo("v0.5.2"));
        assert!(!pseudo("v1.0.0-rc.1"));
        assert!(!pseudo("v1.0.0-beta-05cf82c0ff12"));
        assert!(pseudo("v2.0.0-20261113214351-05cf82c0ff12+incompatible"));
    }

    #[test]
    fn the_list_of_tools_takes_plain_names_once() {
        let list = format!("{LIST_HEADER}, …\nglint\ncrux  # remark\n\nglint\nlumen\n../evil\nBig\n-x\nnew-tool2\n");
        assert_eq!(tool_names(&list), ["glint", "crux", "new-tool2"]);
        assert!(tool_names("<html>Not Found</html>").is_empty());
        assert!(tool_names("404\nglint\n").is_empty(), "a page that isn't the list");
        let shipped = include_str!("../packaging/tools.txt");
        assert_eq!(tool_names(shipped), TOOLS, "the list in the repository and the one built in");
    }

    #[test]
    fn a_skill_is_replaced_only_when_a_release_had_it() {
        let dir = std::env::temp_dir().join(format!("lumen-skill-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("SKILL.md");
        let released = vec![b"old skill\n".to_vec(), b"older skill\n".to_vec()];

        std::fs::write(&file, "new skill\n").unwrap();
        assert_eq!(put_skill(&file, b"new skill\n", &released, How::ByHand), Skill::Current);

        std::fs::write(&file, "old skill, and my own line\n").unwrap();
        assert_eq!(put_skill(&file, b"new skill\n", &released, How::ByHand), Skill::Changed);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old skill, and my own line\n");

        std::fs::write(&file, "older skill\n").unwrap();
        assert_eq!(put_skill(&file, b"new skill\n", &released, How::Check), Skill::Old);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "older skill\n");
        assert_eq!(put_skill(&file, b"new skill\n", &released, How::Timer), Skill::Replaced);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new skill\n");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "the file beside it is gone");

        // a link where the file beside it goes is not written through
        let elsewhere = dir.join("elsewhere");
        std::fs::write(&elsewhere, "mine\n").unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join(".SKILL.md.lumen-update")).unwrap();
        std::fs::write(&file, "old skill\n").unwrap();
        assert_eq!(put_skill(&file, b"new skill\n", &released, How::Timer), Skill::Replaced);
        assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), "mine\n");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new skill\n");

        // a read-only skill is kept on purpose
        std::fs::write(&file, "old skill\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
        assert_eq!(put_skill(&file, b"new skill\n", &released, How::Timer), Skill::Pinned);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_skill_in_a_checkout_is_the_checkouts_but_a_home_that_is_one_doesnt_count() {
        let home = Path::new("/h");
        let dir = std::env::temp_dir().join(format!("lumen-checkout-test-{}", std::process::id()));
        let file = dir.join("repo/skill/SKILL.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        assert!(!in_checkout(&file, home));
        std::fs::write(dir.join("repo/.git"), "gitdir: elsewhere\n").unwrap(); // a worktree's
        assert!(in_checkout(&file, home));
        // the same with the folder above as home: a home's own .git doesn't count
        assert!(!in_checkout(&file, &dir.join("repo")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_last_lines_are_what_went_wrong() {
        let said = b"go: downloading a\ngo: downloading b\n\nerror one\nerror two\n";
        assert_eq!(last_lines(said), "go: downloading b / error one / error two");
    }
}
