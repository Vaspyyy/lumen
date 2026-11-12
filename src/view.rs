//! A terminal on screen: its keyboard, mouse, selection, clipboard, links
//! and search.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};
use gpui::prelude::*;
use gpui::{
    App, Bounds, BoxShadow, ClickEvent, ClipboardItem, Context, Entity, EntityInputHandler, EventEmitter,
    ExternalDragPayload, ExternalPaths, FileDragPaths, FocusHandle, Focusable, KeyContext, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollDelta, ScrollEvent, SharedString, Subscription, Task, UTF16Selection, Window, div, point, px, svg,
};

use crate::actions::{
    Copy, CopyOutput, NextCommand, Paste, PreviousCommand, ScrollBottom, ScrollPageDown, ScrollPageUp, ScrollTop,
    ToggleSearch, ZoomIn, ZoomOut, ZoomReset,
};
use crate::commands;
use crate::config::Config;
use crate::element::{Geometry, MarkSpot, Motion, TerminalElement};
use crate::keys::Press;
use crate::links::{self, Target};
use crate::pulse::{self, Pulse};
use crate::terminal::{CommandEvent, Listener, Started, Terminal, TerminalEvent};
use crate::{keys, mouse};

pub enum ViewEvent {
    /// The program ended: the pane can go.
    Closed,
    /// The title changed.
    Title,
    /// The program printed something.
    Output,
    /// The pane was clicked or otherwise took the keyboard.
    Focused,
    /// The program rang the bell.
    Bell,
    /// A command was sent off, or one ended.
    Command(CommandEvent),
    /// Something to tell the user in passing: a file was copied.
    Notice(String),
    /// A line in a file is to be opened in the editor.
    Edit { path: PathBuf, line: u32, column: Option<u32> },
    /// The program has something to tell the user.
    Notify(commands::Note),
}

/// A line of the screen, as it is read for what its text names: taken while
/// the terminal is locked, and looked at after.
struct Seen {
    /// Each character with the column it stands in, and whether it is a wide one.
    cells: Vec<(usize, char, bool)>,
    /// The character under the mouse.
    at: usize,
    /// The folder the command that printed the line ran in, where the shell told of it.
    folder: Option<PathBuf>,
}

impl Seen {
    /// The line with the character at `col`. None where there is nothing but space.
    fn at(term: &Term<Listener>, terminal: &Terminal, line: Line, col: usize) -> Option<Seen> {
        let cells: Vec<(usize, char, bool)> = term.grid()[line]
            .into_iter()
            .enumerate()
            .filter(|(_, cell)| !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER))
            .map(|(col, cell)| (col, cell.c, cell.flags.contains(Flags::WIDE_CHAR)))
            .collect();
        let at = cells.iter().rposition(|&(cell_col, ..)| cell_col <= col)?;
        if cells[at].1.is_whitespace() {
            return None;
        }
        // The folder is asked for only where the text may name a file in it: every
        // line up to the command's prompt is read for it, with the terminal locked.
        let text: Vec<char> = cells.iter().map(|&(_, c, _)| c).collect();
        let mut wanted = false;
        let _ = links::in_text(&text, at, |name| {
            wanted = !name.starts_with(['/', '~']);
            None
        });
        let log = terminal.commands.lock().unwrap();
        // A shell that marks no commands has none to ask, and all of the scrollback
        // would be searched for one.
        let printed_by = if !wanted || log.is_empty() {
            None
        } else {
            commands::place_at(term, line).or_else(|| commands::above(term, line, |_| true))
        };
        let folder = printed_by.and_then(|place| log.get(place.id)?.cwd.clone());
        Some(Seen { cells, at, folder })
    }
}

/// What the disk last said about a path.
struct Found {
    name: String,
    folder: Option<PathBuf>,
    path: Option<PathBuf>,
    at: Instant,
}

/// A link the mouse is on.
#[derive(Clone, PartialEq)]
pub struct Hover {
    pub target: Target,
    /// The cells it covers on screen: row, column and length of each
    /// stretch. A link that wraps has more than one.
    pub segments: Vec<(usize, usize, usize)>,
}

/// A search through the scrollback.
pub struct Search {
    pub query: String,
    /// Oldest first.
    pub matches: Rc<[Match]>,
    /// The match the view is on.
    pub current: Option<usize>,
    /// How many lines were above the screen when the matches were found:
    /// what tells how far they have moved up since.
    history: usize,
}

/// More matches than this are not kept: the newest are.
const MAX_MATCHES: usize = 5000;

/// How long a pane has had the keyboard, and its window been in front,
/// before a click on a command for the prompt is taken for one.
const AWAKE: Duration = Duration::from_millis(300);

/// A file being dragged out of a listing.
#[derive(Clone)]
pub struct DraggedFile {
    pub path: PathBuf,
    /// The pane it is dragged from, and where its name is there.
    from: gpui::EntityId,
    name_at: Vec<Bounds<Pixels>>,
}

/// What is under the mouse while a file is dragged.
struct FileGhost(SharedString);

impl Render for FileGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = cx.global::<Config>();
        div()
            .px(px(10.))
            .py(px(5.))
            .rounded(px(8.))
            .bg(config.colors.primary.background.hsla().opacity(0.92))
            .border_1()
            .border_color(config.accent().opacity(0.6))
            .text_size(px(12.5))
            .text_color(gpui::white().opacity(0.92))
            .font_family(crate::fonts::ui())
            .child(self.0.clone())
    }
}

