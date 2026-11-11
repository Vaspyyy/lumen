//! One terminal's screen: what a program printed, as a grid, and what the
//! keys and touches of a phone are in the bytes a program expects.
//!
//! alacritty_terminal does the terminal emulation, as on the desktop. This
//! knows nothing of where the bytes come from or how the grid is drawn: the
//! app asks for a [`Screen::snapshot`] and draws that.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor};

use crate::palette;

/// Lines kept above the screen.
const SCROLLBACK: usize = 10_000;

/// The bytes of a snapshot's header.
pub const HEADER: usize = 48;
/// The bytes of one cell in a snapshot.
pub const CELL: usize = 16;

/// Nothing there: a cursor that is not shown, a selection end out of view.
const NOWHERE: u16 = 0xffff;

// what a cell's `flags` in a snapshot say
const BOLD: u32 = 1;
const ITALIC: u32 = 1 << 1;
const UNDERLINE: u32 = 1 << 2;
const DOUBLE_UNDERLINE: u32 = 1 << 3;
const UNDERCURL: u32 = 1 << 4;
const DOTTED_UNDERLINE: u32 = 1 << 5;
const DASHED_UNDERLINE: u32 = 1 << 6;
const STRIKEOUT: u32 = 1 << 7;
const WIDE: u32 = 1 << 8;
const SPACER: u32 = 1 << 9;
const SELECTED: u32 = 1 << 10;
const LINK: u32 = 1 << 11;
const EXTRAS: u32 = 1 << 12;

// what the header's `mode` says
const MODE_ALT_SCREEN: u16 = 1;
const MODE_MOUSE: u16 = 1 << 1;
const MODE_APP_CURSOR: u16 = 1 << 2;
const MODE_BRACKETED_PASTE: u16 = 1 << 3;
const MODE_SELECTION: u16 = 1 << 4;
const MODE_FOCUS: u16 = 1 << 5;

/// The modifiers held with a key or a touch.
pub const SHIFT: u8 = 1;
pub const ALT: u8 = 2;
pub const CTRL: u8 = 4;

/// The keys that are not text, as the app numbers them.
pub mod key {
    pub const ENTER: u8 = 1;
    pub const TAB: u8 = 2;
    pub const BACKSPACE: u8 = 3;
    pub const ESCAPE: u8 = 4;
    pub const UP: u8 = 5;
    pub const DOWN: u8 = 6;
    pub const RIGHT: u8 = 7;
    pub const LEFT: u8 = 8;
    pub const HOME: u8 = 9;
    pub const END: u8 = 10;
    pub const PAGE_UP: u8 = 11;
    pub const PAGE_DOWN: u8 = 12;
    pub const INSERT: u8 = 13;
    pub const DELETE: u8 = 14;
    pub const F1: u8 = 21;
    pub const F12: u8 = 32;
}

/// What the terminal itself has to say after taking in output.
#[derive(Debug, PartialEq, Eq)]
pub enum Out {
    /// An answer to a question the program asked the terminal: goes back to
    /// the program.
    Reply(Vec<u8>),
    /// The program set a title, or took it back (empty).
    Title(String),
    Bell,
    /// The program asks to put this on the clipboard.
    Clipboard(String),
}

