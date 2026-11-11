//! The bytes that turn any terminal into a copy of a session.
//!
//! A terminal that attaches to a session may be brand new, or may still
//! show what it showed before its connection was lost. [`replay`] writes a
//! reset, then every line of the scrollback and the screen with the colors
//! and attributes of each cell, then the cursor, the pen and the modes a
//! program set. A terminal of the same size that takes these bytes ends up
//! laid out cell for cell like the session's.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{
    self, Color, CursorShape, CursorStyle, KeyboardModes, KeyboardModesApplyBehavior, NamedColor,
};

/// The attributes a cell's look depends on, which SGR sets.
const LOOK: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::ALL_UNDERLINES)
    .union(Flags::INVERSE)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT);

/// Where the kitty keyboard flags are in [`TermMode`].
const KITTY_SHIFT: u32 = 18;

/// Bytes that make a terminal of the same size a copy of `term`. `title`
/// is the title the program set, if it set one (the engine keeps it to
/// itself).
///
/// While a program is on the alternate screen, the primary screen is
/// reached by swapping to it and back, which leaves `term` as it was.
pub fn replay<L: EventListener>(term: &mut Term<L>, title: Option<&str>) -> Vec<u8> {
    let mut out = Writer::default();
    // a full reset, the scrollback cleared too, the cursor at the top
    out.raw(b"\x1bc\x1b[3J\x1b[H");
    let mode = *term.mode();
    if mode.contains(TermMode::ALT_SCREEN) {
        let alternate = term.grid().clone();
        term.swap_alt();
        let primary_kitty = kitty_flags(*term.mode());
        out.grid(term.grid());
        // the cursor the terminal saves on the way to the alternate screen
        out.cursor(term.grid());
        out.kitty(primary_kitty);
        term.swap_alt();
        *term.grid_mut() = alternate;
        if kitty_flags(*term.mode()) != kitty_flags(mode) {
            // set without a push: the swap took it from the stack instead
            let modes = KeyboardModes::from_bits_truncate(kitty_flags(mode));
            ansi::Handler::set_keyboard_mode(term, modes, KeyboardModesApplyBehavior::Replace);
        }
        out.raw(b"\x1b[?1049h\x1b[H");
        out.grid(term.grid());
    } else {
        out.grid(term.grid());
    }
    out.state(term, title);
    out.bytes
}

fn kitty_flags(mode: TermMode) -> u8 {
    ((mode & TermMode::KITTY_KEYBOARD_PROTOCOL).bits() >> KITTY_SHIFT) as u8
}

/// What the receiving terminal's pen is, so that SGR goes out only where a
/// cell differs from the one before.
#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
    pen: Pen,
}

#[derive(Clone, PartialEq)]
struct Pen {
    fg: Color,
    bg: Color,
    flags: Flags,
    underline: Option<Color>,
    link: Option<Hyperlink>,
}

impl Default for Pen {
    fn default() -> Pen {
        Pen {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            underline: None,
            link: None,
        }
    }
}

impl Pen {
    fn of(cell: &Cell) -> Pen {
        Pen {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & LOOK,
            underline: cell.underline_color(),
            link: cell.hyperlink(),
        }
    }
}

impl Writer {
    fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn text(&mut self, text: &str) {
        self.bytes.extend_from_slice(text.as_bytes());
    }

    /// Every line of a grid, the scrollback first. The receiving terminal's
    /// cursor is at the top left of a cleared screen.
    fn grid(&mut self, grid: &Grid<Cell>) {
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;
        let last = grid.columns() - 1;
        for line in top..=bottom {
            let row = &grid[Line(line)];
            // a line that went on in the next one: written to its end, the next one
            // follows directly and the terminal's own wrapping joins them again
            let joined = row[Column(last)].flags.contains(Flags::WRAPLINE)
                && line < bottom
                && content_end(grid, Line(line + 1)) > 0;
            let end = if joined { last + 1 } else { content_end(grid, Line(line)) };
            for column in 0..end {
                let cell = &row[Column(column)];
                // the right half of a wide character, or the gap a wide character
                // left at the end of a line: the terminal makes both itself
                if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                self.cell(cell, column);
            }
            if line < bottom && !joined {
                self.raw(b"\r\n");
            }
        }
    }