pub struct TerminalView {
    pub terminal: Entity<Terminal>,
    pub focus: FocusHandle,
    geometry: Rc<Cell<Option<Geometry>>>,
    zoom: f32,
    /// A selection is being dragged out.
    selecting: bool,
    /// The cell the last mouse report was for, so a move within a cell says nothing.
    reported_cell: Option<(usize, usize)>,
    /// Wheel movement that hasn't added up to a line yet.
    scroll_rest: f32,
    last_input: Instant,
    blink_on: bool,
    /// When the blink last changed halves: the cursor fades from there.
    blink_at: Instant,
    motion: Rc<Cell<Motion>>,
    /// Text an input method is still composing.
    composing: Option<String>,
    /// Where the mouse is, while it is over this pane.
    mouse: Option<Point<Pixels>>,
    modifiers: Modifiers,
    hover: Option<Hover>,
    /// When the pane last got the keyboard, or its window came to the front.
    awake: Instant,
    /// The mouse went down on a link in a pane that was already being worked in.
    armed: bool,
    /// The path that was last looked up for a link in plain text.
    found: RefCell<Option<Found>>,
    /// Where the marks of the commands on screen are, as of the last frame.
    marks: Rc<RefCell<Vec<MarkSpot>>>,
    /// The command whose mark the mouse is on.
    hovered_mark: Option<u32>,
    /// The keys whose press went to the program, and whose release may too.
    keys_down: Vec<String>,
    search: Option<Search>,
    /// The wait before the matches are looked for again after new output.
    search_refresh: Option<Task<()>>,
    _blink: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ViewEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TerminalView {
    pub fn new(started: Started, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let terminal = cx.new(|cx| Terminal::new(started, cx));
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.observe(&terminal, |this, _, cx| {
                // what is under the mouse, and what the search finds, may have changed
                this.update_hover(cx);
                this.refresh_search_soon(cx);
                cx.emit(ViewEvent::Output);
                cx.notify();
            }),
            cx.subscribe(&terminal, |_, _, event: &TerminalEvent, cx| match event {
                TerminalEvent::Title => cx.emit(ViewEvent::Title),
                TerminalEvent::Exited => cx.emit(ViewEvent::Closed),
                TerminalEvent::Bell => cx.emit(ViewEvent::Bell),
                TerminalEvent::Command(event) => cx.emit(ViewEvent::Command(*event)),
                TerminalEvent::Notify(note) => cx.emit(ViewEvent::Notify(note.clone())),
            }),
            cx.on_focus(&focus, window, |this, _, cx| {
                this.touch();
                this.awake = Instant::now();
                this.report_focus(true, cx);
                cx.emit(ViewEvent::Focused);
                cx.notify();
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.awake = Instant::now();
                }
                // a command for the prompt is a link only in the window that is worked in
                cx.notify();
            }),
            cx.on_blur(&focus, window, |this, _, cx| {
                this.report_focus(false, cx);
                cx.notify();
            }),
            cx.observe_global::<Config>(|this, cx| {
                this.terminal.read(cx).configure(cx.global::<Config>());
                cx.notify();
            }),
        ];
        let blink = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(530)).await;
                let alive = this.update(cx, |this, cx| {
                    if !cx.global::<Config>().cursor.blink {
                        return;
                    }
                    // typing holds the cursor lit
                    let on = !this.blink_on || this.last_input.elapsed() < Duration::from_millis(500);
                    if on != this.blink_on {
                        this.blink_on = on;
                        this.blink_at = Instant::now();
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        TerminalView {
            terminal,
            focus,
            geometry: Rc::new(Cell::new(None)),
            zoom: 1.,
            selecting: false,
            reported_cell: None,
            scroll_rest: 0.,
            last_input: Instant::now(),
            blink_on: true,
            blink_at: Instant::now() - Duration::from_secs(1),
            motion: Rc::default(),
            // for a screenshot of composed text, where there is no input method to compose it
            composing: std::env::var("LUMEN_COMPOSING").ok().filter(|_| crate::preview()),
            mouse: None,
            modifiers: Modifiers::default(),
            hover: None,
            awake: Instant::now(),
            armed: false,
            found: RefCell::new(None),
            marks: Rc::default(),
            hovered_mark: None,
            keys_down: Vec::new(),
            search: None,
            search_refresh: None,
            _blink: blink,
            _subscriptions: subscriptions,
        }
    }

    fn touch(&mut self) {
        self.last_input = Instant::now();
        if !self.blink_on {
            self.blink_on = true;
            self.blink_at = Instant::now();
        }
    }

    fn mode(&self, cx: &App) -> TermMode {
        self.terminal.read(cx).mode()
    }

    fn report_focus(&self, focused: bool, cx: &App) {
        if self.mode(cx).contains(TermMode::FOCUS_IN_OUT) {
            self.terminal.read(cx).write(if focused { &b"\x1b[I"[..] } else { &b"\x1b[O"[..] });
        }
    }

    /// Sends what was typed, and shows the prompt it went to.
    fn send(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let terminal = self.terminal.read(cx);
        terminal.clear_selection();
        terminal.scroll(Scroll::Bottom);
        terminal.write(bytes);
        self.touch();
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.composing.is_some() {
            return;
        }
        if self.search.is_some() {
            if self.search_key(&event.keystroke, cx) {
                cx.stop_propagation();
            }
            return;
        }
        // The press is the program's from here on, as bytes now or as text through
        // the input method in a moment: so is the release.
        if !self.keys_down.contains(&event.keystroke.key) {
            self.keys_down.push(event.keystroke.key.clone());
        }
        let press = if event.is_held { Press::Repeat } else { Press::Down };
        if let Some(bytes) = keys::encode(&event.keystroke, self.mode(cx), press) {
            self.send(bytes, cx);
            cx.stop_propagation();
        }
    }

    /// A key let go: only a program that asked to hear of it is told, and
    /// only of a key whose press it got (not one that was a shortcut of
    /// lumen's, or went to the search).
    fn key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.keys_down.iter().position(|key| *key == event.keystroke.key) else {
            return;
        };
        self.keys_down.swap_remove(at);
        if self.composing.is_some() || self.search.is_some() {
            return;
        }
        let mode = self.mode(cx);
        if mode.contains(TermMode::REPORT_EVENT_TYPES)
            && let Some(bytes) = keys::encode(&event.keystroke, mode, Press::Up)
        {
            self.terminal.read(cx).write(bytes);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.terminal.read(cx).selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if let Some(search) = &mut self.search {
            search.query.push_str(text.lines().next().unwrap_or_default());
            self.search_changed(cx);
        } else {
            self.paste_text(&text, cx);
        }
    }

    fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let bytes = if self.mode(cx).contains(TermMode::BRACKETED_PASTE) {
            // the end marker inside the text would end the paste early and run the rest
            let text = text.replace('\x1b', "");
            format!("\x1b[200~{text}\x1b[201~").into_bytes()
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
        };
        self.send(bytes, cx);
    }

    /// Files dropped on the pane are typed as their paths.
    fn drop_paths(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let words: Vec<String> = paths.iter().map(|path| links::shell_quote(path)).collect();
        if !words.is_empty() {
            window.focus(&self.focus, cx);
            self.paste_text(&(words.join(" ") + " "), cx);
        }
    }

    fn scroll_by(&mut self, scroll: Scroll, cx: &mut Context<Self>) {
        self.terminal.read(cx).scroll(scroll);
        self.update_hover(cx);
        cx.notify();
    }

    fn zoom(&mut self, factor: Option<f32>, cx: &mut Context<Self>) {
        self.zoom = match factor {
            Some(f) => (self.zoom * f).clamp(0.4, 4.),
            None => 1.,
        };
        cx.notify();
    }

    /// The link under the mouse, if there is one lumen can follow.
    fn link_under_mouse(&self, cx: &App) -> Option<Hover> {
        let (position, geometry) = (self.mouse?, self.geometry.get()?);
        let grid = Bounds::new(
            geometry.origin,
            gpui::size(geometry.cell.width * geometry.cols as f32, geometry.cell.height * geometry.rows as f32),
        );
        if !grid.contains(&position) {
            return None;
        }
        let (col, row, _) = geometry.cell_at(position);
        let terminal = self.terminal.read(cx);
        let term = terminal.term.lock();
        let (cols, rows) = (term.columns(), term.screen_lines());
        if col >= cols || row >= rows {
            return None;
        }
        let offset = term.grid().display_offset() as i32;
        let link_at = |row: usize, col: usize| term.grid()[Line(row as i32 - offset)][Column(col)].hyperlink();
        // a link the program marked, or else what its plain text names
        let Some((target, link)) = link_at(row, col).and_then(|link| Some((links::target(link.uri())?, link))) else {
            // What plain text names is looked for only while Ctrl is held, which is when a
            // click would follow it: a file is looked up on disk. That is done without
            // the terminal's lock, which the program's output waits for.
            let seen =
                if self.modifiers.control { Seen::at(&term, terminal, Line(row as i32 - offset), col) } else { None };
            drop(term);
            return self.text_link(seen?, terminal, row);
        };
        // the cells around `col` that carry the same link
        let stretch = |row: usize, col: usize| {
            let (mut start, mut end) = (col, col + 1);
            while start > 0 && link_at(row, start - 1).as_ref() == Some(&link) {
                start -= 1;
            }
            while end < cols && link_at(row, end).as_ref() == Some(&link) {
                end += 1;
            }
            (start, end)
        };
        let (start, end) = stretch(row, col);
        let mut segments = vec![(row, start, end - start)];
        // a link that wraps goes on in the rows above and below
        let (mut above, mut first) = (row, start);
        while above > 0 && first == 0 && link_at(above - 1, cols - 1).as_ref() == Some(&link) {
            above -= 1;
            let (start, end) = stretch(above, cols - 1);
            segments.push((above, start, end - start));
            first = start;
        }
        let (mut below, mut last) = (row, end);
        while below + 1 < rows && last == cols && link_at(below + 1, 0).as_ref() == Some(&link) {
            below += 1;
            let (start, end) = stretch(below, 0);
            segments.push((below, start, end - start));
            last = end;
        }
        // A click copies what the text names. A link that says one thing and leads to
        // another file is not taken at its word: it is a link like any other.
        segments.sort();
        let mut text = String::new();
        for &(row, start, len) in &segments {
            for cell in &term.grid()[Line(row as i32 - offset)][Column(start)..Column(start + len)] {
                // the second half of a wide character is not a character of its own
                if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(cell.c);
                text.extend(cell.zerowidth().into_iter().flatten());
            }
        }
        let target = match target {
            Target::Copy(path) if !names(&text, &path) => Target::File(path),
            target => target,
        };
        // A command for the prompt is typed by a plain click. So its link is one only on
        // its text: a program can't lay it over the empty parts of the screen, where a
        // click means something else.
        if matches!(target, Target::Prompt(_)) {
            let shows = |col: usize| {
                let cells = &term.grid()[Line(row as i32 - offset)];
                // the second half of a wide character shows what the first half does
                let second = cells[Column(col)].flags.contains(Flags::WIDE_CHAR_SPACER) && col > 0;
                let cell = &cells[Column(if second { col - 1 } else { col })];
                cell.hyperlink().as_ref() == Some(&link)
                    && !cell.flags.contains(Flags::HIDDEN)
                    && cell.fg != cell.bg
                    && !links::blank(cell.c)
            };
            // one space between two words of it is part of it
            let between = col > 0 && col + 1 < cols && shows(col - 1) && shows(col + 1);
            if !shows(col) && !between {
                return None;
            }
        }
        Some(Hover { target, segments })
    }

    /// What the plain text of a line names where the mouse is: a web address,
    /// or a file that is there.
    fn text_link(&self, seen: Seen, terminal: &Terminal, row: usize) -> Option<Hover> {
        let text: Vec<char> = seen.cells.iter().map(|&(_, c, _)| c).collect();
        let folder = seen.folder;
        let (target, part) = links::in_text(&text, seen.at, |name| {
            // a path is meant from where the command that printed it was run, or else from where the shell is now
            let folder = folder.or_else(|| terminal.cwd());
            self.find_file(name, folder.as_deref())
        })?;
        let (first, (last, _, wide)) = (seen.cells[part.start].0, seen.cells[part.end - 1]);
        Some(Hover { target, segments: vec![(row, first, last + 1 + wide as usize - first)] })
    }

    /// Where a path as a program printed it leads. The answer is kept for a
    /// moment: the question comes with every move of the mouse over the same
    /// word, and each time it is the disk that is asked.
    fn find_file(&self, name: &str, folder: Option<&std::path::Path>) -> Option<PathBuf> {
        const FRESH: Duration = Duration::from_millis(1500);
        let mut found = self.found.borrow_mut();
        if let Some(found) = found.as_ref()
            && found.name == name
            && found.folder.as_deref() == folder
            && found.at.elapsed() < FRESH
        {
            return found.path.clone();
        }
        let path = links::find_file(name, folder);
        *found = Some(Found {
            name: name.to_string(),
            folder: folder.map(PathBuf::from),
            path: path.clone(),
            at: Instant::now(),
        });
        path
    }

    fn update_hover(&mut self, cx: &mut Context<Self>) {
        let hover = self.link_under_mouse(cx);
        if hover != self.hover {
            self.hover = hover;
            cx.notify();
        }
    }

    /// The hovered link, if a click would follow it now: a file from a
    /// listing always, a command for the prompt in the pane that is worked
    /// in (`focused`), anything else with Ctrl held. Shift leaves the text to
    /// be selected, and a program that uses the mouse gets the click.
    fn live_link(&self, focused: bool, cx: &App) -> Option<&Hover> {
        let hover = self.hover.as_ref()?;
        let wanted = match hover.target {
            Target::Copy(_) => true,
            Target::Prompt(_) => focused,
            _ => self.modifiers.control,
        };
        let free = !self.modifiers.shift && !self.selecting && !self.mode(cx).intersects(TermMode::MOUSE_MODE);
        (wanted && free).then_some(hover)
    }

    /// A click on the hovered link: a listed file is copied, or opened with
    /// Ctrl held; a command is put at the prompt; everything else is opened.
    fn follow(&mut self, click: &ClickEvent, cx: &mut Context<Self>) {
        let armed = std::mem::take(&mut self.armed);
        let Some(hover) = self.hover.clone() else {
            return;
        };
        match (&hover.target, click.modifiers().control) {
            (Target::Copy(path), false) => self.copy_file(path, cx),
            (Target::Copy(path) | Target::File(path), _) => cx.open_with_system(path),
            (Target::Place { path, line, column }, _) => {
                cx.emit(ViewEvent::Edit { path: path.clone(), line: *line, column: *column })
            },
            (Target::Prompt(text), _) => {
                // Typed once, however often the mouse is clicked. Not by the click that
                // brought the pane or the window to the front, and only if the link is
                // still what the mouse is on: the screen may have moved under it.
                let still = || self.link_under_mouse(cx).is_some_and(|now| now.target == hover.target);
                if armed && click.click_count() <= 1 && still() {
                    self.put_at_prompt(text, cx);
                }
            },
            (Target::Url(url), _) => cx.open_url(url),
        }
    }

    /// Puts a command at the shell's prompt, as a link asked for. It is not
    /// run: Enter is the user's. Where lumen knows that no prompt is waiting
    /// (a full-screen program has the screen, or the shell says a command
    /// runs), nothing is typed: the text would go to that program.
    fn put_at_prompt(&mut self, text: &str, cx: &mut Context<Self>) {
        let terminal = self.terminal.read(cx);
        let full_screen = terminal.mode().contains(TermMode::ALT_SCREEN);
        let running = terminal.commands.lock().unwrap().running().is_some();
        if full_screen || running {
            cx.emit(ViewEvent::Notice(format!("A program runs here, so this was not typed: {text}")));
            return;
        }
        self.paste_text(text, cx);
    }

    /// Puts a file on the clipboard, and says so.
    fn copy_file(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        let name = path.file_name().map_or_else(|| "/".into(), |n| n.to_string_lossy().into_owned());
        // a file from somewhere else than where the shell is, is named with its folder
        let here = self.terminal.read(cx).cwd();
        let folder =
            path.parent().filter(|folder| here.as_deref() != Some(*folder)).map(|folder| {
                match dirs::home_dir().and_then(|home| folder.strip_prefix(home).ok().map(|rest| rest.to_path_buf())) {
                    Some(rest) => format!(" from ~/{}", rest.display()),
                    None => format!(" from {}", folder.display()),
                }
            });
        let notice = if !path.exists() {
            format!("{name} is gone")
        } else {
            match links::copy_files(std::slice::from_ref(path)) {
                Ok(()) => format!("Copied {name}{}", folder.unwrap_or_default()),
                Err(e) => e,
            }
        };
        cx.emit(ViewEvent::Notice(notice));
    }

    /// The command whose mark is at this position.
    fn mark_at(&self, position: Point<Pixels>) -> Option<MarkSpot> {
        self.marks.borrow().iter().find(|spot| spot.bounds.contains(&position)).copied()
    }

    /// Scrolls to the command before or after the one the view starts with:
    /// its prompt becomes the first line.
    fn jump(&mut self, forward: bool, cx: &mut Context<Self>) {
        let terminal = self.terminal.read(cx);
        {
            let mut term = terminal.term.lock();
            let log = terminal.commands.lock().unwrap();
            let sent_off = |id: u32| log.get(id).is_some_and(|command| command.started.is_some());
            let offset = term.grid().display_offset() as i32;
            let top = Line(-offset);
            let target = if forward {
                let from = commands::place_at(&*term, top).map_or(top, |place| place.last);
                commands::below(&*term, from, sent_off)
            } else {
                commands::above(&*term, top, sent_off)
            };
            let wanted = match target {
                // a command that is on the screen as it stands needs no scrolling back
                Some(place) => (-place.first.0).max(0),
                None if forward => 0,
                None => return,
            };
            term.scroll_display(Scroll::Delta(wanted - offset));
        }
        self.update_hover(cx);
        cx.notify();
    }

    /// Copies what a command printed, and selects it: that shows what was copied.
    fn copy_output_of(
        &mut self,
        find: impl FnOnce(&Term<Listener>, &commands::Log) -> Option<commands::Place>,
        cx: &mut Context<Self>,
    ) {
        let terminal = self.terminal.read(cx);
        let found = {
            let mut term = terminal.term.lock();
            let log = terminal.commands.lock().unwrap();
            find(&term, &log).map(|place| {
                let name = log.get(place.id).and_then(|command| command.name());
                let copied = commands::output(&*term, place).map(|lines| {
                    let mut selection =
                        Selection::new(SelectionType::Lines, GridPoint::new(lines.0, Column(0)), Side::Left);
                    selection.update(GridPoint::new(lines.1, term.last_column()), Side::Right);
                    term.selection = Some(selection);
                    (commands::text(&*term, lines), (lines.1.0 - lines.0.0) as usize + 1)
                });
                (name, copied)
            })
        };
        let notice = match found {
            None => "No command here: the shell has to mark its commands for this".to_string(),
            Some((name, None)) => match name {
                Some(name) => format!("{name} printed nothing"),
                None => "That command printed nothing".to_string(),
            },
            Some((name, Some((text, lines)))) => {
                cx.write_to_primary(ClipboardItem::new_string(text.clone()));
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                let lines = if lines == 1 { "1 line".to_string() } else { format!("{lines} lines") };
                match name {
                    Some(name) => format!("Copied the output of {name}: {lines}"),
                    None => format!("Copied {lines} of output"),
                }
            },
        };
        cx.emit(ViewEvent::Notice(notice));
        cx.notify();
    }

    /// The output of the command the view starts with, when it is scrolled
    /// back; of the last command otherwise.
    fn copy_output(&mut self, _: &CopyOutput, _: &mut Window, cx: &mut Context<Self>) {
        self.copy_output_of(
            |term, log| {
                let sent_off = |id: u32| log.get(id).is_some_and(|command| command.started.is_some());
                let offset = term.grid().display_offset() as i32;
                if offset == 0 {
                    return commands::above(term, term.bottommost_line() + 1, sent_off);
                }
                let top = Line(-offset);
                commands::place_at(term, top)
                    .filter(|place| sent_off(place.id))
                    .or_else(|| commands::above(term, top, sent_off))
            },
            cx,
        );
    }

    /// What is said about a command while the mouse is on its mark.
    fn render_mark(&self, spot: MarkSpot, geometry: Geometry, cx: &App) -> Option<impl IntoElement + use<>> {
        let config = cx.global::<Config>();
        let terminal = self.terminal.read(cx);
        let (mut about, running) = {
            let log = terminal.commands.lock().unwrap();
            let command = log.get(spot.id)?;
            let mut about = command.describe()?;
            if let Some(peak) = command.peak() {
                about += &format!(" · up to {}", pulse::bytes(peak));
            }
            (about, command.status() == commands::Status::Running)
        };
        // what it costs right now, where pulse tells
        let now = cx.try_global::<Pulse>().and_then(|pulse| pulse.0.read(cx).of(terminal.shell_pid()));
        if let Some(now) = now.filter(|_| running) {
            about += &format!(" · {}", now.work());
        }
        const HEIGHT: f32 = 26.;
        // above the command's lines, or below them where there is no room above
        let above = spot.bounds.top() - geometry.bounds.origin.y - px(HEIGHT + 4.);
        let top = if above >= px(0.) { above } else { spot.bounds.bottom() - geometry.bounds.origin.y + px(4.) };
        Some(
            chip(config)
                .left(geometry.origin.x - geometry.bounds.origin.x)
                .top(top)
                .h(px(HEIGHT))
                .child(SharedString::from(about))
                .child(div().text_color(gpui::white().opacity(0.45)).child("click copies the output")),
        )
    }

    /// What a click on a link for the prompt would type: the link's text
    /// need not say it. All of it is shown, over as many lines as it takes.
    fn render_prompt_hint(&self, hover: &Hover, geometry: Geometry, cx: &App) -> Option<impl IntoElement + use<>> {
        let Target::Prompt(text) = &hover.target else {
            return None;
        };
        let &(row, col, len) = hover.segments.first()?;
        const NOTE: &str = "click puts it at the prompt";
        const EDGE: Pixels = px(8.);
        const LINE: f32 = 18.;
        let config = cx.global::<Config>();
        let pane = geometry.bounds.size;
        let left = geometry.origin.x + geometry.cell.width * col as f32 - geometry.bounds.origin.x;
        let line = geometry.origin.y + geometry.cell.height * row as f32 - geometry.bounds.origin.y;
        let end = left + geometry.cell.width * len as f32;
        // About how wide the note is on one line: its letters are as wide as they are
        // in the widest of the fonts it may be set in.
        let letters = |count: usize| px(count as f32 * 7.8);
        let (command, note) = (letters(text.chars().count()), letters(NOTE.len()));
        let one_line = command + note + px(30.);
        let widest = (pane.width - EDGE * 2.).max(px(120.));
        let command_shown = div().font_family(config.font.family.clone()).child(SharedString::from(text.clone()));
        let note_shown = div().text_color(gpui::white().opacity(0.45)).child(NOTE);
        let (hint, height) = if one_line <= widest {
            let hint = chip(config).h(px(26.)).child(command_shown).child(note_shown);
            // it begins where the link does, or ends where it ends, or else lies as far left as it must
            let hint = if left + one_line <= pane.width - EDGE {
                hint.left(left)
            } else if end - one_line >= EDGE {
                hint.right(pane.width - end)
            } else {
                hint.left(EDGE)
            };
            (hint, px(26.))
        } else {
            // the text takes the pane's width and the lines it needs, the note stands under it
            let lines = (command / (widest - px(22.))).ceil().max(1.) + 1.;
            let hint = chip(config)
                .flex_col()
                .items_start()
                .gap(px(2.))
                .py(px(5.))
                .left(EDGE)
                .w(widest)
                .line_height(px(LINE))
                .child(command_shown.w_full())
                .child(note_shown);
            (hint, px(lines * LINE + 12.))
        };
        // above the link, or below it where there is more room
        let (above, below) = (line - px(4.), pane.height - line - geometry.cell.height - px(4.));
        Some(if above >= height || above >= below {
            hint.bottom(pane.height - above)
        } else {
            hint.top(line + geometry.cell.height + px(4.))
        })
    }

    fn toggle_search(&mut self, _: &ToggleSearch, _: &mut Window, cx: &mut Context<Self>) {
        self.search = match self.search {
            Some(_) => None,
            None => Some(Search { query: String::new(), matches: Rc::new([]), current: None, history: 0 }),
        };
        cx.notify();
    }

    /// A key while the search is open. True when the key was the search's.
    fn search_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        let m = key.modifiers;
        let Some(search) = &mut self.search else {
            return false;
        };
        match key.key.as_str() {
            "escape" => {
                self.search = None;
                cx.notify();
            },
            // Enter goes back in time, as the newest match is where a search starts
            "enter" | "up" | "down" => {
                let older = (key.key == "enter" && !m.shift) || key.key == "up";
                self.search_step(if older { -1 } else { 1 }, cx);
            },
            "backspace" => {
                if m.control || m.alt {
                    search.query.clear();
                } else {
                    search.query.pop();
                }
                self.search_changed(cx);
            },
            "space" if !m.control && !m.alt => {
                search.query.push(' ');
                self.search_changed(cx);
            },
            // text arrives through the input method; nothing else goes to the program meanwhile
            _ => return m.control || m.alt || key.key.chars().count() > 1,
        }
        true
    }

    /// Looks for the query again and goes to the newest match.
    fn search_changed(&mut self, cx: &mut Context<Self>) {
        let Some(search) = &mut self.search else {
            return;
        };
        let terminal = self.terminal.read(cx);
        let term = terminal.term.lock();
        search.matches = find(&term, &search.query).into();
        search.current = search.matches.len().checked_sub(1);
        search.history = term.history_size();
        drop(term);
        self.show_match(cx);
    }

    /// After new output: the same search, staying on the match it was on.
    fn refresh_search_soon(&mut self, cx: &mut Context<Self>) {
        if self.search.as_ref().is_none_or(|search| search.query.is_empty()) || self.search_refresh.is_some() {
            return;
        }
        self.search_refresh = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            let _ = this.update(cx, |this, cx| {
                this.search_refresh = None;
                let Some(search) = &mut this.search else {
                    return;
                };
                let (matches, history) = {
                    let terminal = this.terminal.read(cx);
                    let term = terminal.term.lock();
                    (find(&term, &search.query), term.history_size())
                };
                // The match the search is on is the same text, further up by the lines
                // printed since. Once the history is full that count is lost: then it
                // is the match with the same number.
                let moved = history as i32 - search.history as i32;
                let on = search.current.and_then(|i| search.matches.get(i).cloned());
                search.current = on
                    .and_then(|on| {
                        let (line, column) = (on.start().line.0 - moved, on.start().column);
                        matches.iter().position(|m| m.start().line.0 == line && m.start().column == column)
                    })
                    .or(search.current.map(|i| i.min(matches.len().saturating_sub(1))))
                    .filter(|_| !matches.is_empty());
                search.history = history;
                search.matches = matches.into();
                cx.notify();
            });
        }));
    }

    fn search_step(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(search) = &mut self.search else {
            return;
        };
        let count = search.matches.len() as isize;
        if count == 0 {
            return;
        }
        let at = search.current.map_or(count - 1, |i| i as isize + by);
        search.current = Some(at.rem_euclid(count) as usize);
        self.show_match(cx);
    }

    /// Scrolls so that the match the search is on can be seen.
    fn show_match(&mut self, cx: &mut Context<Self>) {
        if let Some(search) = &self.search
            && let Some(found) = search.current.and_then(|i| search.matches.get(i))
        {
            self.terminal.read(cx).term.lock().scroll_to_point(*found.start());
        }
        cx.notify();
    }

    pub fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let (col, row, right_half) = geometry.cell_at(event.position);
        let mode = self.mode(cx);
        if mode.intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            if let Some(button) = mouse::button_code(event.button) {
                self.reported_cell = Some((col, row));
                self.terminal.read(cx).write(mouse::report(button, true, col, row, event.modifiers, mode));
            }
            return;
        }
        // a click on a command's mark is for the command, not for the text beside it
        if event.button == MouseButton::Left
            && let Some(spot) = self.mark_at(event.position)
        {
            self.copy_output_of(
                |term, _| {
                    let line = Line(spot.row as i32 - term.grid().display_offset() as i32);
                    commands::place_at(term, line).filter(|place| place.id == spot.id)
                },
                cx,
            );
            return;
        }
        match event.button {
            MouseButton::Left => {
                let ty = match event.click_count {
                    0 | 1 => SelectionType::Simple,
                    2 => SelectionType::Semantic,
                    _ => SelectionType::Lines,
                };
                let ty = if event.modifiers.control && event.click_count <= 1 { SelectionType::Block } else { ty };
                let side = if right_half { Side::Right } else { Side::Left };
                let terminal = self.terminal.read(cx);
                terminal.start_selection(ty, terminal.grid_point(col, row), side);
                self.selecting = true;
                cx.notify();
            },
            MouseButton::Middle => {
                if let Some(text) = cx.read_from_primary().and_then(|item| item.text()) {
                    self.paste_text(&text, cx);
                }
            },
            _ => {},
        }
    }

    pub fn mouse_move(&mut self, event: &MouseMoveEvent, hovered: bool, _: &mut Window, cx: &mut Context<Self>) {
        self.mouse = hovered.then_some(event.position);
        if self.modifiers != event.modifiers {
            // Ctrl makes a link a link: what is drawn depends on it
            self.modifiers = event.modifiers;
            cx.notify();
        }
        self.update_hover(cx);
        let mark = if hovered && !self.selecting { self.mark_at(event.position).map(|spot| spot.id) } else { None };
        if mark != self.hovered_mark {
            self.hovered_mark = mark;
            cx.notify();
        }
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let (col, row, right_half) = geometry.cell_at(event.position);
        if self.selecting {
            let side = if right_half { Side::Right } else { Side::Left };
            let terminal = self.terminal.read(cx);
            terminal.update_selection(terminal.grid_point(col, row), side);
            cx.notify();
            return;
        }
        let mode = self.mode(cx);
        let wanted = mode.contains(TermMode::MOUSE_MOTION)
            || (mode.contains(TermMode::MOUSE_DRAG) && event.pressed_button.is_some());
        if hovered && wanted && !event.modifiers.shift && self.reported_cell != Some((col, row)) {
            self.reported_cell = Some((col, row));
            let button = event.pressed_button.and_then(mouse::button_code).unwrap_or(mouse::NO_BUTTON);
            self.terminal.read(cx).write(mouse::report(button + mouse::MOTION, true, col, row, event.modifiers, mode));
        }
    }

    pub fn mouse_up(&mut self, event: &MouseUpEvent, hovered: bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            if event.button == MouseButton::Left {
                self.selecting = false;
                if let Some(text) = self.terminal.read(cx).selection_text() {
                    cx.write_to_primary(ClipboardItem::new_string(text.clone()));
                    if cx.global::<Config>().copy_on_select {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                } else {
                    // a click that selected nothing takes the old selection away
                    self.terminal.read(cx).clear_selection();
                }
                cx.notify();
            }
            return;
        }
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let mode = self.mode(cx);
        if hovered
            && mode.intersects(TermMode::MOUSE_MODE)
            && !event.modifiers.shift
            && let Some(button) = mouse::button_code(event.button)
        {
            let (col, row, _) = geometry.cell_at(event.position);
            self.terminal.read(cx).write(mouse::report(button, false, col, row, event.modifiers, mode));
        }
    }

    pub fn scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let lines = match event.delta {
            ScrollDelta::Pixels(delta) => delta.y / geometry.cell.height,
            // already counted in lines per wheel step by the platform
            ScrollDelta::Lines(delta) => delta.y,
        };
        self.scroll_rest += lines;
        let whole = self.scroll_rest.trunc();
        self.scroll_rest -= whole;
        let steps = whole as i32;
        if steps == 0 {
            return;
        }
        let mode = self.mode(cx);
        let terminal = self.terminal.read(cx);
        if mode.intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            let (col, row, _) = geometry.cell_at(event.position);
            let button = if steps > 0 { mouse::WHEEL_UP } else { mouse::WHEEL_DOWN };
            for _ in 0..steps.unsigned_abs() {
                terminal.write(mouse::report(button, true, col, row, event.modifiers, mode));
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !event.modifiers.shift {
            // a full-screen program without mouse support (less, man) scrolls by arrow keys
            let key: &[u8] = match (steps > 0, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            terminal.write(key.repeat(steps.unsigned_abs() as usize));
        } else {
            terminal.scroll(Scroll::Delta(steps));
        }
        self.update_hover(cx);
        cx.notify();
    }

    /// The hot spots over the hovered link: what takes the click, and what a
    /// file is dragged by.
    fn render_link(&self, hover: &Hover, geometry: Geometry, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let name_at = hover
            .segments
            .iter()
            .map(|&(row, col, len)| {
                let origin = point(
                    geometry.origin.x + geometry.cell.width * col as f32,
                    geometry.origin.y + geometry.cell.height * row as f32,
                );
                Bounds::new(origin, gpui::size(geometry.cell.width * len as f32, geometry.cell.height))
            })
            .collect();
        let file =
            hover.target.path().map(|path| DraggedFile { path: path.to_path_buf(), from: cx.entity_id(), name_at });
        hover
            .segments
            .iter()
            .enumerate()
            .map(|(n, &(row, col, len))| {
                let left = geometry.origin.x + geometry.cell.width * col as f32 - geometry.bounds.origin.x;
                let top = geometry.origin.y + geometry.cell.height * row as f32 - geometry.bounds.origin.y;
                let spot = div()
                    .id(("link", n))
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(geometry.cell.width * len as f32)
                    .h(geometry.cell.height)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            // a pane or a window that only just came to the front was clicked
                            // on to get there, whatever was under the mouse
                            let here = this.focus.is_focused(window) && (window.is_window_active() || crate::preview());
                            this.armed = here && this.awake.elapsed() >= AWAKE;
                            // the click is the link's: no selection starts under it
                            window.focus(&this.focus, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| this.follow(event, cx)));
                match &file {
                    Some(file) => {
                        let name = file.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        spot.on_drag(file.clone(), move |_, _, _, cx| cx.new(|_| FileGhost(name.clone().into())))
                            // out of the window, it is a file being dragged to another program
                            .external_drag_payload(|file: &DraggedFile, _, _| {
                                let entry = (file.path.clone(), file.path.is_dir());
                                Some(ExternalDragPayload::Files(FileDragPaths::new([entry])))
                            })
                            .into_any_element()
                    },
                    None => spot.into_any_element(),
                }
            })
            .collect()
    }

    fn render_search(&self, search: &Search, cx: &App) -> impl IntoElement + use<> {
        let config = cx.global::<Config>();
        let count = match (search.current, search.matches.len()) {
            (_, 0) if search.query.is_empty() => String::new(),
            (_, 0) => "no match".into(),
            (Some(at), found) if found >= MAX_MATCHES => format!("{} of {found}+", at + 1),
            (Some(at), found) => format!("{} of {found}", at + 1),
            (None, found) => found.to_string(),
        };
        let dim = gpui::white().opacity(0.45);
        div()
            .absolute()
            .top(px(8.))
            .right(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(30.))
            .px(px(10.))
            .rounded(px(9.))
            .bg(config.colors.primary.background.hsla().opacity(0.95))
            .border_1()
            .border_color(config.accent().opacity(0.45))
            .shadow(vec![BoxShadow {
                color: gpui::black().opacity(0.35),
                offset: point(px(0.), px(2.)),
                blur_radius: px(10.),
                spread_radius: px(0.),
                inset: false,
            }])
            .font_family(crate::fonts::ui())
            .text_size(px(12.5))
            .child(svg().path("icons/search.svg").flex_none().size(px(12.)).text_color(dim))
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w(px(110.))
                    .max_w(px(320.))
                    .overflow_hidden()
                    .text_color(gpui::white().opacity(0.92))
                    .child(SharedString::from(search.query.clone()))
                    .child(div().flex_none().w(px(1.5)).h(px(14.)).ml(px(1.)).bg(config.accent())),
            )
            .child(div().flex_none().text_color(dim).child(SharedString::from(count)))
    }
}

