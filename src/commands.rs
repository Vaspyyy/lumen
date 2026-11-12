//! What a shell says about its commands: where a prompt begins, where a
//! command's output begins, and how the command ended (OSC 133).
//!
//! The terminal's own parser drops those marks. They are found in the
//! program's output on its way there ([`Scanner`]) and applied between the
//! bytes before and the bytes after them ([`advance`]).
//!
//! What a mark says about the grid is written into the grid. While a prompt
//! and the command typed at it are being written, every cell gets a link only
//! lumen makes (`lumen-mark:...`), as the cells of an image do. So a prompt
//! scrolls, is erased and is rewrapped with the text it is made of, and
//! nothing has to follow the grid's lines. What a mark says about the command
//! itself (its text, how it ended, how long it took) is kept in the [`Log`],
//! under the number in the link.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Handler, Hyperlink};

/// What the link of a cell of a prompt or of a typed command starts with.
const SCHEME: &str = "lumen-mark:";

/// The part of a command's own lines a cell belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The prompt, and what was typed where the shell doesn't say where the prompt ends.
    Prompt,
    /// What was typed at the prompt.
    Input,
}

/// The command a cell belongs to and the part of it, if its link is one of
/// lumen's marks.
pub fn cell_of(uri: &str) -> Option<(u32, Part)> {
    let (id, part) = uri.strip_prefix(SCHEME)?.split_once(':')?;
    let part = match part {
        "p" => Part::Prompt,
        "i" => Part::Input,
        _ => return None,
    };
    Some((id.parse().ok()?, part))
}

/// What a shell said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A prompt begins (`A`).
    Prompt,
    /// The prompt is over, what is typed follows (`B`).
    Input,
    /// The command was sent off, its output follows (`C`). Some shells say
    /// what the command was.
    Output { command: Option<String> },
    /// The command ended (`D`), with this exit status if the shell said it.
    End { exit: Option<i32> },
}

impl Mark {
    /// What is between `ESC ] 133 ;` and the end of the sequence.
    fn parse(payload: &[u8]) -> Option<Mark> {
        let text = std::str::from_utf8(payload).ok()?;
        let mut fields = text.split(';');
        Some(match fields.next()? {
            "A" => Mark::Prompt,
            "B" => Mark::Input,
            "C" => {
                let command = fields
                    .find_map(|field| field.strip_prefix("cmdline_url="))
                    .and_then(crate::links::percent_decode)
                    .and_then(|bytes| String::from_utf8(bytes).ok());
                Mark::Output { command }
            },
            "D" => Mark::End { exit: fields.next().and_then(|field| field.parse().ok()) },
            _ => return None,
        })
    }
}

#[derive(Default, Clone, Copy, PartialEq)]
enum State {
    #[default]
    Text,
    /// After ESC.
    Esc,
    /// In an operating system command (`ESC ]`), which may be a mark.
    Osc,
    /// After ESC inside one.
    OscEsc,
}

/// A mark's text is short, but the command a shell names in it need not be.
const MAX_MARK: usize = 64 << 10;

/// Finds the marks in output that arrives in arbitrary pieces. The output
/// itself is left as it is: the parser has no use for the marks, and skips
/// them.
#[derive(Default)]
pub struct Scanner {
    state: State,
    /// What the sequence being read holds so far, for as long as it can still be a mark.
    payload: Vec<u8>,
    /// It can't be one any more.
    other: bool,
}