struct Listener(Sender<Event>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

/// The grid's size in cells, and a cell's size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl Size {
    pub fn new(cols: i32, rows: i32, cell_width: i32, cell_height: i32) -> Size {
        Size {
            cols: cols.clamp(2, 1000) as u16,
            rows: rows.clamp(1, 1000) as u16,
            cell_width: cell_width.clamp(1, 1000) as u16,
            cell_height: cell_height.clamp(1, 1000) as u16,
        }
    }
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

struct Inner {
    term: Term<Listener>,
    parser: Processor,
    events: Receiver<Event>,
    size: Size,
}

pub struct Screen {
    inner: Mutex<Inner>,
    /// Whether something changed since the last snapshot.
    dirty: AtomicBool,
}

impl Screen {
    pub fn new(size: Size) -> Screen {
        let (tx, rx) = channel();
        let config = Config {
            scrolling_history: SCROLLBACK,
            // Keys are sent the way every program understands. A program
            // that asks for the kitty keyboard protocol is told no by silence.
            kitty_keyboard: false,
            ..Config::default()
        };
        let term = Term::new(config, &size, Listener(tx));
        Screen {
            inner: Mutex::new(Inner { term, parser: Processor::new(), events: rx, size }),
            dirty: AtomicBool::new(true),
        }
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        // a panic while drawing must not take the terminal with it
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Notes that the screen changed. True if the app has not been told yet.
    pub fn touch(&self) -> bool {
        !self.dirty.swap(true, Ordering::AcqRel)
    }

    pub fn size(&self) -> Size {
        self.inner().size
    }

    /// Takes in what the program printed.
    pub fn advance(&self, bytes: &[u8]) -> Vec<Out> {
        let mut inner = self.inner();
        let Inner { term, parser, events, size } = &mut *inner;
        parser.advance(term, bytes);
        let mut outs = Vec::new();
        for event in events.try_iter() {
            match event {
                // Asked what it is, the engine answers as a VT102 ("?6c"). Programs
                // that wait for that answer take only a longer one: a VT220 with color.
                Event::PtyWrite(text) if text == "\x1b[?6c" => outs.push(Out::Reply(b"\x1b[?62;22c".to_vec())),
                Event::PtyWrite(text) => outs.push(Out::Reply(text.into_bytes())),
                Event::Title(title) => outs.push(Out::Title(title)),
                Event::ResetTitle => outs.push(Out::Title(String::new())),
                Event::Bell => outs.push(Out::Bell),
                Event::ClipboardStore(_, text) => outs.push(Out::Clipboard(text)),
                Event::ColorRequest(index, format) => {
                    outs.push(Out::Reply(format(palette::answer(index, term.colors())).into_bytes()))
                },
                Event::TextAreaSizeRequest(format) => outs.push(Out::Reply(format(window_size(*size)).into_bytes())),
                // What is on the phone's clipboard is not a program's to read.
                _ => {},
            }
        }
        outs
    }

    /// True if the size changed.
    pub fn resize(&self, size: Size) -> bool {
        let mut inner = self.inner();
        if inner.size == size {
            return false;
        }
        inner.size = size;
        inner.term.resize(size);
        true
    }

    // ---- what the fingers do ----

    /// Scrolls by `lines`, back into what was printed if positive. Where a
    /// program has the screen, the bytes to send it instead: wheel turns if
    /// it asked for the mouse, arrow keys otherwise.
    pub fn scroll(&self, lines: i32, col: usize, row: usize) -> Vec<u8> {
        let mut inner = self.inner();
        let mode = *inner.term.mode();
        if lines == 0 {
            return Vec::new();
        }
        let turns = lines.unsigned_abs().min(100) as usize;
        if mode.intersects(TermMode::MOUSE_MODE) {
            let button = if lines > 0 { 64 } else { 65 };
            return mouse_report(mode, button, false, col, row).repeat(turns);
        }
        if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
            let key = if lines > 0 { key::UP } else { key::DOWN };
            return encode_key(key, 0, mode).repeat(turns);
        }
        inner.term.scroll_display(Scroll::Delta(lines));
        Vec::new()
    }

    pub fn scroll_to_bottom(&self) {
        self.inner().term.scroll_display(Scroll::Bottom);
    }

    /// A touch as a mouse event, if the program asked for the mouse.
    /// `kind`: 0 press, 1 release, 2 move.
    pub fn mouse(&self, kind: u8, button: u8, col: usize, row: usize, mods: u8) -> Option<Vec<u8>> {
        let mode = *self.inner().term.mode();
        if !mode.intersects(TermMode::MOUSE_MODE) {
            return None;
        }
        let mut code = button.min(2) as u32;
        if mods & SHIFT != 0 {
            code += 4;
        }
        if mods & ALT != 0 {
            code += 8;
        }
        if mods & CTRL != 0 {
            code += 16;
        }
        Some(match kind {
            0 => mouse_report(mode, code, false, col, row),
            1 => mouse_report(mode, code, true, col, row),
            // a move is only told with a button down, or to a program that wants every move
            _ if mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) => {
                mouse_report(mode, code + 32, false, col, row)
            },
            _ => Vec::new(),
        })
    }

    /// `kind`: 0 start by word, 1 extend, 2 clear, 3 start by character, 4
    /// start by line.
    pub fn select(&self, kind: u8, col: usize, row: usize) {
        let mut inner = self.inner();
        let point = inner.point(col, row);
        let start = |ty| Some(Selection::new(ty, point, Side::Left));
        match kind {
            0 => inner.term.selection = start(SelectionType::Semantic),
            1 => {
                if let Some(selection) = inner.term.selection.as_mut() {
                    selection.update(point, Side::Right);
                }
            },
            3 => inner.term.selection = start(SelectionType::Simple),
            4 => inner.term.selection = start(SelectionType::Lines),
            _ => inner.term.selection = None,
        }
    }

    pub fn selection(&self) -> Option<String> {
        self.inner().term.selection_to_string().filter(|text| !text.is_empty())
    }

    /// The link under a cell: one the program marked (OSC 8), or a web
    /// address in the text.
    pub fn link_at(&self, col: usize, row: usize) -> Option<String> {
        let inner = self.inner();
        let point = inner.point(col, row);
        let grid = inner.term.grid();
        if let Some(link) = grid[point.line][point.column].hyperlink() {
            return Some(link.uri().to_string());
        }
        let line: Vec<char> = (0..grid.columns()).map(|c| grid[point.line][Column(c)].c).collect();
        web_address(&line, point.column.0)
    }

    // ---- what the keys are, in bytes ----

    pub fn key(&self, key: u8, mods: u8) -> Vec<u8> {
        encode_key(key, mods, *self.inner().term.mode())
    }

    /// Typed text. Ctrl with a letter is the control character, Alt puts an
    /// ESC in front.
    pub fn text(&self, text: &str, mods: u8) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len() + 1);
        for c in text.chars() {
            if mods & ALT != 0 {
                out.push(0x1b);
            }
            match control(c).filter(|_| mods & CTRL != 0) {
                Some(byte) => out.push(byte),
                // what a keyboard's Enter is to a program
                None if c == '\n' => out.push(b'\r'),
                None => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
            }
        }
        out
    }

    /// Pasted text: marked as such if the program asked for that, so that
    /// a shell doesn't run it line by line.
    pub fn paste(&self, text: &str) -> Vec<u8> {
        if self.inner().term.mode().contains(TermMode::BRACKETED_PASTE) {
            // text that holds the end mark, or an ESC at all, could step out of the paste
            let clean = text.replace('\x1b', "");
            format!("\x1b[200~{clean}\x1b[201~").into_bytes()
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
        }
    }

    /// What a program that asked to know is told when the terminal comes
    /// to the front or leaves it.
    pub fn focus(&self, focused: bool) -> Vec<u8> {
        if self.inner().term.mode().contains(TermMode::FOCUS_IN_OUT) {
            if focused { b"\x1b[I".to_vec() } else { b"\x1b[O".to_vec() }
        } else {
            Vec::new()
        }
    }

    // ---- what the app draws ----

    /// Writes what is to be seen into `out`: a header, the cells row by
    /// row, then the cells that hold more than one character (the layout is
    /// in this crate's `README.md`). Returns the bytes written, or how many are
    /// needed if `out` is too small.
    pub fn snapshot(&self, out: &mut [u8]) -> Result<usize, usize> {
        self.dirty.store(false, Ordering::Release);
        let inner = self.inner();
        let term = &inner.term;
        let (cols, rows) = (term.columns(), term.screen_lines());
        let cells = HEADER + cols * rows * CELL;
        if out.len() < cells {
            // room for a screen full of accents, so that the second try fits
            return Err(cells + cols * rows * 4);
        }

        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        let selected = palette::blend(palette::BACKGROUND, palette::ACCENT, 0.28);
        let mut extras: Vec<u32> = Vec::new();
        let mut extra_count = 0u32;

        for indexed in content.display_iter {
            let (row, col) = ((indexed.point.line.0 + offset) as usize, indexed.point.column.0);
            if row >= rows || col >= cols {
                continue;
            }
            let cell = indexed.cell;
            let mut fg = palette::resolve(cell.fg, content.colors);
            let mut bg = palette::resolve(cell.bg, content.colors);
            if cell.flags.contains(Flags::DIM) {
                fg = palette::dim(fg);
            }
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.contains(Flags::HIDDEN) {
                fg = bg;
            }
            let mut flags = 0;
            for (theirs, ours) in [
                (Flags::BOLD, BOLD),
                (Flags::ITALIC, ITALIC),
                (Flags::UNDERLINE, UNDERLINE),
                (Flags::DOUBLE_UNDERLINE, DOUBLE_UNDERLINE),
                (Flags::UNDERCURL, UNDERCURL),
                (Flags::DOTTED_UNDERLINE, DOTTED_UNDERLINE),
                (Flags::DASHED_UNDERLINE, DASHED_UNDERLINE),
                (Flags::STRIKEOUT, STRIKEOUT),
                (Flags::WIDE_CHAR, WIDE),
            ] {
                if cell.flags.contains(theirs) {
                    flags |= ours;
                }
            }
            let spacer = cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
            if spacer {
                flags |= SPACER;
            }
            if selection.is_some_and(|range| range.contains(indexed.point)) {
                flags |= SELECTED;
                bg = selected;
            }
            if cell.hyperlink().is_some() {
                flags |= LINK;
            }
            let character = if spacer || cell.c == ' ' { 0 } else { cell.c as u32 };
            if let Some(more) = cell.zerowidth().filter(|_| character != 0) {
                flags |= EXTRAS;
                extra_count += 1;
                extras.push((row * cols + col) as u32);
                extras.push(more.len() as u32);
                extras.extend(more.iter().map(|&c| c as u32));
            }
            let at = HEADER + (row * cols + col) * CELL;
            put_u32(out, at, character);
            put_u32(out, at + 4, 0xff00_0000 | fg);
            put_u32(out, at + 8, 0xff00_0000 | bg);
            put_u32(out, at + 12, flags);
        }

        let total = cells + extras.len() * 4;
        if out.len() < total {
            return Err(total);
        }
        for (i, word) in extras.iter().enumerate() {
            put_u32(out, cells + i * 4, *word);
        }

        // where a point of the grid is on screen, if it is
        let seen = |point: Point| {
            let row = point.line.0 + offset;
            (0..rows as i32).contains(&row).then_some((point.column.0 as u16, row as u16))
        };
        let cursor = match content.cursor.shape {
            CursorShape::Hidden => None,
            _ => seen(content.cursor.point),
        };
        let shape = match content.cursor.shape {
            CursorShape::Underline => 1,
            CursorShape::Beam => 2,
            CursorShape::HollowBlock => 3,
            _ => 0,
        };
        let mode = content.mode;
        let mut bits = 0;
        for (theirs, ours) in [
            (TermMode::ALT_SCREEN, MODE_ALT_SCREEN),
            (TermMode::MOUSE_MODE, MODE_MOUSE),
            (TermMode::APP_CURSOR, MODE_APP_CURSOR),
            (TermMode::BRACKETED_PASTE, MODE_BRACKETED_PASTE),
            (TermMode::FOCUS_IN_OUT, MODE_FOCUS),
        ] {
            if mode.intersects(theirs) {
                bits |= ours;
            }
        }
        if selection.is_some() {
            bits |= MODE_SELECTION;
        }
        let ends = selection.map(|range| (seen(range.start), seen(range.end)));
        let (from, to) = ends.unwrap_or((None, None));

        put_u16(out, 0, cols as u16);
        put_u16(out, 2, rows as u16);
        put_u16(out, 4, cursor.map_or(NOWHERE, |(col, _)| col));
        put_u16(out, 6, cursor.map_or(NOWHERE, |(_, row)| row));
        out[8] = shape;
        out[9] = 0;
        put_u16(out, 10, bits);
        put_u32(out, 12, content.display_offset as u32);
        put_u32(out, 16, term.grid().history_size() as u32);
        put_u32(out, 20, 0xff00_0000 | palette::resolve(named(NamedColor::Background), content.colors));
        put_u32(out, 24, 0xff00_0000 | palette::resolve(named(NamedColor::Foreground), content.colors));
        put_u32(out, 28, 0xff00_0000 | palette::resolve(named(NamedColor::Cursor), content.colors));
        put_u32(out, 32, 0xff00_0000 | palette::CURSOR_TEXT);
        put_u16(out, 36, from.map_or(NOWHERE, |(col, _)| col));
        put_u16(out, 38, from.map_or(NOWHERE, |(_, row)| row));
        put_u16(out, 40, to.map_or(NOWHERE, |(col, _)| col));
        put_u16(out, 42, to.map_or(NOWHERE, |(_, row)| row));
        put_u32(out, 44, extra_count);
        Ok(total)
    }

    /// The text on screen, line by line: for tests, and for saying why a
    /// terminal could not be started.
    pub fn text_lines(&self) -> Vec<String> {
        let inner = self.inner();
        let grid = inner.term.grid();
        (0..grid.screen_lines() as i32)
            .map(|line| {
                let text: String = (0..grid.columns())
                    .map(|col| &grid[Line(line)][Column(col)])
                    .filter(|cell| !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER))
                    .map(|cell| cell.c)
                    .collect();
                text.trim_end().to_string()
            })
            .collect()
    }
}

