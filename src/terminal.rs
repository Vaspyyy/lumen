//! One running terminal: the program, its PTY, and the grid it draws on.
//!
//! alacritty_terminal does the terminal emulation. An I/O thread
//! ([`crate::pty`]) reads the PTY and updates the grid behind a lock; what
//! the window needs to know about (new output, a title, a clipboard
//! request, the exit) comes over a channel into this entity.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};
use alacritty_terminal::vte::ansi::{CursorShape as VteCursorShape, CursorStyle, Rgb as VteRgb};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{ClipboardItem, Context, EventEmitter, Task};

use crate::commands;
use crate::config::{Config, CursorShape};
use crate::graphics;
use crate::palette;
use crate::pty::{IoLoop, Msg, Sender};

#[derive(Clone)]
pub struct Listener(UnboundedSender<TermEvent>);

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        let _ = self.0.unbounded_send(event);
    }
}

/// The grid's size in cells, and a cell's size in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TermSize {
    pub cols: usize,
    pub rows: usize,
    pub cell_width: f32,
    pub cell_height: f32,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

impl From<TermSize> for WindowSize {
    fn from(s: TermSize) -> Self {
        WindowSize {
            num_lines: s.rows as u16,
            num_cols: s.cols as u16,
            // whole pixels are all a program can be told: the nearest ones
            cell_width: s.cell_width.round() as u16,
            cell_height: s.cell_height.round() as u16,
        }
    }
}

pub enum TerminalEvent {
    /// The title or the working directory changed.
    Title,
    Bell,
    /// A command was sent off, or one ended.
    Command(CommandEvent),
    /// A program has something to tell the user.
    Notify(commands::Note),
    /// The program ended.
    Exited,
}

/// What happened to a command, as far as the window cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandEvent {
    Started,
    Ended {
        /// It ended with an error of its own.
        failed: bool,
    },
}

pub struct Terminal {
    pub term: Arc<FairMutex<Term<Listener>>>,
    /// The images the program has put on screen.
    pub images: graphics::Shared,
    /// The commands the shell has told of.
    pub commands: commands::Shared,
    /// The command that was running, and how many had ended and failed,
    /// when that was last looked at.
    commands_seen: (Option<u32>, (u64, u64)),
    io: Sender,
    pub size: TermSize,
    title: Option<String>,
    shell_pid: u32,
    /// The environment of the launch it was started for, if not lumen's own.
    pub env: Option<Env>,
    pub exited: bool,
    _events: Task<()>,
}

impl EventEmitter<TerminalEvent> for Terminal {}

/// What a new terminal starts with.
#[derive(Clone, Default)]
pub struct Spawn {
    pub cwd: Option<PathBuf>,
    /// A program and its arguments, instead of the shell.
    pub command: Option<Vec<String>>,
    /// The environment of the launch it is for, in place of lumen's own: see
    /// [`crate::instance`]. None for lumen's own environment.
    pub env: Option<Env>,
}

/// An environment, sorted by name. Shared: every terminal of a window gets the same one.
pub type Env = std::sync::Arc<Vec<(String, String)>>;

/// What the PTY sets in a terminal's environment itself, whatever lumen's or the launch's
/// has: lumen's own variables, and what alacritty_terminal adds.
const SET_HERE: &[&str] =
    &["TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION", "USER", "HOME", "WINDOWID", "ALACRITTY_WINDOW_ID"];

/// The program a terminal runs. For a launch's terminal, its shell is the launch's
/// `$SHELL`, not lumen's (a daemon has the one systemd gave it), and what lumen's own
/// environment has that the launch's doesn't is taken out with `env -u`: the PTY can only
/// add to the environment it inherits from lumen.
fn program(spawn: &Spawn, config: &Config, ours: impl Iterator<Item = String>) -> std::io::Result<Option<Shell>> {
    let (program, args) = match &spawn.command {
        Some(argv) if !argv.is_empty() => (Some(argv[0].clone()), argv[1..].to_vec()),
        _ => (config.shell.clone(), Vec::new()),
    };
    let Some(env) = &spawn.env else { return Ok(program.map(|program| Shell::new(program, args))) };
    let theirs = |name: &str| env.iter().find(|(key, _)| key == name).map(|(_, value)| value);
    let program = program.or_else(|| theirs("SHELL").filter(|shell| !shell.is_empty()).cloned());
    // a launch without SHELL runs lumen's, and keeps it named
    let unset: Vec<String> =
        ours.filter(|name| theirs(name).is_none() && name != "SHELL" && !SET_HERE.contains(&name.as_str())).collect();
    if unset.is_empty() {
        return Ok(program.map(|program| Shell::new(program, args)));
    }
    let program = program.or_else(|| std::env::var("SHELL").ok()).unwrap_or_else(|| "/bin/sh".into());
    // env takes NAME=VALUE for a variable, also after --: such a program starts without it
    if program.contains('=') {
        return Ok(Some(Shell::new(program, args)));
    }
    // env would say in the tab that the program isn't there, and the tab would close at
    // once: the launch is told instead, as for a terminal started without env
    if !runnable(&program, theirs("PATH").map(String::as_str), spawn.cwd.as_deref()) {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("there is no program {program}")));
    }
    let mut wrapped: Vec<String> = unset.into_iter().flat_map(|name| ["-u".to_string(), name]).collect();
    wrapped.push("--".into());
    wrapped.push(program);
    wrapped.extend(args);
    Ok(Some(Shell::new("/usr/bin/env".into(), wrapped)))
}