impl Scanner {
    /// Calls `found` for each mark that ends in `input`, with how many bytes
    /// of `input` come before and with it.
    pub fn feed(&mut self, input: &[u8], mut found: impl FnMut(usize, Mark)) {
        const PREFIX: &[u8] = b"133;";
        let mut i = 0;
        while i < input.len() {
            match self.state {
                State::Text => match input[i..].iter().position(|&b| b == 0x1b) {
                    Some(at) => {
                        i += at + 1;
                        self.state = State::Esc;
                    },
                    None => i = input.len(),
                },
                State::Esc => {
                    if input[i] == b']' {
                        i += 1;
                        self.payload.clear();
                        self.other = false;
                        self.state = State::Osc;
                    } else {
                        // some other escape sequence; this byte is looked at again, it may be the next ESC
                        self.state = State::Text;
                    }
                },
                State::Osc => {
                    let end = input[i..].iter().position(|&b| matches!(b, 0x07 | 0x1b | 0x18 | 0x1a));
                    let upto = end.map_or(input.len(), |at| i + at);
                    if !self.other {
                        self.payload.extend_from_slice(&input[i..upto]);
                        let known = self.payload.len().min(PREFIX.len());
                        self.other = self.payload[..known] != PREFIX[..known] || self.payload.len() > MAX_MARK;
                        if self.other {
                            self.payload.clear();
                        }
                    }
                    i = upto;
                    if i < input.len() {
                        let byte = input[i];
                        i += 1;
                        match byte {
                            0x07 => self.finish(i, &mut found),
                            0x1b => self.state = State::OscEsc,
                            // cancelled
                            _ => self.state = State::Text,
                        }
                    }
                },
                State::OscEsc => {
                    if input[i] == b'\\' {
                        i += 1;
                        self.finish(i, &mut found);
                    } else {
                        // an ESC that doesn't end the sequence starts another one: this one is void
                        self.state = State::Esc;
                    }
                },
            }
        }
    }

    fn finish(&mut self, end: usize, found: &mut impl FnMut(usize, Mark)) {
        self.state = State::Text;
        if !self.other
            && let Some(mark) = self.payload.strip_prefix(b"133;").and_then(Mark::parse)
        {
            found(end, mark);
        }
        self.payload.clear();
    }
}

/// How far a command has come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// It is being typed, or its prompt was left without one.
    Typed,
    Running,
    /// It ended, with this exit status where the shell said it.
    Ended(Option<i32>),
}

/// What is known about one command.
#[derive(Clone, Debug, Default)]
pub struct Command {
    /// What was typed, when the shell said it or marked where it stands.
    pub text: Option<String>,
    /// The folder the shell was in when the command was sent off.
    pub cwd: Option<PathBuf>,
    /// When it was sent off.
    pub started: Option<Instant>,
    /// The same moment on the wall clock.
    pub at: Option<SystemTime>,
    pub ended: Option<Instant>,
    pub exit: Option<i32>,
}

impl Command {
    pub fn status(&self) -> Status {
        match (self.started, self.ended) {
            (None, _) => Status::Typed,
            (Some(_), None) => Status::Running,
            (Some(_), Some(_)) => Status::Ended(self.exit),
        }
    }

    /// How long it ran, or has been running.
    pub fn took(&self) -> Option<Duration> {
        let started = self.started?;
        Some(self.ended.unwrap_or_else(Instant::now).saturating_duration_since(started))
    }

    /// It ended in a way that is the command's own fault: not with success,
    /// and not because it was interrupted from the keyboard.
    pub fn failed(&self) -> bool {
        matches!(self.status(), Status::Ended(Some(code)) if code != 0 && code != INTERRUPTED)
    }
}

/// The exit status of a command that Ctrl+C ended: 128 plus SIGINT.
pub const INTERRUPTED: i32 = 130;

impl Command {
    /// How it stands, in a few words. Nothing for a command that was never sent off.
    pub fn describe(&self) -> Option<String> {
        let took = span(self.took()?);
        Some(match self.status() {
            Status::Typed => return None,
            Status::Running => format!("running for {took}"),
            Status::Ended(Some(0)) => format!("done in {took}"),
            Status::Ended(Some(INTERRUPTED)) => format!("interrupted after {took}"),
            Status::Ended(Some(code)) => format!("exit {code} after {took}"),
            Status::Ended(None) => format!("ended after {took}"),
        })
    }

    /// The command in a few words, for where it is named in passing.
    pub fn name(&self) -> Option<String> {
        let text = self.text.as_deref()?.lines().next()?.trim();
        const MOST: usize = 36;
        Some(match text.char_indices().nth(MOST) {
            Some((at, _)) => format!("{}…", text[..at].trim_end()),
            None => text.to_string(),
        })
    }
}