fn named(name: NamedColor) -> Color {
    Color::Named(name)
}

impl Inner {
    /// The grid point under a cell of what is shown, which may be scrolled
    /// back.
    fn point(&self, col: usize, row: usize) -> Point {
        let offset = self.term.grid().display_offset() as i32;
        let col = col.min(self.term.columns().saturating_sub(1));
        let row = row.min(self.term.screen_lines().saturating_sub(1));
        Point::new(Line(row as i32 - offset), Column(col))
    }
}

fn window_size(size: Size) -> WindowSize {
    WindowSize { num_lines: size.rows, num_cols: size.cols, cell_width: size.cell_width, cell_height: size.cell_height }
}

fn put_u16(out: &mut [u8], at: usize, value: u16) {
    out[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut [u8], at: usize, value: u32) {
    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// The control character Ctrl makes of a key.
fn control(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        '@'..='_' => Some(c as u8 - b'@'),
        ' ' | '2' => Some(0),
        '3' => Some(0x1b),
        '4' => Some(0x1c),
        '5' => Some(0x1d),
        '6' => Some(0x1e),
        '7' | '/' => Some(0x1f),
        '8' | '?' => Some(0x7f),
        _ => None,
    }
}

/// A key that is not text, as xterm sends it.
fn encode_key(key: u8, mods: u8, mode: TermMode) -> Vec<u8> {
    // how xterm numbers the modifiers held with a key
    let held = 1 + (mods & SHIFT) + (mods & ALT) + (mods & CTRL);
    let plain = held == 1;
    let alt = |bytes: &[u8]| {
        let mut out = if mods & ALT != 0 { vec![0x1b] } else { Vec::new() };
        out.extend_from_slice(bytes);
        out
    };
    // ESC [ 1 ; mods X, or ESC [ X (ESC O X where the program asked for that)
    let letter = |final_byte: u8| {
        if !plain {
            format!("\x1b[1;{held}{}", final_byte as char).into_bytes()
        } else if mode.contains(TermMode::APP_CURSOR) {
            vec![0x1b, b'O', final_byte]
        } else {
            vec![0x1b, b'[', final_byte]
        }
    };
    // ESC [ n ~, or ESC [ n ; mods ~
    let tilde = |number: u8| {
        if plain { format!("\x1b[{number}~").into_bytes() } else { format!("\x1b[{number};{held}~").into_bytes() }
    };
    match key {
        key::ENTER => alt(b"\r"),
        key::TAB if mods & SHIFT != 0 => alt(b"\x1b[Z"),
        key::TAB => alt(b"\t"),
        key::BACKSPACE if mods & CTRL != 0 => alt(b"\x08"),
        key::BACKSPACE => alt(b"\x7f"),
        key::ESCAPE => alt(b"\x1b"),
        key::UP => letter(b'A'),
        key::DOWN => letter(b'B'),
        key::RIGHT => letter(b'C'),
        key::LEFT => letter(b'D'),
        key::HOME => letter(b'H'),
        key::END => letter(b'F'),
        key::INSERT => tilde(2),
        key::DELETE => tilde(3),
        key::PAGE_UP => tilde(5),
        key::PAGE_DOWN => tilde(6),
        key::F1..=key::F12 => {
            let n = key - key::F1;
            match n {
                // F1 to F4: ESC O P..S, with modifiers ESC [ 1 ; mods P..S
                0..=3 if plain => vec![0x1b, b'O', b'P' + n],
                0..=3 => format!("\x1b[1;{held}{}", (b'P' + n) as char).into_bytes(),
                _ => tilde([15, 17, 18, 19, 20, 21, 23, 24][n as usize - 4]),
            }
        },
        _ => Vec::new(),
    }
}

/// A mouse event the way the program asked to be told: in SGR form, or the
/// old one that ends at column 223.
fn mouse_report(mode: TermMode, code: u32, release: bool, col: usize, row: usize) -> Vec<u8> {
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if release { 'm' } else { 'M' };
        return format!("\x1b[<{code};{};{}{end}", col + 1, row + 1).into_bytes();
    }
    if col >= 223 || row >= 223 {
        return Vec::new();
    }
    // the old form has no button in a release
    let code = if release { 3 + (code & !3) } else { code };
    vec![0x1b, b'[', b'M', 32 + code as u8, 32 + 1 + col as u8, 32 + 1 + row as u8]
}