    fn cell(&mut self, cell: &Cell, column: usize) {
        self.pen(Pen::of(cell));
        if cell.c == '\t' {
            // a tab the program printed: the engine marks where it started
            self.text(&format!("\t\x1b[{}G", column + 2));
            return;
        }
        let mut buf = [0u8; 4];
        self.text(cell.c.encode_utf8(&mut buf));
        for c in cell.zerowidth().unwrap_or_default() {
            self.text(c.encode_utf8(&mut buf));
        }
    }

    fn pen(&mut self, pen: Pen) {
        if pen == self.pen {
            return;
        }
        if pen.link != self.pen.link {
            match &pen.link {
                Some(link) => {
                    // what a program put into a link must not end the sequence early
                    let plain = |part: &str| part.chars().filter(|c| !c.is_control()).collect::<String>();
                    self.text(&format!("\x1b]8;id={};{}\x1b\\", plain(link.id()), plain(link.uri())))
                },
                None => self.raw(b"\x1b]8;;\x1b\\"),
            }
        }
        let mut sgr = String::from("\x1b[0");
        for (flag, code) in [
            (Flags::BOLD, "1"),
            (Flags::DIM, "2"),
            (Flags::ITALIC, "3"),
            (Flags::UNDERLINE, "4"),
            (Flags::DOUBLE_UNDERLINE, "4:2"),
            (Flags::UNDERCURL, "4:3"),
            (Flags::DOTTED_UNDERLINE, "4:4"),
            (Flags::DASHED_UNDERLINE, "4:5"),
            (Flags::INVERSE, "7"),
            (Flags::HIDDEN, "8"),
            (Flags::STRIKEOUT, "9"),
        ] {
            if pen.flags.contains(flag) {
                sgr.push(';');
                sgr.push_str(code);
            }
        }
        color(&mut sgr, pen.fg, 30, 90, "38");
        color(&mut sgr, pen.bg, 40, 100, "48");
        if let Some(underline) = pen.underline {
            color(&mut sgr, underline, 0, 0, "58");
        }
        sgr.push('m');
        self.text(&sgr);
        self.pen = pen;
    }

    /// Puts the receiving terminal's cursor where the grid's is, in the
    /// same state, with the same pen.
    fn cursor(&mut self, grid: &Grid<Cell>) {
        let cursor = &grid.cursor;
        let line = cursor.point.line.0 + 1;
        let column = cursor.point.column.0;
        if cursor.input_needs_wrap {
            // The cursor is past the last character written: the next one goes to
            // the next line. Writing that character again leaves it in that state.
            let row = &grid[cursor.point.line];
            let mut at = column;
            if row[Column(at)].flags.contains(Flags::WIDE_CHAR_SPACER) && at > 0 {
                at -= 1;
            }
            self.text(&format!("\x1b[{line};{}H", at + 1));
            self.cell(&row[Column(at)], at);
        } else {
            self.text(&format!("\x1b[{line};{}H", column + 1));
        }
        self.pen(Pen::of(&cursor.template));
    }

