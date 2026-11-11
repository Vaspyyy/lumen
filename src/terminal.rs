//! One running terminal: the program, its PTY, and the grid it draws on.
//!
//! alacritty_terminal does the terminal emulation. Its I/O thread reads the
//! PTY and updates the grid behind a lock; what the window needs to know
//! about (new output, a title, a clipboard request, the exit) comes over a
//! channel into this entity.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::event::{Event as TermEvent, EventListener, Notify, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
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

use crate::config::{Config, CursorShape};
use crate::palette;

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
            cell_width: s.cell_width as u16,
            cell_height: s.cell_height as u16,
        }
    }
}

pub enum TerminalEvent {
    /// The title or the working directory changed.
    Title,
    Bell,
    /// The program ended.
    Exited,
}

pub struct Terminal {
    pub term: Arc<FairMutex<Term<Listener>>>,
    notifier: Notifier,
    pub size: TermSize,
    title: Option<String>,
    shell_pid: u32,
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
}

/// A terminal whose program is running, before it is an entity.
pub struct Started {
    term: Arc<FairMutex<Term<Listener>>>,
    notifier: Notifier,
    size: TermSize,
    shell_pid: u32,
    events: UnboundedReceiver<TermEvent>,
}

impl Terminal {
    /// Starts the program on a new PTY. This is the step that can fail.
    ///
    /// `size` is the grid the pane is expected to have. It is corrected when
    /// the pane is first drawn, but a program that prints before that has
    /// its output laid out for this size.
    pub fn start(spawn: Spawn, config: &Config, size: TermSize) -> std::io::Result<Started> {
        let shell = match spawn.command {
            Some(mut argv) if !argv.is_empty() => Some(Shell::new(argv.remove(0), argv)),
            _ => config.shell.clone().map(|program| Shell::new(program, Vec::new())),
        };
        let mut env = HashMap::new();
        env.insert("TERM".into(), "xterm-256color".into());
        env.insert("COLORTERM".into(), "truecolor".into());
        env.insert("TERM_PROGRAM".into(), "lumen".into());
        env.insert("TERM_PROGRAM_VERSION".into(), env!("CARGO_PKG_VERSION").into());
        let options = PtyOptions { shell, working_directory: spawn.cwd, drain_on_exit: true, env };
        let pty = tty::new(&options, size.into(), 0)?;
        let shell_pid = pty.child().id();

        let (tx, rx) = unbounded();
        let listener = Listener(tx);
        let term_config = TermConfig {
            scrolling_history: config.scrollback,
            default_cursor_style: CursorStyle {
                shape: match config.cursor.style {
                    CursorShape::Block => VteCursorShape::Block,
                    CursorShape::Beam => VteCursorShape::Beam,
                    CursorShape::Underline => VteCursorShape::Underline,
                },
                blinking: config.cursor.blink,
            },
            ..TermConfig::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(term_config, &size, listener.clone())));
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false)?;
        let notifier = Notifier(event_loop.channel());
        let _io_thread = event_loop.spawn();
        Ok(Started { term, notifier, size, shell_pid, events: rx })
    }

    pub fn new(started: Started, cx: &mut Context<Self>) -> Self {
        let Started { term, notifier, size, shell_pid, events: mut rx } = started;
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

        Terminal { term, notifier, size, title: None, shell_pid, exited: false, _events: events }
    }

    fn handle(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Wakeup | TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange => cx.notify(),
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
            TermEvent::PtyWrite(text) => self.write(text.into_bytes()),
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
        self.notifier.notify(bytes);
    }

    pub fn resize(&mut self, size: TermSize) {
        if size == self.size {
            return;
        }
        self.size = size;
        let _ = self.notifier.0.send(Msg::Resize(size.into()));
        self.term.lock().resize(size);
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

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}