/// A length of time as a person would say it.
pub fn span(time: Duration) -> String {
    let seconds = time.as_secs();
    match seconds {
        0 => format!("{} ms", time.as_millis()),
        1..60 => format!("{:.1} s", time.as_secs_f32()),
        60..3600 => format!("{} min {} s", seconds / 60, seconds % 60),
        _ => format!("{} h {} min", seconds / 3600, seconds % 3600 / 60),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    /// No prompt is open and no command runs.
    #[default]
    Idle,
    Prompt,
    Input,
    Running,
}

/// More commands than this are not remembered: the oldest go.
const MAX_COMMANDS: usize = 4000;

/// The commands of one terminal.
#[derive(Default)]
pub struct Log {
    commands: BTreeMap<u32, Command>,
    next: u32,
    /// The command whose prompt is open or that runs.
    current: Option<u32>,
    phase: Phase,
    /// The command that ended last.
    last_ended: Option<u32>,
    /// How many commands have ended: what tells a reader that one has since it last looked.
    ended: u64,
    /// The shell's process, whose folder a command starts in.
    shell: Option<u32>,
}

pub type Shared = Arc<Mutex<Log>>;

impl Log {
    pub fn new(shell: u32) -> Log {
        Log { shell: Some(shell), ..Log::default() }
    }

    pub fn get(&self, id: u32) -> Option<&Command> {
        self.commands.get(&id)
    }

    /// The command that is running, with its number.
    pub fn running(&self) -> Option<(u32, &Command)> {
        let id = self.current.filter(|_| self.phase == Phase::Running)?;
        Some((id, self.commands.get(&id)?))
    }

    /// The command that ended last, and how many have ended so far.
    pub fn last_ended(&self) -> (Option<&Command>, u64) {
        (self.last_ended.and_then(|id| self.commands.get(&id)), self.ended)
    }

    fn open(&mut self) -> u32 {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        self.commands.insert(id, Command::default());
        while self.commands.len() > MAX_COMMANDS {
            self.commands.pop_first();
        }
        self.current = Some(id);
        id
    }

    fn end(&mut self, exit: Option<i32>) {
        if let Some(id) = self.current
            && let Some(command) = self.commands.get_mut(&id)
        {
            command.ended = Some(Instant::now());
            command.exit = exit;
            self.last_ended = Some(id);
            self.ended += 1;
        }
        self.phase = Phase::Idle;
    }

    /// Takes on what a mark says, with the cursor where the mark stood in
    /// the output.
    pub fn apply<L: EventListener>(&mut self, mark: Mark, term: &mut Term<L>) {
        // a full-screen program's screen has no commands; a shell inside it is that program's to show
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        match mark {
            Mark::Prompt => {
                if self.phase == Phase::Running {
                    // the shell is back at a prompt without having said how the command ended
                    self.end(None);
                }
                // A prompt that is written again where it stands (after a resize, after
                // a completion) is still the same prompt. One on a line of its own is a new one.
                let line = term.grid().cursor.point.line;
                let again = matches!(self.phase, Phase::Prompt | Phase::Input)
                    && self.current.is_some_and(|id| owner(term, line) == Some(id));
                let id = match self.current {
                    Some(id) if again => id,
                    _ => self.open(),
                };
                self.phase = Phase::Prompt;
                tag(term, Some((id, Part::Prompt)));
            },
            Mark::Input => {
                if let (Some(id), Phase::Prompt | Phase::Input) = (self.current, self.phase) {
                    self.phase = Phase::Input;
                    tag(term, Some((id, Part::Input)));
                }
            },
            Mark::Output { command } => {
                tag(term, None);
                let id = match (self.current, self.phase) {
                    (Some(id), Phase::Prompt | Phase::Input) => id,
                    (_, Phase::Running) => {
                        self.end(None);
                        self.open()
                    },
                    // a command without a prompt before it
                    _ => self.open(),
                };
                let text = command.or_else(|| typed(term, id)).map(|text| text.trim().to_string());
                let cwd = self.shell.and_then(|pid| std::fs::read_link(format!("/proc/{pid}/cwd")).ok());
                if let Some(entry) = self.commands.get_mut(&id) {
                    entry.text = text.filter(|text| !text.is_empty());
                    entry.cwd = cwd;
                    entry.started = Some(Instant::now());
                    entry.at = Some(SystemTime::now());
                }
                self.phase = Phase::Running;
            },
            Mark::End { exit } => {
                tag(term, None);
                if self.phase == Phase::Running {
                    self.end(exit);
                }
                self.phase = Phase::Idle;
            },
        }
    }
}

/// Parses `bytes` into the grid, and takes on the marks among them where
/// they stand.
pub fn advance<L: EventListener>(
    scanner: &mut Scanner,
    log: &Shared,
    parser: &mut ansi::Processor,
    term: &mut Term<L>,
    bytes: &[u8],
) {
    let mut marks = Vec::new();
    scanner.feed(bytes, |end, mark| marks.push((end, mark)));
    let mut done = 0;
    for (end, mark) in marks {
        parser.advance(term, &bytes[done..end]);
        done = end;
        // Inside a synchronized update the parser holds the text back until the
        // update ends. The mark belongs between that text, so the update is ended
        // here: in order matters more than all at once.
        if parser.sync_bytes_count() > 0 {
            parser.stop_sync(term);
        }
        log.lock().unwrap().apply(mark, term);
    }
    parser.advance(term, &bytes[done..]);
}

/// Makes what is written from here on cells of this command's own lines, or
/// plain cells again.
fn tag<L: EventListener>(term: &mut Term<L>, cells: Option<(u32, Part)>) {
    match cells {
        Some((id, part)) => {
            let part = if part == Part::Prompt { 'p' } else { 'i' };
            let uri = format!("{SCHEME}{id}:{part}");
            term.set_hyperlink(Some(Hyperlink { id: Some(uri.clone()), uri }));
        },
        None => {
            // a link the program itself has open is the program's
            let ours = term.grid().cursor.template.hyperlink().is_some_and(|link| cell_of(link.uri()).is_some());
            if ours {
                term.set_hyperlink(None);
            }
        },
    }
}

/// The command a line belongs to as part of its prompt or of what was typed:
/// the number in the first mark on it.
pub fn owner<L>(term: &Term<L>, line: Line) -> Option<u32> {
    if line < term.topmost_line() || line > term.bottommost_line() {
        return None;
    }
    term.grid()[line].into_iter().find_map(|cell| cell_of(cell.hyperlink()?.uri())).map(|(id, _)| id)
}

/// The first and the last of the lines around `line` that are the command's
/// own. A line of the command that nothing was written to (an empty line in
/// what was typed) is between them, and doesn't end the search.
pub fn own_lines<L>(term: &Term<L>, line: Line, id: u32) -> (Line, Line) {
    // a prompt with the command typed at it is no taller than this
    let reach = term.screen_lines() as i32;
    let mut first = line;
    for above in (line.0 - reach..line.0).rev().map(Line) {
        match owner(term, above) {
            Some(other) if other == id => first = above,
            Some(_) => break,
            None if above < term.topmost_line() => break,
            None => {},
        }
    }
    let mut last = line;
    for below in (line.0 + 1..=line.0 + reach).map(Line) {
        match owner(term, below) {
            Some(other) if other == id => last = below,
            Some(_) => break,
            None if below > term.bottommost_line() => break,
            None => {},
        }
    }
    (first, last)
}

/// What was typed at the prompt of this command, read from the cells the
/// shell marked as typed. The cursor is where the command was sent off.
fn typed<L>(term: &Term<L>, id: u32) -> Option<String> {
    let cursor = term.grid().cursor.point.line;
    // the cursor is on the command's last line, or on the line after it
    let line = [cursor, cursor - 1].into_iter().find(|&line| owner(term, line) == Some(id))?;
    let (first, last) = own_lines(term, line, id);
    let mut text = String::new();
    let mut any = false;
    for line in (first.0..=last.0).map(Line) {
        let row = &term.grid()[line];
        let mut wrapped = false;
        for cell in row.into_iter() {
            wrapped = cell.flags.contains(Flags::WRAPLINE);
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let input = cell.hyperlink().and_then(|link| cell_of(link.uri())) == Some((id, Part::Input));
            if input {
                any = true;
                text.push(cell.c);
                text.extend(cell.zerowidth().into_iter().flatten());
            }
        }
        if !wrapped && line != last {
            text.push('\n');
        }
    }
    any.then_some(text)
}

/// Where a command is in the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub id: u32,
    /// The first and last of its own lines: the prompt and what was typed.
    pub first: Line,
    pub last: Line,
}

