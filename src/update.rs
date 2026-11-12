//! `lumen update`: lumen and the tools made for it, at their newest releases.
//!
//! lumen comes from its release on GitHub: the archive is fetched, checked
//! against the release's SHA256SUMS, and installed by the `install.sh` in it,
//! into the folder the running program is in. A lumen built from source is
//! replaced by the release once a newer one is out. One from a package (under
//! /usr, which root owns) is left to the package manager, and its owner is
//! told once per version. A build in a cargo `target` folder is left alone.
//!
//! The tools (glint, crux, recap, seek, pulse) are installed with `go
//! install`, and updated the same way, each into the folder its program file
//! is in. Only the ones that are installed, that Go built from their module,
//! are updated, and only where Go is. Their newest version is the one Go's
//! module proxy names, which is what `go install` takes.
//!
//! Nothing is ever replaced by an older version. `lumen-update.timer` runs
//! this once a day, with `--notify`, at the lowest priority.

use std::collections::BTreeSet;
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
/// Go's module proxy, where `go install` looks: the module path's capitals as `!` and the letter.
const PROXY: &str = "https://proxy.golang.org/github.com/!vaspyyy";
const DOCS: &str = "https://vaspyyy.github.io/lumen/docs/install/#updates";
const TOOLS: &[&str] = &["glint", "crux", "recap", "seek", "pulse"];
/// The archive of a release, for the one machine releases are built for.
const ARCHIVE: &str = "lumen-x86_64-linux.tar.gz";
/// Where an update unpacks a release, next to the program: one left by an update that
/// was stopped is cleared by the next.
const WORK: &str = ".lumen-update";

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
    // SAFETY: geteuid can't fail
    if unsafe { libc::geteuid() } == 0 {
        eprintln!(
            "lumen: update runs as the user lumen belongs to, not as root: a package is updated by its package manager"
        );
        return Some(1);
    }
    Some(update(check, notify))
}

/// A version as releases are numbered: 0.11.2, or v0.11.2 as a tag. A pre-release
/// (0.6.0-rc.1) comes before its release.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    numbers: Vec<u64>,
    /// false for a pre-release: false sorts first
    released: bool,
    pre: String,
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
        Some(Version { numbers, released: pre.is_empty(), pre: pre.to_string() })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let numbers: Vec<String> = self.numbers.iter().map(u64::to_string).collect();
        f.write_str(&numbers.join("."))?;
        if !self.released {
            write!(f, "-{}", self.pre)?;
        }
        Ok(())
    }
}

/// What one update did, for the notification.
#[derive(Default)]
struct Report {
    /// "glint 0.11.1 → 0.11.2"
    updated: Vec<String>,
    /// What is out but can't be put in place here, keyed so that each is told once.
    waiting: Vec<(String, String)>,
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
    let mut told = Told::load();
    let mut report = Report::default();
    update_lumen(check, &mut told, &mut report);
    for tool in TOOLS {
        update_tool(tool, check, &mut report);
    }
    if notify {
        tell(&report, &mut told);
    }
    told.save();
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

/// What the owner was told of, or what failed, so that it is said and tried once per
/// version: `lumen 0.6.0`, `failed lumen 0.6.0`, `seek 0.5.3 needs Go`.
struct Told {
    file: Option<PathBuf>,
    lines: BTreeSet<String>,
}

impl Told {
    fn load() -> Told {
        let file = dirs::state_dir().map(|dir| dir.join("lumen/update-told"));
        let lines = file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default();
        Told { file, lines }
    }

    fn knows(&self, line: &str) -> bool {
        self.lines.contains(line)
    }