/// Whether a program can be started, as `env` looks for it from the terminal's folder
/// `cwd`: by its path where it has a slash, else in the folders of `path`, where a
/// relative folder is one from `cwd` and an empty one is `cwd` itself.
fn runnable(program: &str, path: Option<&str>, cwd: Option<&std::path::Path>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    let executable =
        |file: &Path| file.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0);
    // from the terminal's folder, not lumen's
    let from_cwd = |file: &Path| match cwd {
        Some(cwd) if file.is_relative() => cwd.join(file),
        _ => file.to_path_buf(),
    };
    if program.contains('/') {
        return executable(&from_cwd(Path::new(program)));
    }
    path.unwrap_or("/usr/local/bin:/usr/bin:/bin")
        .split(':')
        .map(|dir| if dir.is_empty() { "." } else { dir })
        .any(|dir| executable(&from_cwd(&Path::new(dir).join(program))))
}

/// A terminal whose program is running, before it is an entity.
pub struct Started {
    term: Arc<FairMutex<Term<Listener>>>,
    images: graphics::Shared,
    commands: commands::Shared,
    io: Sender,
    size: TermSize,
    shell_pid: u32,
    env: Option<Env>,
    events: UnboundedReceiver<TermEvent>,
}

impl Terminal {
    /// Starts the program on a new PTY. This is the step that can fail.
    ///
    /// `size` is the grid the pane is expected to have. It is corrected when
    /// the pane is first drawn, but a program that prints before that has
    /// its output laid out for this size.
    pub fn start(spawn: Spawn, config: &Config, size: TermSize) -> std::io::Result<Started> {
        let shell = program(&spawn, config, std::env::vars_os().filter_map(|(name, _)| name.into_string().ok()))?;
        let launch_env = spawn.env.clone();
        let mut env: HashMap<String, String> = spawn.env.iter().flat_map(|env| env.iter().cloned()).collect();
        env.insert("TERM".into(), "xterm-256color".into());
        env.insert("COLORTERM".into(), "truecolor".into());
        env.insert("TERM_PROGRAM".into(), "lumen".into());
        env.insert("TERM_PROGRAM_VERSION".into(), env!("CARGO_PKG_VERSION").into());
        let options = PtyOptions { shell, working_directory: spawn.cwd, drain_on_exit: true, env };
        let pty = tty::new(&options, size.into(), 0)?;
        let shell_pid = pty.child().id();

        let (tx, rx) = unbounded();
        let listener = Listener(tx);
        let term = Arc::new(FairMutex::new(Term::new(term_options(config), &size, listener.clone())));
        let images = graphics::Shared::default();
        let commands = commands::Shared::new(commands::Log::new(shell_pid).into());
        let cell = (size.cell_width, size.cell_height);
        let io_loop = IoLoop::new(term.clone(), listener, pty, images.clone(), commands.clone(), cell)?;
        let io = io_loop.sender();
        io_loop.spawn();
        Ok(Started { term, images, commands, io, size, shell_pid, env: launch_env, events: rx })
    }