/// The command whose own lines `line` is in.
pub fn place_at<L>(term: &Term<L>, line: Line) -> Option<Place> {
    let id = owner(term, line)?;
    let (first, last) = own_lines(term, line, id);
    Some(Place { id, first, last })
}

/// The nearest command above `line` that `wanted` says yes to: the one that
/// printed `line`, if `line` is output.
pub fn above<L>(term: &Term<L>, line: Line, mut wanted: impl FnMut(u32) -> bool) -> Option<Place> {
    let top = term.topmost_line();
    let mut line = line - 1;
    while line >= top {
        match place_at(term, line) {
            Some(place) if wanted(place.id) => return Some(place),
            Some(place) => line = place.first - 1,
            None => line -= 1,
        }
    }
    None
}

/// The nearest command below `line` that `wanted` says yes to.
pub fn below<L>(term: &Term<L>, line: Line, mut wanted: impl FnMut(u32) -> bool) -> Option<Place> {
    let bottom = term.bottommost_line();
    let mut line = line + 1;
    while line <= bottom {
        match place_at(term, line) {
            Some(place) if wanted(place.id) => return Some(place),
            Some(place) => line = place.last + 1,
            None => line += 1,
        }
    }
    None
}

/// The lines a command printed: from below its own lines to above the next
/// prompt, without the empty lines at the end. None if it printed nothing.
pub fn output<L>(term: &Term<L>, place: Place) -> Option<(Line, Line)> {
    let start = place.last + 1;
    let mut end = below(term, place.last, |_| true).map_or(term.bottommost_line(), |next| next.first - 1);
    while end >= start && term.grid()[end].is_clear() {
        end -= 1;
    }
    (end >= start).then_some((start, end))
}