    /// The cursor, the pen and everything else a program may have set.
    fn state<L: EventListener>(&mut self, term: &Term<L>, title: Option<&str>) {
        let mode = *term.mode();
        // origin mode moves the cursor home: first
        if mode.contains(TermMode::ORIGIN) {
            self.raw(b"\x1b[?6h");
        }
        self.cursor(term.grid());
        let private = [
            (TermMode::APP_CURSOR, 1),
            (TermMode::MOUSE_REPORT_CLICK, 1000),
            (TermMode::MOUSE_DRAG, 1002),
            (TermMode::MOUSE_MOTION, 1003),
            (TermMode::FOCUS_IN_OUT, 1004),
            (TermMode::UTF8_MOUSE, 1005),
            (TermMode::SGR_MOUSE, 1006),
            (TermMode::BRACKETED_PASTE, 2004),
        ];
        for (flag, number) in private {
            if mode.contains(flag) {
                self.text(&format!("\x1b[?{number}h"));
            }
        }
        if mode.contains(TermMode::APP_KEYPAD) {
            self.raw(b"\x1b=");
        }
        if !mode.contains(TermMode::ALTERNATE_SCROLL) {
            self.raw(b"\x1b[?1007l");
        }
        if !mode.contains(TermMode::LINE_WRAP) {
            self.raw(b"\x1b[?7l");
        }
        if mode.contains(TermMode::INSERT) {
            self.raw(b"\x1b[4h");
        }
        if mode.contains(TermMode::LINE_FEED_NEW_LINE) {
            self.raw(b"\x1b[20h");
        }
        if !mode.contains(TermMode::SHOW_CURSOR) {
            self.raw(b"\x1b[?25l");
        }
        let style = term.cursor_style();
        if style != CursorStyle::default() {
            let shape = match style.shape {
                CursorShape::Underline => 3,
                CursorShape::Beam => 5,
                _ => 1,
            };
            self.text(&format!("\x1b[{} q", if style.blinking { shape } else { shape + 1 }));
        }
        if let Some(title) = title {
            let title: String = title.chars().filter(|c| !c.is_control()).collect();
            self.text(&format!("\x1b]2;{title}\x1b\\"));
        }
        self.kitty(kitty_flags(mode));
    }

    fn kitty(&mut self, flags: u8) {
        if flags != 0 {
            self.text(&format!("\x1b[>{flags}u"));
        }
    }
}