    pub fn new(started: Started, cx: &mut Context<Self>) -> Self {
        let Started { term, images, commands, io, size, shell_pid, env, events: mut rx } = started;
        let events = cx.spawn(async move |this, cx| {
            while let Some(first) = rx.next().await {
                // output arrives in bursts: take what is there, redraw once
                let mut batch = vec![first];
                while let Ok(event) = rx.try_recv() {
                    batch.push(event);
                }
                let alive = this.update(cx, |this, cx| {
                    for event in batch {
                        this.handle(event, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });

        Terminal {
            term,
            images,
            commands,
            commands_seen: (None, (0, 0)),
            io,
            size,
            title: None,
            shell_pid,
            env,
            exited: false,
            _events: events,
        }
    }

    /// Tells of the commands that were sent off or ended since this last looked.
    fn notice_commands(&mut self, cx: &mut Context<Self>) {
        let (running, ended, notes) = {
            let mut log = self.commands.lock().unwrap();
            let notes = log.take_notes();
            (log.running().map(|(id, _)| id), log.ended(), notes)
        };
        for note in notes {
            cx.emit(TerminalEvent::Notify(note));
        }
        let (was_running, had_ended) = std::mem::replace(&mut self.commands_seen, (running, ended));
        if ended.0 != had_ended.0 {
            // of several that ended since the last look, one that failed is the news
            cx.emit(TerminalEvent::Command(CommandEvent::Ended { failed: ended.1 != had_ended.1 }));
        }
        if running.is_some() && running != was_running {
            cx.emit(TerminalEvent::Command(CommandEvent::Started));
        }
    }

    fn handle(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Wakeup => {
                crate::lag::woke();
                self.notice_commands(cx);
                cx.notify();
            },
            TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange => cx.notify(),
            TermEvent::Title(title) => {
                self.title = Some(title);
                cx.emit(TerminalEvent::Title);
            },
            TermEvent::ResetTitle => {
                self.title = None;
                cx.emit(TerminalEvent::Title);
            },
            TermEvent::ClipboardStore(_, text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            TermEvent::ClipboardLoad(_, format) => {
                let text = cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default();
                self.write(format(&text).into_bytes());
            },
            TermEvent::ColorRequest(index, format) => {
                let color = self.term.lock().colors()[index];
                let color = color.unwrap_or_else(|| {
                    let rgb = palette::indexed(index, &cx.global::<Config>().colors);
                    VteRgb { r: rgb.0, g: rgb.1, b: rgb.2 }
                });
                self.write(format(color).into_bytes());
            },
            TermEvent::PtyWrite(text) => {
                // Asked what it is, the engine answers as a VT102 ("?6c"). Programs that
                // wait for that answer (kitty's icat does, to know its other questions
                // are through) take only a longer one: a VT220 with color.
                let text = if text == "\x1b[?6c" { "\x1b[?62;22c".to_string() } else { text };
                self.write(text.into_bytes())
            },
            TermEvent::TextAreaSizeRequest(format) => self.write(format(self.size.into()).into_bytes()),
            TermEvent::Bell => cx.emit(TerminalEvent::Bell),
            TermEvent::Exit | TermEvent::ChildExit(_) => {
                if !self.exited {
                    self.exited = true;
                    cx.emit(TerminalEvent::Exited);
                }
            },
        }
    }

    /// Sends bytes to the program, as if typed.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let bytes = bytes.into();
        if !bytes.is_empty() {
            self.io.send(Msg::Input(bytes));
        }
    }

    /// Takes on what a changed config says about the terminal itself.
    pub fn configure(&self, config: &Config) {
        self.term.lock().set_options(term_options(config));
    }

    pub fn resize(&mut self, size: TermSize) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.io.send(Msg::Resize(size.into(), (size.cell_width, size.cell_height)));
        commands::resize(&self.commands, &mut self.term.lock(), size);
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn scroll(&self, scroll: Scroll) {
        self.term.lock().scroll_display(scroll);
    }

    /// The title the program set, or the folder the shell is in.
    pub fn title(&self) -> String {
        if let Some(title) = self.title.as_ref().filter(|t| !t.is_empty()) {
            return title.clone();
        }
        match self.cwd() {
            Some(dir) => {
                let home = dirs::home_dir();
                if home.as_deref() == Some(dir.as_path()) {
                    "~".into()
                } else {
                    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into())
                }
            },
            None => "lumen".into(),
        }
    }

    /// The process the pane started: the shell, or the program run in its place.
    pub fn shell_pid(&self) -> u32 {
        self.shell_pid
    }

    /// What is typed at the prompt that is open, up to the cursor: see
    /// [`commands::typing`].
    pub fn typing(&self) -> Option<String> {
        let term = self.term.lock();
        let log = self.commands.lock().unwrap();
        commands::typing(&log, &term)
    }

    /// The folder the shell is in: where a new tab or split next to this
    /// one starts.
    pub fn cwd(&self) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.shell_pid)).ok()
    }