/// The web address that the character at `col` of a line is part of.
fn web_address(line: &[char], col: usize) -> Option<String> {
    let part = |c: char| !c.is_whitespace() && !c.is_control() && !"<>\"'`{}|\\^".contains(c) && c != '\0';
    if !line.get(col).copied().is_some_and(part) {
        return None;
    }
    let start = (0..col).rev().find(|&i| !part(line[i])).map_or(0, |i| i + 1);
    let end = (col..line.len()).find(|&i| !part(line[i])).unwrap_or(line.len());
    let word: String = line[start..end].iter().collect();
    let at = ["https://", "http://"].iter().filter_map(|scheme| word.find(scheme)).min()?;
    // what a sentence or a bracket around the address ends with is not part of it
    let address = word[at..].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
    let before = word[..at].chars().count();
    let within = (start + before..start + before + address.chars().count()).contains(&col);
    (within && address.contains("://") && address.len() > "https://".len()).then(|| address.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(cols: i32, rows: i32) -> Screen {
        Screen::new(Size::new(cols, rows, 10, 20))
    }

    fn shot(screen: &Screen) -> Vec<u8> {
        let mut out = vec![0; 1 << 20];
        let len = screen.snapshot(&mut out).unwrap();
        out.truncate(len);
        out
    }

    fn u16_at(shot: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(shot[at..at + 2].try_into().unwrap())
    }

    fn u32_at(shot: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(shot[at..at + 4].try_into().unwrap())
    }

    /// Character, foreground, background and flags of a cell.
    fn cell(shot: &[u8], cols: usize, col: usize, row: usize) -> (char, u32, u32, u32) {
        let at = HEADER + (row * cols + col) * CELL;
        let c = char::from_u32(u32_at(shot, at)).unwrap();
        (c, u32_at(shot, at + 4), u32_at(shot, at + 8), u32_at(shot, at + 12))
    }

    #[test]
    fn a_snapshot_shows_what_was_printed() {
        let screen = screen(20, 4);
        screen.advance(b"hi \x1b[1;31mred\x1b[0m\r\n\x1b[4;7mx");
        let shot = shot(&screen);
        assert_eq!(shot.len(), HEADER + 20 * 4 * CELL);
        assert_eq!((u16_at(&shot, 0), u16_at(&shot, 2)), (20, 4));
        // the cursor: after the x on the second line, a block
        assert_eq!((u16_at(&shot, 4), u16_at(&shot, 6), shot[8]), (1, 1, 0));
        assert_eq!(u32_at(&shot, 20), 0xff00_0000 | palette::BACKGROUND);
        assert_eq!(
            cell(&shot, 20, 0, 0),
            ('h', 0xff00_0000 | palette::FOREGROUND, 0xff00_0000 | palette::BACKGROUND, 0)
        );
        assert_eq!(cell(&shot, 20, 3, 0), ('r', 0xfff2_7a93, 0xff00_0000 | palette::BACKGROUND, BOLD));
        // empty cells hold no character
        assert_eq!(cell(&shot, 20, 2, 0).0, '\0');
        // inverse is applied: the colors come swapped
        assert_eq!(
            cell(&shot, 20, 0, 1),
            ('x', 0xff00_0000 | palette::BACKGROUND, 0xff00_0000 | palette::FOREGROUND, UNDERLINE)
        );
    }

    #[test]
    fn a_buffer_too_small_says_what_it_takes() {
        let screen = screen(10, 2);
        let needed = screen.snapshot(&mut [0; 16]).unwrap_err();
        let mut out = vec![0; needed];
        assert!(screen.snapshot(&mut out).is_ok());
    }

    #[test]
    fn wide_characters_and_accents() {
        let screen = screen(10, 2);
        screen.advance("日e\u{301}".as_bytes());
        let shot = shot(&screen);
        assert_eq!(cell(&shot, 10, 0, 0).0, '日');
        assert_eq!(cell(&shot, 10, 0, 0).3, WIDE);
        assert_eq!(cell(&shot, 10, 1, 0), ('\0', cell(&shot, 10, 1, 0).1, cell(&shot, 10, 1, 0).2, SPACER));
        assert_eq!(cell(&shot, 10, 2, 0).3, EXTRAS);
        // one cell with more: its place, how many, which
        assert_eq!(u32_at(&shot, 44), 1);
        let extras = HEADER + 10 * 2 * CELL;
        assert_eq!((u32_at(&shot, extras), u32_at(&shot, extras + 4), u32_at(&shot, extras + 8)), (2, 1, 0x301));
        assert_eq!(shot.len(), extras + 12);
    }

    #[test]
    fn the_terminal_answers_questions() {
        let screen = screen(10, 2);
        // "what are you?" and "where is the cursor?"
        assert_eq!(screen.advance(b"\x1b[c"), [Out::Reply(b"\x1b[?62;22c".to_vec())]);
        assert_eq!(screen.advance(b"ab\x1b[6n"), [Out::Reply(b"\x1b[1;3R".to_vec())]);
        assert_eq!(screen.advance(b"\x1b]2;vim\x07"), [Out::Title("vim".into())]);
        assert_eq!(screen.advance(b"\x07"), [Out::Bell]);
        // the kitty keyboard protocol is not spoken: the question gets no answer
        assert_eq!(screen.advance(b"\x1b[?u"), []);
    }

    #[test]
    fn keys() {
        let screen = screen(10, 2);
        assert_eq!(screen.key(key::UP, 0), b"\x1b[A");
        assert_eq!(screen.key(key::UP, CTRL), b"\x1b[1;5A");
        assert_eq!(screen.key(key::HOME, SHIFT), b"\x1b[1;2H");
        assert_eq!(screen.key(key::DELETE, 0), b"\x1b[3~");
        assert_eq!(screen.key(key::PAGE_UP, ALT), b"\x1b[5;3~");
        assert_eq!(screen.key(key::F1, 0), b"\x1bOP");
        assert_eq!(screen.key(key::F1 + 4, 0), b"\x1b[15~");
        assert_eq!(screen.key(key::F12, CTRL), b"\x1b[24;5~");
        assert_eq!(screen.key(key::TAB, SHIFT), b"\x1b[Z");
        assert_eq!(screen.key(key::BACKSPACE, 0), b"\x7f");
        assert_eq!(screen.key(key::ENTER, ALT), b"\x1b\r");
        // a program that asked for application cursor keys
        screen.advance(b"\x1b[?1h");
        assert_eq!(screen.key(key::UP, 0), b"\x1bOA");
        assert_eq!(screen.key(key::UP, SHIFT), b"\x1b[1;2A");
    }

    #[test]
    fn typed_and_pasted_text() {
        let screen = screen(10, 2);
        assert_eq!(screen.text("ls\n", 0), b"ls\r");
        assert_eq!(screen.text("c", CTRL), [3]);
        assert_eq!(screen.text("C", CTRL), [3]);
        assert_eq!(screen.text("[", CTRL), [0x1b]);
        assert_eq!(screen.text("b", ALT), b"\x1bb");
        assert_eq!(screen.text("é", 0), "é".as_bytes());
        assert_eq!(screen.paste("a\r\nb\nc"), b"a\rb\rc");
        screen.advance(b"\x1b[?2004h");
        assert_eq!(screen.paste("a\nb"), b"\x1b[200~a\nb\x1b[201~");
        // text can't end the paste early to have its rest run
        assert_eq!(screen.paste("x\x1b[201~rm"), b"\x1b[200~x[201~rm\x1b[201~");
    }

    #[test]
    fn scrolling_goes_to_whoever_has_the_screen() {
        let screen = screen(10, 2);
        screen.advance(b"1\r\n2\r\n3\r\n4");
        // the shell's output: scrolled here
        assert_eq!(screen.scroll(1, 0, 0), b"");
        assert_eq!(u32_at(&shot(&screen), 12), 1);
        assert_eq!(cell(&shot(&screen), 10, 0, 0).0, '2');
        screen.scroll_to_bottom();
        assert_eq!(u32_at(&shot(&screen), 12), 0);
        // a program on the alternate screen gets arrow keys
        screen.advance(b"\x1b[?1049h");
        assert_eq!(screen.scroll(2, 0, 0), b"\x1b[A\x1b[A");
        assert_eq!(screen.scroll(-1, 0, 0), b"\x1b[B");
        // and one that asked for the mouse gets the wheel
        screen.advance(b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(screen.scroll(1, 4, 1), b"\x1b[<64;5;2M");
        assert_eq!(screen.scroll(-1, 4, 1), b"\x1b[<65;5;2M");
    }

    #[test]
    fn the_mouse_only_for_programs_that_want_it() {
        let screen = screen(10, 2);
        assert_eq!(screen.mouse(0, 0, 1, 1, 0), None);
        screen.advance(b"\x1b[?1000h");
        assert_eq!(screen.mouse(0, 0, 1, 1, 0), Some(b"\x1b[M\x20\x22\x22".to_vec()));
        assert_eq!(screen.mouse(1, 0, 1, 1, 0), Some(b"\x1b[M\x23\x22\x22".to_vec()));
        assert_eq!(screen.mouse(2, 0, 1, 1, 0), Some(Vec::new()));
        screen.advance(b"\x1b[?1006h\x1b[?1002h");
        assert_eq!(screen.mouse(0, 0, 1, 1, CTRL), Some(b"\x1b[<16;2;2M".to_vec()));
        assert_eq!(screen.mouse(1, 0, 1, 1, 0), Some(b"\x1b[<0;2;2m".to_vec()));
        assert_eq!(screen.mouse(2, 0, 3, 1, 0), Some(b"\x1b[<32;4;2M".to_vec()));
    }

    #[test]
    fn selecting() {
        let screen = screen(20, 3);
        screen.advance(b"one two three\r\nfour");
        assert_eq!(screen.selection(), None);
        // a long press on "two" takes the word
        screen.select(0, 5, 0);
        assert_eq!(screen.selection().as_deref(), Some("two"));
        let shot1 = shot(&screen);
        assert_eq!(cell(&shot1, 20, 4, 0).3 & SELECTED, SELECTED);
        assert_eq!(cell(&shot1, 20, 3, 0).3 & SELECTED, 0);
        assert_eq!(u16_at(&shot1, 10) & MODE_SELECTION, MODE_SELECTION);
        assert_eq!((u16_at(&shot1, 36), u16_at(&shot1, 38), u16_at(&shot1, 40), u16_at(&shot1, 42)), (4, 0, 6, 0));
        // dragging on takes more words
        screen.select(1, 1, 1);
        assert_eq!(screen.selection().as_deref(), Some("two three\nfour"));
        screen.select(2, 0, 0);
        assert_eq!(screen.selection(), None);
        assert_eq!(u16_at(&shot(&screen), 36), NOWHERE);
        screen.select(4, 2, 1);
        assert_eq!(screen.selection().as_deref(), Some("four\n"));
    }

    #[test]
    fn links() {
        let screen = screen(60, 3);
        screen.advance(b"see (https://example.com/a?b=1). now\r\n");
        screen.advance(b"\x1b]8;;glint-copy:///home/u/a.txt\x1b\\a.txt\x1b]8;;\x1b\\ plain");
        assert_eq!(screen.link_at(8, 0).as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(screen.link_at(29, 0).as_deref(), Some("https://example.com/a?b=1"));
        // the bracket in front, the bracket and the full stop behind, the next word
        for col in [4, 30, 31, 33] {
            assert_eq!(screen.link_at(col, 0), None, "column {col}");
        }
        assert_eq!(screen.link_at(1, 0), None);
        assert_eq!(screen.link_at(2, 1).as_deref(), Some("glint-copy:///home/u/a.txt"));
        assert_eq!(screen.link_at(8, 1), None);
        assert_eq!(cell(&shot(&screen), 60, 2, 1).3 & LINK, LINK);
    }

    #[test]
    fn a_new_size() {
        let screen = screen(10, 2);
        assert!(!screen.resize(Size::new(10, 2, 10, 20)));
        assert!(screen.resize(Size::new(30, 5, 10, 20)));
        let shot = shot(&screen);
        assert_eq!((u16_at(&shot, 0), u16_at(&shot, 2)), (30, 5));
        // a program that asks for the size in pixels is told the new one
        assert_eq!(screen.advance(b"\x1b[14t"), [Out::Reply(b"\x1b[4;100;300t".to_vec())]);
    }

    #[test]
    fn told_once_until_looked_at() {
        let screen = screen(10, 2);
        // a new screen has yet to be drawn
        assert!(!screen.touch());
        shot(&screen);
        assert!(screen.touch());
        assert!(!screen.touch());
        shot(&screen);
        assert!(screen.touch());
    }
}