/// SGR for a color: the 16 named ones with their own codes, the 256 by
/// number, the rest as RGB. The default colors need nothing after a reset.
fn color(sgr: &mut String, color: Color, base: u8, bright: u8, extended: &str) {
    match color {
        Color::Named(named) => {
            let index = named as usize;
            if base == 0 {
                // underlines have no short codes
                if index < 16 {
                    sgr.push_str(&format!(";{extended};5;{index}"));
                }
            } else if index < 8 {
                sgr.push_str(&format!(";{}", base as usize + index));
            } else if index < 16 {
                sgr.push_str(&format!(";{}", bright as usize + index - 8));
            }
        },
        Color::Indexed(index) => sgr.push_str(&format!(";{extended};5;{index}")),
        Color::Spec(rgb) => sgr.push_str(&format!(";{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b)),
    }
}

/// How many cells of a line are worth writing: up to the last one that is
/// not a plain blank. A blank with a background or an attribute counts.
fn content_end(grid: &Grid<Cell>, line: Line) -> usize {
    let row = &grid[line];
    let blank = |cell: &Cell| {
        cell.c == ' '
            && cell.fg == Color::Named(NamedColor::Foreground)
            && cell.bg == Color::Named(NamedColor::Background)
            && (cell.flags - Flags::WRAPLINE).is_empty()
            && cell.extra.is_none()
    };
    (0..grid.columns()).rev().find(|&c| !blank(&row[Column(c)])).map_or(0, |c| c + 1)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use alacritty_terminal::event::Event;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use super::*;
    use crate::terminal::TermSize;

    /// Keeps the title the way the keeper does.
    #[derive(Clone, Default)]
    struct Titles(Arc<Mutex<Option<String>>>);

    impl EventListener for Titles {
        fn send_event(&self, event: Event) {
            match event {
                Event::Title(title) => *self.0.lock().unwrap() = Some(title),
                Event::ResetTitle => *self.0.lock().unwrap() = None,
                _ => {},
            }
        }
    }

    fn term(cols: usize, rows: usize) -> Term<Titles> {
        term_with(cols, rows, Titles::default())
    }

    fn term_with(cols: usize, rows: usize, titles: Titles) -> Term<Titles> {
        let size = TermSize { cols, rows, cell_width: 8., cell_height: 16. };
        let config = Config { scrolling_history: 5000, kitty_keyboard: true, ..Config::default() };
        Term::new(config, &size, titles)
    }

    fn feed(term: &mut Term<Titles>, bytes: &[u8]) {
        let mut parser: Processor = Processor::new();
        parser.advance(term, bytes);
    }

    /// Feeds `input` into a session, replays it into a fresh terminal and
    /// compares the two. Returns both for more checks.
    fn round_trip(cols: usize, rows: usize, input: &[u8]) -> (Term<Titles>, Term<Titles>) {
        let titles = Titles::default();
        let mut a = term_with(cols, rows, titles.clone());
        feed(&mut a, input);
        let title = titles.0.lock().unwrap().clone();
        let bytes = replay(&mut a, title.as_deref());
        let mut b = term(cols, rows);
        // the client may show anything before: stale content from a lost connection
        feed(&mut b, b"\x1b[31mstale\r\n\x1b[?1049hold alternate\x1b[?1h");
        feed(&mut b, &bytes);
        assert_same(&a, &b);
        (a, b)
    }

    fn assert_grids(a: &Grid<Cell>, b: &Grid<Cell>, what: &str) {
        assert_eq!(a.history_size(), b.history_size(), "{what}: scrollback lines");
        for line in a.topmost_line().0..=a.bottommost_line().0 {
            for column in 0..a.columns() {
                let (x, y) = (&a[Line(line)][Column(column)], &b[Line(line)][Column(column)]);
                let at = format!("{what}: line {line}, column {column}");
                assert_eq!(x.c, y.c, "{at}: character");
                assert_eq!(x.fg, y.fg, "{at}: foreground");
                assert_eq!(x.bg, y.bg, "{at}: background");
                assert_eq!(x.flags, y.flags, "{at}: flags");
                assert_eq!(x.zerowidth(), y.zerowidth(), "{at}: zero-width characters");
                assert_eq!(x.underline_color(), y.underline_color(), "{at}: underline color");
                assert_eq!(x.hyperlink(), y.hyperlink(), "{at}: link");
            }
        }
        assert_eq!(a.cursor.point, b.cursor.point, "{what}: cursor");
        assert_eq!(a.cursor.input_needs_wrap, b.cursor.input_needs_wrap, "{what}: cursor past the end");
        assert!(Pen::of(&a.cursor.template) == Pen::of(&b.cursor.template), "{what}: pen");
    }

    fn assert_same(a: &Term<Titles>, b: &Term<Titles>) {
        assert_grids(a.grid(), b.grid(), "screen");
        assert_eq!(*a.mode(), *b.mode(), "modes");
        assert_eq!(a.cursor_style(), b.cursor_style(), "cursor style");
    }

    /// Compares the primary screens of two terminals on the alternate screen.
    fn assert_primaries(a: &mut Term<Titles>, b: &mut Term<Titles>) {
        let (alt_a, alt_b) = (a.grid().clone(), b.grid().clone());
        a.swap_alt();
        b.swap_alt();
        assert_grids(a.grid(), b.grid(), "primary screen");
        a.swap_alt();
        b.swap_alt();
        *a.grid_mut() = alt_a;
        *b.grid_mut() = alt_b;
    }

    #[test]
    fn colors_and_attributes() {
        let input = "plain \x1b[1mbold\x1b[22m \x1b[2mdim\x1b[0m \x1b[3mitalic\x1b[0m \x1b[4munder\x1b[0m \
             \x1b[4:3mcurly\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:4mdotted\x1b[4:5mdashed\x1b[0m \x1b[7minverse\x1b[0m \x1b[8mhidden\x1b[0m \
             \x1b[9mstrike\x1b[0m\r\n\x1b[31mred\x1b[92m bright green\x1b[38;5;208m orange\x1b[48;5;17m on navy\
             \x1b[38;2;10;20;30;48;2;200;100;0m rgb\x1b[0m\r\n\x1b[44mblue to the end\x1b[K\x1b[0m\r\n\
             \x1b[4;58;2;255;0;0mred underline\x1b[0m \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ after\r\n\
             \x1b[1;35mthe pen stays";
        round_trip(80, 10, input.as_bytes());
    }

    #[test]
    fn wide_characters_and_emoji() {
        // a wide character that doesn't fit at the end of a line moves to the next one
        let input = "日本語 テキスト 👋🏽 e\u{301} cafe\u{301}\r\n123456789日本\r\n12345678日本";
        round_trip(10, 6, input.as_bytes());
    }

    #[test]
    fn wrapped_long_lines() {
        let long: String = (0..250).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let input = format!("{long}\r\nshort\r\n{}\r\n\x1b[42m{}\x1b[0m", "x".repeat(40), " ".repeat(55));
        let (a, b) = round_trip(40, 8, input.as_bytes());
        assert!(a.grid()[Line(0)][Column(39)].flags.contains(Flags::WRAPLINE));
        assert_eq!(b.grid().cursor.point.line, a.grid().cursor.point.line);
    }

    #[test]
    fn more_than_fits_goes_to_the_scrollback() {
        let input: String = (0..300).map(|i| format!("\x1b[3{}mline {i}\x1b[0m\r\n", i % 8)).collect();
        let (a, _) = round_trip(30, 12, input.as_bytes());
        assert!(a.grid().history_size() > 280);
    }

    #[test]
    fn a_cleared_screen_with_a_few_lines() {
        let input: String =
            (0..50).map(|i| format!("old {i}\r\n")).collect::<String>() + "\x1b[H\x1b[2Jone\r\ntwo\r\nthree";
        round_trip(30, 10, input.as_bytes());
    }

    #[test]
    fn the_cursor_in_the_middle_of_a_line() {
        round_trip(30, 6, b"some text here\x1b[1;6H\x1b[4;33m");
        round_trip(30, 6, b"some text\r\n\x1b[3;20H");
        // past the last column: the next character goes to the next line
        round_trip(10, 4, b"0123456789");
        round_trip(10, 4, b"12345678\xe6\x97\xa5");
        round_trip(10, 4, b"a\tb\tc");
    }

    #[test]
    fn modes_and_cursor_style() {
        round_trip(
            40,
            8,
            b"\x1b[?1h\x1b=\x1b[?2004h\x1b[?1004h\x1b[?1002h\x1b[?1006h\x1b[?7l\x1b[4h\x1b[5 q\x1b[?25l\x1b[>5u\x1b[?1007l text",
        );
        round_trip(40, 8, b"\x1b[?1003h\x1b[?1005h\x1b[20h\x1b[2 q");
        round_trip(40, 8, b"\x1b[3;6r\x1b[?6h\x1b[2;3Hin the region");
    }

    #[test]
    fn the_alternate_screen() {
        let mut input: Vec<u8> = (0..40).flat_map(|i| format!("shell line {i}\r\n").into_bytes()).collect();
        input.extend_from_slice(b"$ vim\x1b[?1049h\x1b[H\x1b[2J\x1b[44m~ editor\x1b[0m\r\n~\x1b[1;3H\x1b[>1u\x1b[?1h");
        let (mut a, mut b) = round_trip(30, 10, &input);
        assert!(b.mode().contains(TermMode::ALT_SCREEN));
        assert_primaries(&mut a, &mut b);
        assert_same(&a, &b);
        // leaving it shows the shell as it was on both
        feed(&mut a, b"\x1b[?1049l");
        feed(&mut b, b"\x1b[?1049l");
        assert_same(&a, &b);
        assert_eq!(a.grid()[Line(9)][Column(0)].c, b.grid()[Line(9)][Column(0)].c);
    }

    #[test]
    fn the_title_goes_along() {
        let mut a = term(20, 4);
        feed(&mut a, b"\x1b]2;my title\x07hi");
        let bytes = replay(&mut a, Some("my title"));
        let mut b = term(20, 4);
        feed(&mut b, &bytes);
        assert_same(&a, &b);
        assert!(String::from_utf8_lossy(&bytes).contains("\x1b]2;my title\x1b\\"));
    }
}