/// Whether the text of a link is the name of the file it leads to: all of
/// it, or its start where a listing cut a long name short.
fn names(text: &str, path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
        return false;
    };
    let text = text.trim();
    match text.strip_suffix('…') {
        Some(start) => start.chars().count() >= 3 && name.starts_with(start),
        None => text.contains(name.as_ref()),
    }
}

/// Every place `query` is found in the terminal's lines, oldest first. The
/// query is plain text; it ignores case unless it has a capital in it.
fn find<T>(term: &Term<T>, query: &str) -> Vec<Match> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut pattern = String::with_capacity(query.len() * 2);
    for c in query.chars() {
        // the characters a regex takes for more than themselves, and no others:
        // an escaped `<` would be the start of a word
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    let Ok(mut regex) = RegexSearch::new(&pattern) else {
        return Vec::new();
    };
    let top = GridPoint::new(term.topmost_line(), Column(0));
    let bottom = GridPoint::new(term.bottommost_line(), term.last_column());
    // from the end, so that when there are too many the newest are the ones kept
    let mut matches: Vec<Match> = Vec::new();
    for found in RegexIter::new(bottom, top, Direction::Left, term, &mut regex) {
        // going backwards, a query that overlaps itself (== in ====) is found again one cell on
        if matches.last().is_some_and(|newer| found.end() >= newer.start()) {
            continue;
        }
        matches.push(found);
        if matches.len() == MAX_MATCHES {
            break;
        }
    }
    matches.reverse();
    matches
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window) && (window.is_window_active() || crate::preview());
        let link = self.live_link(focused, cx).cloned();
        let spots = match (&link, self.geometry.get()) {
            (Some(hover), Some(geometry)) => self.render_link(hover, geometry, cx),
            _ => Vec::new(),
        };
        let search = self.search.as_ref().map(|search| self.render_search(search, cx));
        let hovered_mark =
            self.hovered_mark.and_then(|id| self.marks.borrow().iter().find(|spot| spot.id == id).copied());
        let about = match (hovered_mark, self.geometry.get()) {
            (Some(spot), Some(geometry)) => self.render_mark(spot, geometry, cx),
            _ => None,
        };
        let hint = match (&link, self.geometry.get()) {
            (Some(hover), Some(geometry)) => self.render_prompt_hint(hover, geometry, cx),
            _ => None,
        };
        let mut context = KeyContext::default();
        context.add("Terminal");
        // the keys for commands are lumen's only where the shell marks its commands
        if !self.terminal.read(cx).commands.lock().unwrap().is_empty() {
            context.add("marks");
        }
        if self.mode(cx).contains(TermMode::ALT_SCREEN) {
            context.add("alt_screen");
        }
        div()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .key_context(context)
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                // Ctrl makes a link of what plain text names: under a mouse that rests, too
                this.modifiers = event.modifiers;
                this.update_hover(cx);
                cx.notify();
            }))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::toggle_search))
            .on_action(cx.listener(Self::copy_output))
            .on_action(cx.listener(|this, _: &PreviousCommand, _, cx| this.jump(false, cx)))
            .on_action(cx.listener(|this, _: &NextCommand, _, cx| this.jump(true, cx)))
            .on_action(cx.listener(|this, _: &ScrollPageUp, _, cx| this.scroll_by(Scroll::PageUp, cx)))
            .on_action(cx.listener(|this, _: &ScrollPageDown, _, cx| this.scroll_by(Scroll::PageDown, cx)))
            .on_action(cx.listener(|this, _: &ScrollTop, _, cx| this.scroll_by(Scroll::Top, cx)))
            .on_action(cx.listener(|this, _: &ScrollBottom, _, cx| this.scroll_by(Scroll::Bottom, cx)))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom(Some(1.1), cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom(Some(1. / 1.1), cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.zoom(None, cx)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| this.drop_paths(paths.paths(), window, cx)))
            .on_drop(cx.listener(|this, file: &DraggedFile, window, cx| {
                // let go on the name it was picked up by, it was a click with an unsteady hand
                let position = window.mouse_position();
                if file.from == cx.entity_id() && file.name_at.iter().any(|at| at.contains(&position)) {
                    if matches!(this.hover.as_ref().map(|hover| &hover.target), Some(Target::Copy(_))) {
                        this.copy_file(&file.path, cx);
                    }
                } else {
                    this.drop_paths(std::slice::from_ref(&file.path), window, cx);
                }
            }))
            .child(TerminalElement {
                terminal: self.terminal.clone(),
                view: cx.entity(),
                focus: self.focus.clone(),
                geometry: self.geometry.clone(),
                focused,
                cursor_on: self.blink_on,
                blink_at: self.blink_at,
                motion: self.motion.clone(),
                zoom: self.zoom,
                link: link.map(|hover| hover.segments).unwrap_or_default(),
                search: self.search.as_ref().map(|search| (search.matches.clone(), search.current)),
                composing: self.composing.clone(),
                marks: self.marks.clone(),
                hovered_mark: self.hovered_mark,
            })
            .children(spots)
            .children(about)
            .children(hint)
            .children(search)
    }
}