/// The text of these lines, as a selection of them would be copied.
pub fn text<L>(term: &Term<L>, lines: (Line, Line)) -> String {
    term.bounds_to_string(Point::new(lines.0, Column(0)), Point::new(lines.1, term.last_column()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TermSize;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config as TermConfig;

    fn marks(pieces: &[&[u8]]) -> Vec<(usize, Mark)> {
        let mut scanner = Scanner::default();
        let mut found = Vec::new();
        for piece in pieces {
            scanner.feed(piece, |end, mark| found.push((end, mark)));
        }
        found
    }

    #[test]
    fn marks_are_found_in_the_output() {
        let found = marks(&[b"one\x1b]133;A\x07two\x1b]133;D;1\x1b\\three"]);
        assert_eq!(found, [(11, Mark::Prompt), (25, Mark::End { exit: Some(1) })]);
        // what fish sends
        let found =
            marks(&[b"\x1b]133;A;click_events=1\x1b\\$ \x1b]133;B\x1b\\\x1b]133;C;cmdline_url=echo%20hi%3B\x1b\\"]);
        let kinds: Vec<Mark> = found.into_iter().map(|(_, mark)| mark).collect();
        assert_eq!(kinds, [Mark::Prompt, Mark::Input, Mark::Output { command: Some("echo hi;".into()) }]);
        assert_eq!(marks(&[b"\x1b]133;D\x07"]), [(8, Mark::End { exit: None })]);
        assert_eq!(marks(&[b"\x1b]133;D;130;aid=7\x07"]), [(18, Mark::End { exit: Some(130) })]);
    }

    #[test]
    fn however_the_output_is_cut_up() {
        let whole: &[u8] = b"a\x1b]133;C;cmdline_url=ls%20-l\x1b\\b\x1b]133;D;0\x07c";
        let expected = marks(&[whole]);
        assert_eq!(expected.len(), 2);
        for cut in 1..whole.len() {
            let (first, second) = whole.split_at(cut);
            let mut scanner = Scanner::default();
            let mut found = Vec::new();
            scanner.feed(first, |end, mark| found.push((end, mark)));
            scanner.feed(second, |end, mark| found.push((cut + end, mark)));
            assert_eq!(found, expected, "cut at {cut}");
        }
    }

    #[test]
    fn other_sequences_are_not_marks() {
        // a title, a link, a clipboard, a mark of another kind, a cancelled and a broken off one
        for other in [
            &b"\x1b]0;133;A\x07"[..],
            b"\x1b]8;;http://x/133;A\x1b\\",
            b"\x1b]1337;A\x07",
            b"\x1b]133;Z\x07",
            b"\x1b]133;A\x18\x07",
            b"\x1b]133;A\x1b[0m\x07",
            b"\x1b[133;A\x07",
            b"133;A\x07",
        ] {
            assert_eq!(marks(&[other]), [], "{:?}", String::from_utf8_lossy(other));
        }
        // a sequence that is broken off doesn't swallow the one after it
        assert_eq!(marks(&[b"\x1b]133;A\x1b]133;B\x07"]), [(15, Mark::Input)]);
        // something long that is no mark is not kept
        let mut scanner = Scanner::default();
        scanner.feed(b"\x1b]52;c;", |_, _| panic!());
        scanner.feed(&vec![b'A'; 1 << 20], |_, _| panic!());
        assert!(scanner.payload.is_empty());
        scanner.feed(b"\x07\x1b]133;A\x07", |end, mark| assert_eq!((end, mark), (9, Mark::Prompt)));
    }

    /// A terminal of 30 by 8 cells, and what a shell printed to it.
    struct Screen {
        term: Term<VoidListener>,
        log: Shared,
        scanner: Scanner,
        parser: ansi::Processor,
    }

    impl Screen {
        fn new() -> Screen {
            let size = TermSize { cols: 30, rows: 8, cell_width: 8., cell_height: 16. };
            Screen {
                term: Term::new(TermConfig::default(), &size, VoidListener),
                log: Shared::default(),
                scanner: Scanner::default(),
                parser: ansi::Processor::new(),
            }
        }

        fn print(&mut self, text: &str) -> &mut Screen {
            advance(&mut self.scanner, &self.log, &mut self.parser, &mut self.term, text.as_bytes());
            self
        }

        /// A prompt, a command typed at it, its output, and how it ended.
        fn run(&mut self, command: &str, output: &str, exit: i32) -> &mut Screen {
            self.print("\x1b]133;A\x07~> \x1b]133;B\x07").print(command).print("\r\n\x1b]133;C\x07");
            self.print(output).print(&format!("\x1b]133;D;{exit}\x07"))
        }

        fn line(&self, line: i32) -> String {
            let row = &self.term.grid()[Line(line)];
            row.into_iter().map(|cell| cell.c).collect::<String>().trim_end().to_string()
        }

        /// The first lines of the commands, top to bottom, with their numbers.
        fn places(&self) -> Vec<(u32, i32, i32)> {
            let mut places = Vec::new();
            let mut line = self.term.topmost_line() - 1;
            while let Some(place) = below(&self.term, line, |_| true) {
                places.push((place.id, place.first.0, place.last.0));
                line = place.last;
            }
            places
        }

        fn command(&self, id: u32) -> Command {
            self.log.lock().unwrap().get(id).cloned().unwrap()
        }
    }

    #[test]
    fn a_command_is_known_by_its_prompt() {
        let mut screen = Screen::new();
        screen.run("echo hi", "hi\r\n", 0).run("false", "", 1).print("\x1b]133;A\x07~> \x1b]133;B\x07");
        assert_eq!(screen.line(0), "~> echo hi");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 2, 2), (2, 3, 3)]);
        let (first, second, third) = (screen.command(0), screen.command(1), screen.command(2));
        assert_eq!(first.text.as_deref(), Some("echo hi"));
        assert_eq!(first.status(), Status::Ended(Some(0)));
        assert!(!first.failed());
        assert_eq!(second.text.as_deref(), Some("false"));
        assert!(second.failed());
        assert_eq!(third.status(), Status::Typed);
        // what was printed is not part of a command's own lines
        assert_eq!(owner(&screen.term, Line(1)), None);
        assert_eq!(output(&screen.term, place_at(&screen.term, Line(0)).unwrap()), Some((Line(1), Line(1))));
        assert_eq!(output(&screen.term, place_at(&screen.term, Line(2)).unwrap()), None);
        // the prompt that is open has no link left open behind it either
        screen.print("ls");
        assert_eq!(owner(&screen.term, Line(3)), Some(2));
        let log = screen.log.lock().unwrap();
        assert_eq!(log.last_ended().1, 2);
        assert!(log.running().is_none());
    }

    #[test]
    fn the_shell_can_name_the_command() {
        let mut screen = Screen::new();
        // no mark for where the prompt ends: the text comes from the shell
        screen.print("\x1b]133;A\x07~> make all\r\n\x1b]133;C;cmdline_url=make%20all\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("make all"));
        assert_eq!(screen.command(0).status(), Status::Running);
        assert_eq!(screen.log.lock().unwrap().running().map(|(id, _)| id), Some(0));
        // without either, the command is known but not its text
        screen.print("\x1b]133;D\x07\x1b]133;A\x07~> true\r\n\x1b]133;C\x07\x1b]133;D;0\x07");
        assert_eq!(screen.command(0).status(), Status::Ended(None));
        assert_eq!(screen.command(1).text, None);
        assert_eq!(screen.command(1).status(), Status::Ended(Some(0)));
    }

    #[test]
    fn a_prompt_written_again_is_the_same_prompt() {
        let mut screen = Screen::new();
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07ec");
        // the shell writes its prompt again in place, as after a completion
        screen.print("\r\x1b]133;A\x07~> \x1b]133;B\x07echo");
        assert_eq!(screen.places(), [(0, 0, 0)]);
        // an empty line sent off: the next prompt is a new one, and the old one stays what it was
        screen.print("\x1b[2K\r\n\x1b]133;A\x07~> \x1b]133;B\x07");
        assert_eq!(screen.places(), [(1, 1, 1)]);
        screen.print("x\r\n\x1b]133;A\x07~> \x1b]133;B\x07");
        assert_eq!(screen.places(), [(1, 1, 1), (2, 2, 2)]);
        assert_eq!(screen.command(1).status(), Status::Typed);
    }

    #[test]
    fn what_was_typed_over_several_lines() {
        let mut screen = Screen::new();
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07for x in 1 2\r\n   echo $x\r\n\r\n   end\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("for x in 1 2\n   echo $x\n\n   end"));
        assert_eq!(screen.places(), [(0, 0, 3)]);
        // a long line that wraps is one line
        let mut screen = Screen::new();
        let long = "echo ".to_string() + &"x".repeat(40);
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07").print(&long).print("\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some(long.as_str()));
        assert_eq!(screen.places(), [(0, 0, 1)]);
    }

    #[test]
    fn a_shell_that_says_less() {
        let mut screen = Screen::new();
        // a prompt after a command that never said it ended
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07sleep 9\r\n\x1b]133;C\x07");
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07");
        assert_eq!(screen.command(0).status(), Status::Ended(None));
        assert_eq!(screen.places(), [(0, 0, 0), (1, 1, 1)]);
        // an end without a command before it says nothing
        let mut screen = Screen::new();
        screen.print("\x1b]133;D;0\x07\x1b]133;A\x07~> ");
        assert_eq!(screen.log.lock().unwrap().last_ended().1, 0);
        // a command without a prompt
        let mut screen = Screen::new();
        screen.print("\x1b]133;C\x07out\r\n\x1b]133;D;3\x07");
        assert_eq!(screen.command(0).status(), Status::Ended(Some(3)));
        assert_eq!(screen.places(), []);
    }

    #[test]
    fn marks_go_where_the_text_goes() {
        let mut screen = Screen::new();
        for n in 0..12 {
            screen.run(&format!("echo {n}"), &format!("{n}\r\n"), n % 2);
        }
        // 24 lines in a terminal of 8: most have scrolled away, with their marks
        let places = screen.places();
        assert_eq!(places.len(), 12);
        assert_eq!(places.first(), Some(&(0, -17, -17)));
        assert_eq!(places.last(), Some(&(11, 5, 5)));
        // from the top of the screen: the command above, the command below
        let term = &screen.term;
        assert_eq!(above(term, Line(0), |_| true).map(|place| place.id), Some(8));
        assert_eq!(below(term, Line(1), |_| true).map(|place| place.id), Some(10));
        // the output of a line names its command
        assert_eq!(above(term, Line(-16), |_| true).map(|place| place.id), Some(0));
        let failed = |id: u32| screen.command(id).failed();
        assert_eq!(above(term, Line(0), failed).map(|place| place.id), Some(7));
        let fourth = place_at(term, Line(-11)).unwrap();
        assert_eq!(text(term, output(term, fourth).unwrap()), "3");
        // erased, a prompt is gone
        screen.print("\x1b[2J\x1b[3J\x1b[H");
        assert_eq!(screen.places(), []);
    }

    #[test]
    fn output_ends_before_the_next_prompt() {
        let mut screen = Screen::new();
        screen.run("seq 3", "1\r\n2\r\n3\r\n\r\n", 0).run("true", "", 0);
        let first = place_at(&screen.term, Line(0)).unwrap();
        let lines = output(&screen.term, first).unwrap();
        assert_eq!(lines, (Line(1), Line(3)));
        assert_eq!(text(&screen.term, lines), "1\n2\n3");
        // a command that is still printing: all there is so far
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07make\r\n\x1b]133;C\x07one\r\ntwo");
        // (the last line scrolled the screen up by one)
        let running = place_at(&screen.term, Line(5)).unwrap();
        assert_eq!(text(&screen.term, output(&screen.term, running).unwrap()), "one\ntwo");
    }

    #[test]
    fn a_full_screen_program_has_no_commands() {
        let mut screen = Screen::new();
        screen.run("vim", "\x1b[?1049h\x1b]133;A\x07~> \x1b]133;B\x07x\x1b]133;C\x07\x1b]133;D;1\x07\x1b[?1049l", 0);
        assert_eq!(screen.places(), [(0, 0, 0)]);
        assert_eq!(screen.command(0).status(), Status::Ended(Some(0)));
        assert_eq!(screen.log.lock().unwrap().last_ended().1, 1);
    }

    #[test]
    fn a_link_of_the_program_is_left_alone() {
        let mut screen = Screen::new();
        // the prompt holds a link of its own, and one is open while the command runs
        screen
            .print("\x1b]133;A\x07\x1b]8;;http://example.com\x1b\\~\x1b]8;;\x1b\\> \x1b]133;B\x07ls\r\n\x1b]133;C\x07");
        let link = |col: usize| screen.term.grid()[Line(0)][Column(col)].hyperlink().map(|link| link.uri().to_string());
        assert_eq!(link(0).as_deref(), Some("http://example.com"));
        assert_eq!(link(3).as_deref(), Some("lumen-mark:0:i"));
        screen.print("\x1b]8;;http://example.com\x1b\\name\x1b]133;D;0\x07 more");
        let link = |col: usize| screen.term.grid()[Line(1)][Column(col)].hyperlink().map(|link| link.uri().to_string());
        assert_eq!(link(5).as_deref(), Some("http://example.com"));
    }

    #[test]
    fn a_command_in_words() {
        let ago = |ms: u64| Instant::now().checked_sub(Duration::from_millis(ms)).unwrap();
        let ended = |exit: Option<i32>, text: &str| Command {
            text: Some(text.into()),
            started: Some(ago(2400)),
            ended: Some(ago(0)),
            exit,
            ..Command::default()
        };
        assert_eq!(ended(Some(0), "ls").describe().as_deref(), Some("done in 2.4 s"));
        assert_eq!(ended(Some(1), "ls").describe().as_deref(), Some("exit 1 after 2.4 s"));
        assert_eq!(ended(Some(130), "ls").describe().as_deref(), Some("interrupted after 2.4 s"));
        assert_eq!(ended(None, "ls").describe().as_deref(), Some("ended after 2.4 s"));
        assert!(!ended(Some(130), "ls").failed() && !ended(None, "ls").failed() && ended(Some(2), "ls").failed());
        let running = Command { started: Some(ago(65_000)), ..Command::default() };
        assert_eq!(running.describe().as_deref(), Some("running for 1 min 5 s"));
        assert_eq!(Command::default().describe(), None);
        assert_eq!(span(Duration::from_millis(38)), "38 ms");
        assert_eq!(span(Duration::from_secs(59)), "59.0 s");
        assert_eq!(span(Duration::from_secs(3600 + 4 * 60 + 9)), "1 h 4 min");
        // a long command, and one of several lines, are named by their start
        assert_eq!(ended(Some(0), "cargo test").name().as_deref(), Some("cargo test"));
        let long = ended(Some(0), "cargo test --workspace --locked -- --exact some::long::name");
        assert_eq!(long.name().as_deref(), Some("cargo test --workspace --locked -- -…"));
        assert_eq!(ended(Some(0), "for x in 1 2\n  echo $x\nend").name().as_deref(), Some("for x in 1 2"));
        assert_eq!(Command::default().name(), None);
    }

    #[test]
    fn links_of_marks() {
        assert_eq!(cell_of("lumen-mark:7:p"), Some((7, Part::Prompt)));
        assert_eq!(cell_of("lumen-mark:12:i"), Some((12, Part::Input)));
        assert_eq!(cell_of("lumen-mark:7"), None);
        assert_eq!(cell_of("lumen-mark:x:p"), None);
        assert_eq!(cell_of("lumen-image:7:0:0"), None);
        assert_eq!(cell_of("https://example.com"), None);
    }
}