    fn note(&mut self, line: String) {
        self.lines.insert(line);
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self.lines.iter().map(|line| format!("{line}\n")).collect();
        let _ = std::fs::write(file, text);
    }
}

/// Runs curl, or wget where there is no curl, over https only, a few times when the
/// network isn't there yet.
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let curl = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-fsSL", "--retry", "3", "--max-time", "300", url])
        .stderr(Stdio::piped())
        .output();
    let output = match curl {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Command::new("wget")
            .args(["-q", "--https-only", "--tries=3", "--timeout=300", "-O", "-", url])
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

/// The tag of lumen's newest release, from where GitHub's page for it leads: unlike
/// GitHub's API, that has no limit of 60 questions an hour.
fn newest_lumen() -> Result<(String, Version), String> {
    let url = format!("{GITHUB}/lumen/releases/latest");
    let output = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-fsS", "--retry", "3", "--max-time", "60"])
        .args(["-o", "/dev/null", "-w", "%{redirect_url}", &url])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("curl is needed to look for lumen's newest release: {e}"))?;
    if !output.status.success() {
        return Err(format!("could not reach {url}: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    let to = String::from_utf8_lossy(&output.stdout);
    let tag = to.rsplit_once("/tag/").map(|(_, tag)| tag.trim().to_string()).unwrap_or_default();
    let version = Version::parse(&tag).ok_or_else(|| format!("{url} leads to no release: {to:?}"))?;
    Ok((tag, version))
}

/// A tool's newest version, as Go's module proxy has it.
fn newest_tool(tool: &str) -> Result<(String, Version), String> {
    let body = fetch(&format!("{PROXY}/{tool}/@latest"))?;
    let latest: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("the Go proxy's answer about {tool} is not JSON: {e}"))?;
    let tag = latest.get("Version").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let version = Version::parse(&tag).ok_or_else(|| format!("{tool} has no version on the Go proxy: {tag:?}"))?;
    Ok((tag, version))
}

fn update_lumen(check: bool, told: &mut Told, report: &mut Report) {
    let ours = Version::parse(env!("CARGO_PKG_VERSION")).expect("lumen's own version");
    let (tag, newest) = match newest_lumen() {
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
    let Some(exe) = std::env::current_exe().ok() else {
        eprintln!("lumen: can't tell where this lumen is");
        report.failed = true;
        return;
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
    if check {
        println!("lumen {ours} → {newest}");
        return;
    }
    // a release that couldn't be installed here is tried again once a newer one is out
    let failed = format!("failed lumen {newest}");
    if told.knows(&failed) {
        println!(
            "lumen {newest} is out, and could not be installed here before; see `journalctl --user -u lumen-update`"
        );
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
            // not again for this version, unless it was only the network
            if !why.starts_with("could not fetch") {
                told.note(failed);
                report
                    .waiting
                    .push((format!("lumen {newest} failed"), format!("lumen {newest} could not be installed: {why}")));
            }
        },
    }
}

/// How the running lumen was installed.
enum Kind {
    /// By install.sh or from source, into this folder of the user's: the update installs here.
    Ours(PathBuf),
    /// By a package manager, under /usr: only it may change it.
    Package,
    /// A build in a cargo `target` folder, not installed at all.
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
    let build = dir.ends_with("target/debug") || dir.ends_with("target/release");
    if build || dir.parent().is_some_and(|parent| parent.file_name().is_some_and(|name| name == "target")) {
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
    let work = dir.join(WORK);
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
        install.arg(folder.join("install.sh")).env("LUMEN_BIN_DIR", dir).stdin(Stdio::null());
        // what was left out or turned off stays so: only services that are there are kept up
        let units = units_dir();
        if !units.join("lumen.service").exists() {
            install.env("LUMEN_DAEMON", "0");
        }
        if !units.join("lumen-update.timer").exists() {
            install.env("LUMEN_AUTO_UPDATE", "0");
        }
        let output = install.output().map_err(|e| format!("can't run install.sh: {e}"))?;
        // what it said goes where this update's output goes: the journal, for the timer
        print!("{}", String::from_utf8_lossy(&output.stdout));
        if !output.status.success() {
            return Err(format!("install.sh failed: {}", last_lines(&output.stderr)));
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

fn update_tool(tool: &str, check: bool, report: &mut Report) {
    let go = find("go", &go_folders());
    // a tool that isn't installed stays so
    let Some(program) = find_tool(tool, go.as_deref()) else { return };
    let Some(ours) = tool_version(tool, &program, go.as_deref()) else {
        println!("{tool}: {} is not {tool} as Go built it, so it stays as it is", program.display());
        return;
    };
    let (tag, newest) = match newest_tool(tool) {
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
    let folder = program.parent().unwrap_or(Path::new("/")).to_path_buf();
    if check {
        println!("{tool} {ours} → {newest}");
        return;
    }
    let Some(go) = go else {
        println!("{tool} {newest} is out (this is {ours}); updating it needs Go, which isn't installed");
        report
            .waiting
            .push((format!("{tool} {newest} needs Go"), format!("{tool} {newest} is out: updating it needs Go")));
        return;
    };
    if !writable(&folder) {
        println!("{tool} {newest} is out (this is {ours}); {} is not yours to write to", folder.display());
        report.waiting.push((
            format!("{tool} {newest} not writable"),
            format!("{tool} {newest} is out: {} is not yours to write to", folder.display()),
        ));
        return;
    }
    let installed = Command::new(&go)
        .args(["install", &format!("github.com/Vaspyyy/{tool}@{tag}")])
        // into the folder the tool's file is in, not wherever Go puts programs by default
        .env("GOBIN", &folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output();
    match installed {
        Ok(output)
            if output.status.success() && tool_version(tool, &program, Some(&go)).is_some_and(|now| now >= newest) =>
        {
            println!("{tool} {ours} → {newest}");
            report.updated.push(format!("{tool} {ours} → {newest}"));
        },
        Ok(output) => {
            eprintln!("lumen: {tool} {newest} is out, and go install failed: {}", last_lines(&output.stderr));
            report.failed = true;
        },
        Err(e) => {
            eprintln!("lumen: can't run go: {e}");
            report.failed = true;
        },
    }
}

/// The program file of a tool: the first on `PATH` or in Go's folders, with links followed,
/// so that `go install` replaces the file itself and not a link to it.
fn find_tool(tool: &str, go: Option<&Path>) -> Option<PathBuf> {
    let found = find(tool, &tool_folders(go))?;
    Some(std::fs::canonicalize(&found).unwrap_or(found))
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

/// The first program of this name on `PATH` or in the folders named after it. The timer
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
/// once. What couldn't be shown is said again next time.
fn tell(report: &Report, told: &mut Told) {
    let news: Vec<&(String, String)> = report.waiting.iter().filter(|(key, _)| !told.knows(key)).collect();
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
        assert!(v("0.6.0-rc.2") > v("0.6.0-rc.1"));
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
        let build = std::env::temp_dir().join(format!("lumen-kind-{}/target/release", std::process::id()));
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("lumen"), "").unwrap();
        assert!(matches!(kind(&build.join("lumen")), Kind::Build));
        let bin = build.parent().unwrap().parent().unwrap().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("lumen"), "").unwrap();
        assert!(matches!(kind(&bin.join("lumen")), Kind::Ours(dir) if dir.ends_with("bin")));
        std::fs::remove_dir_all(build.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn the_last_lines_are_what_went_wrong() {
        let said = b"go: downloading a\ngo: downloading b\n\nerror one\nerror two\n";
        assert_eq!(last_lines(said), "go: downloading b / error one / error two");
    }
}