/// A note that floats over the pane, about what the mouse is on.
fn chip(config: &Config) -> gpui::Div {
    div()
        .absolute()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .rounded(px(8.))
        .bg(config.colors.primary.background.hsla().opacity(0.95))
        .border_1()
        .border_color(gpui::white().opacity(0.14))
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(0.35),
            offset: point(px(0.), px(2.)),
            blur_radius: px(10.),
            spread_radius: px(0.),
            inset: false,
        }])
        .font_family(crate::fonts::ui())
        .text_size(px(12.5))
        .text_color(gpui::white().opacity(0.92))
}

/// Text input. The terminal has no text of its own to edit: what the input
/// method commits is sent to the program (or is the search's query), and
/// what it is still composing is shown at the cursor.
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.composing.as_ref().map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.composing = None;
        cx.notify();
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.composing = None;
        if text.is_empty() {
            cx.notify();
        } else if let Some(search) = &mut self.search {
            search.query.push_str(text);
            self.search_changed(cx);
        } else {
            self.send(text.as_bytes().to_vec(), cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        new_text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composing = (!new_text.is_empty()).then(|| new_text.to_string());
        self.touch();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // where the input method's popup goes: at the cursor
        let geometry = self.geometry.get()?;
        Some(geometry.cursor.unwrap_or(Bounds::new(geometry.origin, geometry.cell)))
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config as TermConfig;
    use alacritty_terminal::vte::ansi::Processor;

    use super::*;
    use crate::terminal::TermSize;

    /// A terminal of 40 by 6 cells that this was printed to.
    fn printed(text: &str) -> Term<VoidListener> {
        let size = TermSize { cols: 40, rows: 6, cell_width: 8., cell_height: 16. };
        let mut term = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, text.as_bytes());
        term
    }

    /// Where the matches start: line and column.
    fn found(term: &Term<VoidListener>, query: &str) -> Vec<(i32, usize)> {
        find(term, query).iter().map(|m| (m.start().line.0, m.start().column.0)).collect()
    }

    #[test]
    fn the_query_is_plain_text() {
        let term = printed("a -> b <tag> a.b axb\r\n(x) [y] 1+1 50% ~/x #1 a|b x&y\r\n");
        assert_eq!(found(&term, "->"), [(0, 2)]);
        assert_eq!(found(&term, "<tag>"), [(0, 7)]);
        assert_eq!(found(&term, "<"), [(0, 7)]);
        // a dot is a dot, not any character
        assert_eq!(found(&term, "a.b"), [(0, 13)]);
        assert_eq!(found(&term, "(x)"), [(1, 0)]);
        assert_eq!(found(&term, "[y]"), [(1, 4)]);
        assert_eq!(found(&term, "1+1"), [(1, 8)]);
        assert_eq!(found(&term, "50%"), [(1, 12)]);
        assert_eq!(found(&term, "~/x"), [(1, 16)]);
        assert_eq!(found(&term, "#1"), [(1, 20)]);
        assert_eq!(found(&term, "a|b"), [(1, 23)]);
        assert_eq!(found(&term, "x&y"), [(1, 27)]);
        assert_eq!(found(&term, "nothing"), []);
    }

    #[test]
    fn case_counts_only_when_the_query_has_a_capital() {
        let term = printed("Error error ERROR\r\n");
        assert_eq!(found(&term, "error").len(), 3);
        assert_eq!(found(&term, "Error"), [(0, 0)]);
    }

    #[test]
    fn matches_do_not_overlap() {
        let term = printed("======\r\naaa\r\n");
        assert_eq!(found(&term, "=="), [(0, 0), (0, 2), (0, 4)]);
        assert_eq!(found(&term, "aa").len(), 1);
    }

    #[test]
    fn older_lines_are_found_too() {
        let lines: String = (0..20).map(|i| format!("line {i}\r\n")).collect();
        let term = printed(&lines);
        assert_eq!(found(&term, "line").len(), 20);
        // 21 lines on a screen of 6: the first 15 are above it
        assert_eq!(found(&term, "line 3"), [(-12, 0)]);
    }

    #[test]
    fn a_link_is_copied_only_under_its_own_name() {
        let path = std::path::Path::new("/home/u/project/report-final.pdf");
        assert!(names("report-final.pdf", path));
        assert!(names(" report-final.pdf ", path));
        assert!(names("project/report-final.pdf", path));
        // a name that a listing cut short
        assert!(names("report-fi…", path));
        assert!(!names("r…", path));
        assert!(!names("click here", path));
        assert!(!names("notes.txt", path));
        assert!(!names("", path));
        assert!(!names("anything", std::path::Path::new("/")));
    }
}
