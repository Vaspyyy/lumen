//! What the machine is busy with, as pulse tells it.
//!
//! pulse is a program of its own. Where it is installed, lumen runs
//! `pulse watch --json` and reads a line every few seconds: whether the
//! machine is busy, and what each pane's shell and everything it started
//! use. Which shells to look at, lumen says by writing their process ids to
//! pulse's input, so one pulse serves every window.
//!
//! Without pulse none of this is shown, and nothing else is missing.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use gpui::{App, AppContext, Context, Entity, EntityId, Global, Subscription, Task};
use serde::Deserialize;

use crate::config::Config;

/// What a shell and everything it started use.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Use {
    pub pid: u32,
    /// There is no such process any more.
    pub gone: bool,
    /// CPUs' worth of work.
    pub cores: f32,
    /// Bytes.
    pub memory: u64,
    pub processes: u32,
    /// The heaviest thing the shell started, with all below it: what the pane runs.
    pub job: Option<String>,
    /// For several panes taken together: the work of the pane the job is from.
    #[serde(skip)]
    most: f32,
}

impl Use {
    /// Two panes' use as one tab's. The job is that of the pane that works most.
    pub fn with(mut self, other: &Use) -> Use {
        if other.job.is_some() && (self.job.is_none() || other.cores > self.most) {
            self.job = other.job.clone();
            self.most = other.cores;
        }
        self.cores += other.cores;
        self.memory += other.memory;
        self.processes += other.processes;
        self
    }

    /// Something works here, or holds memory worth a look.
    pub fn notable(&self) -> bool {
        self.cores >= 0.2 || self.memory >= 1 << 30
    }

    /// The work and the memory: `5.6 cores · 4.1 GB`.
    pub fn work(&self) -> String {
        format!("{} · {}", cores(self.cores), bytes(self.memory))
    }