    pub fn start_selection(&self, ty: SelectionType, point: GridPoint, side: Side) {
        let mut term = self.term.lock();
        term.selection = Some(Selection::new(ty, point, side));
    }

    pub fn update_selection(&self, point: GridPoint, side: Side) {
        let mut term = self.term.lock();
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string().filter(|s| !s.is_empty())
    }

    /// The grid point under a position given in cells from the top left of
    /// what is shown, which may be scrolled back.
    pub fn grid_point(&self, col: usize, row: usize) -> GridPoint {
        let term = self.term.lock();
        let offset = term.grid().display_offset() as i32;
        let col = col.min(self.size.cols.saturating_sub(1));
        let row = row.min(self.size.rows.saturating_sub(1));
        GridPoint::new(Line(row as i32 - offset), Column(col))
    }
}

fn term_options(config: &Config) -> TermConfig {
    TermConfig {
        scrolling_history: config.scrollback,
        default_cursor_style: CursorStyle {
            shape: match config.cursor.style {
                CursorShape::Block => VteCursorShape::Block,
                CursorShape::Beam => VteCursorShape::Beam,
                CursorShape::Underline => VteCursorShape::Underline,
            },
            blinking: config.cursor.blink,
        },
        // programs may ask for keys in the kitty keyboard protocol
        kitty_keyboard: true,
        ..TermConfig::default()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.io.send(Msg::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ours() -> impl Iterator<Item = String> {
        ["PATH", "HOME", "INVOCATION_ID", "TERM", "VIRTUAL_ENV"].into_iter().map(String::from)
    }

    fn env(vars: &[(&str, &str)]) -> Option<Env> {
        Some(Env::new(vars.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect()))
    }

    #[test]
    fn lumens_own_terminals_run_as_asked() {
        let config = Config::default();
        assert_eq!(program(&Spawn::default(), &config, ours()).unwrap(), None);
        let spawn = Spawn { command: Some(vec!["htop".into(), "-d".into(), "5".into()]), ..Default::default() };
        let htop = Shell::new("htop".into(), vec!["-d".into(), "5".into()]);
        assert_eq!(program(&spawn, &config, ours()).unwrap(), Some(htop));
    }

    #[test]
    fn a_launch_gets_its_own_shell_and_nothing_of_lumens_environment() {
        let config = Config::default();
        let spawn = Spawn { env: env(&[("PATH", "/usr/bin:/bin"), ("SHELL", "/bin/sh")]), ..Default::default() };
        let unset = |names: &[&str], program: &str| {
            let mut args: Vec<String> = names.iter().flat_map(|name| ["-u".to_string(), name.to_string()]).collect();
            args.extend(["--".to_string(), program.to_string()]);
            Some(Shell::new("/usr/bin/env".into(), args))
        };
        // HOME and TERM are set by the PTY itself; the rest of lumen's own is taken out
        assert_eq!(program(&spawn, &config, ours()).unwrap(), unset(&["INVOCATION_ID", "VIRTUAL_ENV"], "/bin/sh"));
        // nothing to take out: the launch's shell, as it is
        let only = ["PATH".to_string()].into_iter();
        assert_eq!(program(&spawn, &config, only).unwrap(), Some(Shell::new("/bin/sh".into(), Vec::new())));
    }

    #[test]
    fn a_launch_is_told_of_a_program_that_is_not_there() {
        let config = Config::default();
        let missing = |program: &str| Spawn {
            command: Some(vec![program.into()]),
            env: env(&[("PATH", "/usr/bin:/bin")]),
            ..Default::default()
        };
        let error = program(&missing("no-such-program-here"), &config, ours()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(program(&missing("/nowhere/sh"), &config, ours()).is_err());
        assert!(program(&missing("sh"), &config, ours()).unwrap().is_some());
    }

    #[test]
    fn a_relative_folder_on_the_path_is_one_from_the_terminals_folder() {
        use std::os::unix::fs::PermissionsExt;
        let cwd = std::env::temp_dir().join(format!("lumen-runnable-{}", std::process::id()));
        std::fs::create_dir_all(cwd.join("bin")).unwrap();
        let tool = cwd.join("bin/lumen-test-tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(runnable("lumen-test-tool", Some("bin"), Some(&cwd)));
        assert!(runnable("bin/lumen-test-tool", Some(""), Some(&cwd)));
        assert!(runnable("lumen-test-tool", Some("/nowhere::"), Some(&cwd.join("bin"))));
        assert!(!runnable("lumen-test-tool", Some("bin"), Some(std::path::Path::new("/"))));
        std::fs::remove_dir_all(&cwd).unwrap();
    }
}