    /// In a few words: `cargo build · 5.6 cores · 4.1 GB`.
    pub fn describe(&self) -> String {
        match &self.job {
            Some(job) => format!("{job} · {}", self.work()),
            None => self.work(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cpu {
    pub cores: f32,
    pub of: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Memory {
    pub used: u64,
    pub total: u64,
}

/// One line of `pulse watch --json`. pulse may add fields: what lumen
/// doesn't know, it leaves alone.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Reading {
    /// The version of the format: 1.
    pub v: u32,
    /// Too much is going on for a measurement to mean anything.
    pub busy: bool,
    pub reasons: Vec<String>,
    /// The verdict in one line, as pulse words it.
    pub line: String,
    pub cpu: Cpu,
    pub mem: Memory,
    pub pids: Vec<Use>,
}

impl Reading {
    /// What a line of pulse's output says. None for anything else: another
    /// program by the same name, or a format lumen doesn't know.
    pub fn parse(line: &str) -> Option<Reading> {
        let reading: Reading = serde_json::from_str(line).ok()?;
        (reading.v == 1 && reading.cpu.of > 0).then_some(reading)
    }

    /// The machine as a whole: `cpu 8.0 of 16 cores · memory 24.0 of 62.0 GB`.
    pub fn machine(&self) -> String {
        let gb = |bytes: u64| bytes as f64 / (1u64 << 30) as f64;
        format!(
            "cpu {:.1} of {} cores · memory {:.1} of {:.1} GB",
            self.cpu.cores,
            self.cpu.of,
            gb(self.mem.used),
            gb(self.mem.total)
        )
    }
}

/// CPUs' worth of work, as pulse words it.
pub fn cores(cores: f32) -> String {
    if (cores * 10.).round() == 10. { "1.0 core".to_string() } else { format!("{cores:.1} cores") }
}

/// A size, as pulse words it: `300 MB`, `4.1 GB`.
pub fn bytes(bytes: u64) -> String {
    const GB: u64 = 1 << 30;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{} MB", (bytes as f64 / (1 << 20) as f64).round())
    }
}

/// Where pulse is: in a folder of `path` (the `PATH` lumen was started
/// with), or where `go install` and install scripts put a program. A lumen
/// started from the desktop's menu may not have been told of those folders,
/// though every shell in it has.
fn find(path: Option<&std::ffi::OsStr>, home: Option<&Path>) -> Option<PathBuf> {
    let named: Vec<PathBuf> = path.map(|path| std::env::split_paths(path).collect()).unwrap_or_default();
    let usual = home.into_iter().flat_map(|home| [home.join("go/bin"), home.join(".local/bin")]);
    named.into_iter().chain(usual).map(|folder| folder.join("pulse")).find(|program| {
        std::fs::metadata(program).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
}

/// The line that tells pulse which processes to report on from now on.
fn asking(pids: &[u32]) -> String {
    let mut line = "pids".to_string();
    for pid in pids {
        line.push(' ');
        line.push_str(&pid.to_string());
    }
    line.push('\n');
    line
}

/// A pulse that is running, and its input.
struct Watch {
    child: Child,
    input: ChildStdin,
    _lines: Task<()>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// After this many starts that led to nothing, pulse is left alone.
const TRIES: u32 = 3;
/// The wait before pulse is started again after it ended.
const AGAIN: Duration = Duration::from_secs(20);

/// The one pulse of this lumen, and what it said last.
pub struct Monitor {
    reading: Option<Reading>,
    uses: HashMap<u32, Use>,
    /// The shells each window wants to know about.
    wanted: HashMap<EntityId, Vec<u32>>,
    /// What pulse was last asked about.
    asked: Vec<u32>,
    /// The program, where it was found.
    program: Option<PathBuf>,
    watch: Option<Watch>,
    /// Starts in a row that ended without a reading.
    failed: u32,
    again: Option<Task<()>>,
    /// What the config said about pulse when this last looked.
    config: Option<crate::config::Pulse>,
    _config: Subscription,
}

/// Where the monitor is found.
pub struct Pulse(pub Entity<Monitor>);

impl Global for Pulse {}

/// Starts pulse, if the config wants it and it is there.
pub fn init(cx: &mut App) {
    let monitor = cx.new(|cx| {
        let mut monitor = Monitor {
            reading: None,
            uses: HashMap::new(),
            wanted: HashMap::new(),
            asked: Vec::new(),
            program: None,
            watch: None,
            failed: 0,
            again: None,
            config: None,
            _config: cx.observe_global::<Config>(Monitor::config_changed),
        };
        monitor.config_changed(cx);
        monitor
    });
    cx.set_global(Pulse(monitor));
}

impl Monitor {
    /// What pulse said last, if it runs.
    pub fn reading(&self) -> Option<&Reading> {
        self.reading.as_ref()
    }

    /// The pulse that is read from: the one to start for the monitor itself.
    pub fn program(&self) -> Option<&Path> {
        self.program.as_deref()
    }

    /// What a shell and everything it started use, as of the last reading.
    pub fn of(&self, pid: u32) -> Option<&Use> {
        self.uses.get(&pid).filter(|found| !found.gone)
    }

    /// A window says which shells it has. pulse is asked about those of all windows.
    pub fn want(&mut self, window: EntityId, pids: Vec<u32>) {
        self.wanted.insert(window, pids);
        self.ask();
    }

    /// A window is gone.
    pub fn forget(&mut self, window: EntityId) {
        self.wanted.remove(&window);
        self.ask();
    }

    fn ask(&mut self) {
        let mut pids: Vec<u32> = self.wanted.values().flatten().copied().collect();
        pids.sort_unstable();
        pids.dedup();
        if pids == self.asked {
            return;
        }
        if let Some(watch) = &mut self.watch {
            // if pulse has ended, its output ending says so
            let _ = watch.input.write_all(asking(&pids).as_bytes()).and_then(|()| watch.input.flush());
        }
        self.asked = pids;
    }

    fn enabled(&self) -> bool {
        self.config.as_ref().is_some_and(|config| config.enabled)
    }

    fn config_changed(&mut self, cx: &mut Context<Self>) {
        let config = cx.global::<Config>().pulse.clone();
        if self.config.as_ref() == Some(&config) {
            return;
        }
        // it is started anew: with another wait between its readings, or not at all
        self.config = Some(config);
        self.failed = 0;
        self.again = None;
        self.watch = None;
        self.reading = None;
        self.uses.clear();
        if self.enabled() {
            self.start(cx);
        }
        cx.notify();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let every = self.config.as_ref().map_or(3, |config| config.every);
        self.program = find(std::env::var_os("PATH").as_deref(), dirs::home_dir().as_deref());
        let spawned = self.program.as_ref().map(|program| {
            Command::new(program)
                .args(["watch", "--json", "--every", &every.to_string()])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        });
        // no pulse on this machine: nothing to show, and nothing to say about it
        let Some(Ok(mut child)) = spawned else {
            self.failed = TRIES;
            return;
        };
        let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return;
        };
        let _ = input.write_all(asking(&self.asked).as_bytes()).and_then(|()| input.flush());
        let (tx, mut rx) = unbounded();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if tx.unbounded_send(line).is_err() {
                    break;
                }
            }
        });
        let lines = cx.spawn(async move |this, cx| {
            while let Some(line) = rx.next().await {
                if this.update(cx, |this, cx| this.take(&line, cx)).is_err() {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| this.ended(cx));
        });
        self.watch = Some(Watch { child, input, _lines: lines });
    }

    /// A line pulse printed.
    fn take(&mut self, line: &str, cx: &mut Context<Self>) {
        let Some(reading) = Reading::parse(line) else {
            // not the pulse lumen knows: it is not asked again
            self.failed = TRIES;
            self.watch = None;
            self.reading = None;
            self.uses.clear();
            cx.notify();
            return;
        };
        self.failed = 0;
        self.uses = reading.pids.iter().map(|found| (found.pid, found.clone())).collect();
        self.reading = Some(reading);
        cx.notify();
    }

    /// pulse's output ended: it is gone.
    fn ended(&mut self, cx: &mut Context<Self>) {
        if self.watch.take().is_none() {
            return;
        }
        self.reading = None;
        self.uses.clear();
        self.failed += 1;
        if self.enabled() && self.failed < TRIES {
            self.again = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(AGAIN).await;
                let _ = this.update(cx, |this, cx| {
                    if this.enabled() && this.watch.is_none() {
                        this.start(cx);
                    }
                });
            }));
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"v":1,"at":"2026-11-11T03:30:00+01:00","window_ms":3000,"busy":true,"reasons":["a game (Hades) uses 68% GPU, 2.1 cores and 3.7 GB"],"line":"busy: a game (Hades) uses 68% GPU, 2.1 cores and 3.7 GB","games":["Hades"],"cpu":{"cores":8.04,"of":16,"temp":61.25},"mem":{"used":25769803776,"total":66571993088,"pressure":0},"gpus":[{"name":"RTX 3070","util":71}],"pids":[{"pid":4821,"handle":"4821.kfx","cores":5.56,"memory":4393533440,"processes":10,"job":"cargo build","top":"rustc syn"},{"pid":5133,"gone":true,"cores":0,"memory":0,"processes":0}]}"#;

    #[test]
    fn a_line_of_pulse() {
        let reading = Reading::parse(LINE).unwrap();
        assert!(reading.busy);
        assert_eq!(reading.line, "busy: a game (Hades) uses 68% GPU, 2.1 cores and 3.7 GB");
        assert_eq!(reading.machine(), "cpu 8.0 of 16 cores · memory 24.0 of 62.0 GB");
        assert_eq!(reading.pids.len(), 2);
        assert_eq!(reading.pids[0].describe(), "cargo build · 5.6 cores · 4.1 GB");
        assert!(reading.pids[0].notable());
        assert!(reading.pids[1].gone);
    }

    #[test]
    fn what_is_not_pulse_is_not_read() {
        assert_eq!(Reading::parse(""), None);
        assert_eq!(Reading::parse("pulse: unknown command watch"), None);
        assert_eq!(Reading::parse("{}"), None);
        // a format from later, whose fields may mean something else
        assert_eq!(Reading::parse(&LINE.replace(r#""v":1"#, r#""v":2"#)), None);
        // fields lumen has never heard of are no reason to stop
        assert!(Reading::parse(&LINE.replace(r#""v":1"#, r#""v":1,"new":{"a":[1]}"#)).is_some());
    }

    #[test]
    fn sizes_and_work_in_words() {
        assert_eq!(bytes(43_012_096), "41 MB");
        assert_eq!(bytes(1 << 30), "1.0 GB");
        assert_eq!(bytes(9_556_302_233), "8.9 GB");
        assert_eq!(cores(0.), "0.0 cores");
        assert_eq!(cores(1.04), "1.0 core");
        assert_eq!(cores(5.56), "5.6 cores");
        let idle = Use { memory: 43_012_096, ..Use::default() };
        assert_eq!(idle.describe(), "0.0 cores · 41 MB");
        assert!(!idle.notable());
    }

    #[test]
    fn a_tab_is_its_panes_together() {
        let build =
            Use { cores: 5.5, memory: 4 << 30, processes: 10, job: Some("cargo build".into()), ..Use::default() };
        let idle = Use { cores: 0., memory: 1 << 29, processes: 1, ..Use::default() };
        let serve =
            Use { cores: 0.5, memory: 1 << 30, processes: 3, job: Some("npm run dev".into()), ..Use::default() };
        let tab = Use::default().with(&idle).with(&serve).with(&build);
        assert_eq!(tab.describe(), "cargo build · 6.0 cores · 5.5 GB");
        assert_eq!(tab.processes, 14);
        // the job of the pane that works most, in whichever order they come
        assert_eq!(Use::default().with(&build).with(&serve).job.as_deref(), Some("cargo build"));
        let test = Use { cores: 4., job: Some("cargo test".into()), ..Use::default() };
        let lint = Use { cores: 3., job: Some("cargo clippy".into()), ..Use::default() };
        assert_eq!(Use::default().with(&test).with(&lint).with(&build).job.as_deref(), Some("cargo build"));
    }

    #[test]
    fn pulse_is_found_where_programs_are_put() {
        let scratch = std::env::temp_dir().join(format!("lumen-pulse-{}", std::process::id()));
        let (bin, home) = (scratch.join("bin"), scratch.join("home"));
        let put = |folder: &Path, mode: u32| {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::write(folder.join("pulse"), "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(folder.join("pulse"), std::fs::Permissions::from_mode(mode)).unwrap();
        };
        let path = std::env::join_paths([scratch.join("empty"), bin.clone()]).unwrap();
        assert_eq!(find(Some(&path), Some(&home)), None);
        // where `go install` puts it, for a lumen whose PATH doesn't have that folder
        put(&home.join("go/bin"), 0o755);
        assert_eq!(find(Some(&path), Some(&home)), Some(home.join("go/bin/pulse")));
        assert_eq!(find(None, Some(&home)), Some(home.join("go/bin/pulse")));
        // a file that can't be run is not it; one on the PATH comes first
        put(&bin, 0o644);
        assert_eq!(find(Some(&path), Some(&home)), Some(home.join("go/bin/pulse")));
        put(&bin, 0o755);
        assert_eq!(find(Some(&path), Some(&home)), Some(bin.join("pulse")));
        assert_eq!(find(None, None), None);
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn pulse_is_asked_in_its_own_words() {
        assert_eq!(asking(&[4821, 5133]), "pids 4821 5133\n");
        assert_eq!(asking(&[]), "pids\n");
    }
}
