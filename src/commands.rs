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
//!
//! A program's wish to tell the user something (OSC 777, OSC 9) is dropped
//! by the parser in the same way, and is found here too ([`Note`]).

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
    /// What is left of a prompt that was written again further down: its
    /// earlier copy, with what stood typed at it and what the shell printed
    /// under it. No prompt, nothing typed, and nothing a command printed.
    Left,
}

/// The command a cell belongs to and the part of it, if its link is one of
/// lumen's marks.
pub fn cell_of(uri: &str) -> Option<(u32, Part)> {
    let (id, part) = uri.strip_prefix(SCHEME)?.split_once(':')?;
    let part = match part {
        "p" => Part::Prompt,
        "i" => Part::Input,
        "x" => Part::Left,
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
    /// What follows is to begin on a line of its own (`L`): a new prompt is
    /// on its way.
    FreshLine,
    /// A prompt beside or within what is typed (`P`): the one at the right
    /// of the line (`right`), or the one a further line of the command
    /// begins with. What is typed goes on after the next `B`.
    Aside { right: bool },
    /// Not a mark of a command: a program has something to tell the user.
    Notify(Note),
}

/// What a program wants the user to know: that a long run is over, say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// What it is about. Empty where the program gave only a text.
    pub title: String,
    pub body: String,
}

/// The most characters of a notification's title and of its text that are kept.
pub const TITLE_MOST: usize = 120;
const BODY_MOST: usize = 400;

/// A program's text as a notification can show it: without control
/// characters, and not longer than `most` characters.
pub fn shown(text: &str, most: usize) -> String {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    text.trim().chars().take(most).collect()
}

impl Note {
    /// The texts as they can be shown. None if nothing is left of them.
    fn new(title: &str, body: &str) -> Option<Note> {
        let note = Note { title: shown(title, TITLE_MOST), body: shown(body, BODY_MOST) };
        (!note.title.is_empty() || !note.body.is_empty()).then_some(note)
    }
}

/// What the sequences lumen takes note of begin with, after `ESC ]`.
const KINDS: [&[u8]; 3] = [b"133;", b"777;notify;", b"9;"];

impl Mark {
    /// What is between `ESC ]` and the end of the sequence.
    fn parse(payload: &[u8]) -> Option<Mark> {
        let text = std::str::from_utf8(payload).ok()?;
        if let Some(rest) = text.strip_prefix("777;notify;") {
            // `title ; body`: what OSC 777 calls a notification
            let (title, body) = rest.split_once(';').unwrap_or((rest, ""));
            return Note::new(title, body).map(Mark::Notify);
        }
        if let Some(rest) = text.strip_prefix("9;") {
            // OSC 9 is a notification's text. Where a number follows it is one of
            // ConEmu's sequences (a progress bar, a working directory): not for the user.
            let first = rest.split(';').next().unwrap_or_default();
            if first.is_empty() || first.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            return Note::new("", rest).map(Mark::Notify);
        }
        let mut fields = text.strip_prefix("133;")?.split(';');
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
            "L" => Mark::FreshLine,
            // a prompt by its kind: right (`r`), continuation (`c`) and secondary (`s`)
            // ones stand where a command is typed, any other begins one
            "P" => match fields.find_map(|field| field.strip_prefix("k=")) {
                Some("r") => Mark::Aside { right: true },
                Some("c" | "s") => Mark::Aside { right: false },
                _ => Mark::Prompt,
            },
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
    /// How many bytes went to the terminal that the log has not been told of.
    fed: usize,
}

impl Scanner {
    /// Calls `found` for each mark that ends in `input`, with how many bytes
    /// of `input` come before and with it.
    pub fn feed(&mut self, input: &[u8], mut found: impl FnMut(usize, Mark)) {
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
                        let could_be = KINDS.iter().any(|kind| {
                            let known = self.payload.len().min(kind.len());
                            self.payload[..known] == kind[..known]
                        });
                        self.other = !could_be || self.payload.len() > MAX_MARK;
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
            && let Some(mark) = Mark::parse(&self.payload)
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
    /// The shell never said that it ended: the next prompt came while it ran.
    pub left: bool,
    /// The most memory the pane was seen to use while it ran, in bytes, where
    /// something looked ([`Log::sample`]).
    pub peak: Option<u64>,
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
            Status::Ended(None) if self.left => format!("ran {took} before the next prompt"),
            Status::Ended(None) => format!("ended after {took}"),
        })
    }

    /// The most memory it was seen to use, where that is worth a word: a
    /// command that has ended, and more than a shell alone takes.
    pub fn peak(&self) -> Option<u64> {
        const WORTH_A_WORD: u64 = 100 << 20;
        self.peak.filter(|&peak| self.ended.is_some() && peak >= WORTH_A_WORD)
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
    /// How many commands have ended, and how many of them failed: what tells
    /// a reader that one has since it last looked.
    ended: u64,
    failed: u64,
    /// The shell asked for a line of its own since the last prompt began:
    /// the prompt that comes is a new one.
    fresh: bool,
    /// A prompt beside what is typed is being written: the next `B` ends it.
    aside: bool,
    /// How many bytes were written to the terminal since the prompt that is
    /// open began, or was last written again. An earlier copy of it is no
    /// more lines up than that: a line takes a byte at least. An image counts
    /// for as many bytes as it can have rows.
    written: usize,
    /// The same since the end of the prompt was last told: what stands typed
    /// above that end is no older.
    typed: usize,
    /// The most cells a line had and the most lines the screen had before
    /// the terminal last changed its size, since the prompt that is open
    /// began. Nothing, where it has kept its size.
    widest: usize,
    tallest: usize,
    /// The command that ran when it did. How it ended may still be said.
    over: Option<u32>,
    /// The shell's process, whose folder a command starts in.
    shell: Option<u32>,
    /// What programs wanted the user to know, until someone takes it.
    notes: Vec<Note>,
}

/// More notes than this don't wait to be taken: the oldest go.
const MAX_NOTES: usize = 4;

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

    /// How many commands have ended so far, and how many of them failed.
    pub fn ended(&self) -> (u64, u64) {
        (self.ended, self.failed)
    }

    /// The shell has not told of any command: it doesn't mark them.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// What programs wanted the user to know since this was last asked.
    pub fn take_notes(&mut self) -> Vec<Note> {
        std::mem::take(&mut self.notes)
    }

    /// The pane was seen to use this much memory. The command that runs keeps the most of it.
    pub fn sample(&mut self, memory: u64) {
        if let Some(id) = self.current.filter(|_| self.phase == Phase::Running)
            && let Some(command) = self.commands.get_mut(&id)
        {
            command.peak = Some(command.peak.unwrap_or(0).max(memory));
        }
    }

    /// This much more was written to the terminal: bytes, or what moves its
    /// lines as that many bytes do.
    pub fn wrote(&mut self, bytes: usize) {
        self.written = self.written.saturating_add(bytes);
        self.typed = self.typed.saturating_add(bytes);
    }

    /// The terminal had lines of `columns` cells on a screen of `screen`
    /// lines, and is given another size.
    fn resized(&mut self, columns: usize, screen: usize) {
        self.widest = self.widest.max(columns);
        self.tallest = self.tallest.max(screen);
    }

    /// How many lines above the cursor's something can stand at most that
    /// was on the screen when `bytes` were still to be written: a line for
    /// each byte, the cursor's own, and two screens for a screen that was
    /// cleared in between and for where on it the cursor stood. On lines
    /// that were longer once, each of those can have become several rows: a
    /// wide character that doesn't fit at the end of a row leaves a cell
    /// empty there.
    fn reach<L>(&self, term: &Term<L>, bytes: usize) -> usize {
        let lines = bytes.saturating_add(2 * self.tallest.max(term.screen_lines()) + 1);
        let columns = term.columns();
        let rows = if self.widest > columns { self.widest.div_ceil(columns.saturating_sub(1).max(1)) } else { 1 };
        lines.saturating_mul(rows)
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
            self.ended += 1;
            self.failed += command.failed() as u64;
        }
        self.phase = Phase::Idle;
    }

    /// A prompt or another command came while a command ran, without a word
    /// of how it ended. A shell inside it has taken over (one it started,
    /// or one on another machine), or the shell never says how commands end.
    /// As far as lumen can tell it is over, but nobody is told of an end
    /// that nobody has seen.
    fn leave(&mut self) {
        if let Some(command) = self.current.and_then(|id| self.commands.get_mut(&id)) {
            command.ended = Some(Instant::now());
            command.exit = None;
            command.left = true;
        }
        self.phase = Phase::Idle;
    }

    /// Takes on what a mark says, with the cursor where the mark stood in
    /// the output.
    pub fn apply<L: EventListener>(&mut self, mark: Mark, term: &mut Term<L>) {
        // a note is not about the grid: a full-screen program has as much to tell as any other
        let mark = match mark {
            Mark::Notify(note) => {
                if self.notes.len() >= MAX_NOTES {
                    self.notes.remove(0);
                }
                self.notes.push(note);
                return;
            },
            mark => mark,
        };
        // a full-screen program's screen has no commands; a shell inside it is that program's to show
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        match mark {
            Mark::FreshLine => {
                // what is written from here on is the next prompt's, not typed at the last one
                tag(term, None);
                // bash writes its prompt where the output stopped: after output without a
                // line break at its end, the output's last line would be the prompt's
                if term.grid().cursor.point.column.0 > 0 {
                    term.carriage_return();
                    term.linefeed();
                }
                self.fresh = true;
                self.aside = false;
                // A prompt is on its way, so what ran is over. bash says how it ended
                // in the prompt itself. If the prompt doesn't say it (a theme has set it
                // anew, or it is the first one of a shell inside the command), the
                // command is not left running for good.
                if self.phase == Phase::Running {
                    self.over = self.current;
                    self.leave();
                }
            },
            Mark::Prompt => {
                if self.phase == Phase::Running {
                    // a prompt, and no word of how the command ended
                    self.leave();
                }
                // What the shell wrote on this line before the mark, while a command was
                // being typed, was marked as typed: readline's mode, which stands before
                // all of a prompt. It is this prompt's, and says nothing of whose line it is.
                let typing = matches!(self.phase, Phase::Prompt | Phase::Input);
                let before = self.current.filter(|_| typing).map(|id| typed_before(term, id)).unwrap_or_default();
                remark(term, &before, None);
                // No command was sent off since the prompt that is open: this is that
                // prompt, written again. After a resize, under a list of completions, on
                // a cleared screen, below an empty line that was sent off, below a shell
                // that ended at its prompt: nothing ran, so there is nothing to tell
                // apart. A shell that asks for a line of its own before a prompt says
                // that it is a new one.
                let line = term.grid().cursor.point.line;
                let fresh = std::mem::take(&mut self.fresh);
                let again = typing && !fresh;
                self.over = None;
                self.aside = false;
                let id = match self.current {
                    Some(id) if again => {
                        // what is left of it further up is an earlier copy, and no command
                        let reach = self.reach(term, self.written);
                        forget_above(term, line, id, None, reach);
                        id
                    },
                    _ => self.open(),
                };
                (self.written, self.typed) = (0, 0);
                (self.widest, self.tallest) = (0, 0);
                self.phase = Phase::Prompt;
                remark(term, &before, Some(id));
                tag(term, Some((id, Part::Prompt)));
            },
            Mark::Aside { right } => {
                if let (Some(id), Phase::Prompt | Phase::Input) = (self.current, self.phase) {
                    // What a further line begins with before its prompt (readline's
                    // mode) was marked as typed. Before a prompt at the right of the
                    // line stands the command.
                    if !right {
                        let before = typed_before(term, id);
                        remark(term, &before, Some(id));
                    }
                    self.aside = true;
                    tag(term, Some((id, Part::Prompt)));
                }
            },
            Mark::Input => {
                if let (Some(id), Phase::Prompt | Phase::Input) = (self.current, self.phase) {
                    self.phase = Phase::Input;
                    // the end of a prompt beside what is typed: typing goes on
                    if !std::mem::take(&mut self.aside) {
                        // What stands before this mark on its line is the prompt. Written
                        // again while a command is being typed (the last line of a prompt
                        // of several), it was marked as typed.
                        let before = typed_before(term, id);
                        remark(term, &before, Some(id));
                        // Nothing is typed above the line a prompt ends on: what is marked
                        // so there is left of an earlier copy, on lines the terminal has
                        // wrapped anew.
                        let line = term.grid().cursor.point.line;
                        let typed = std::mem::take(&mut self.typed);
                        let reach = self.reach(term, typed);
                        forget_above(term, line, id, Some(Part::Input), reach);
                    }
                    tag(term, Some((id, Part::Input)));
                }
            },
            Mark::Output { command } => {
                tag(term, None);
                self.over = None;
                self.aside = false;
                let id = match (self.current, self.phase) {
                    (Some(id), Phase::Prompt | Phase::Input) => id,
                    (_, Phase::Running) => {
                        self.leave();
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
                // Only a command that runs can end. A shell whose prompt carries this
                // mark says it again whenever the prompt is written again, and that
                // changes nothing about the prompt.
                if self.phase == Phase::Running {
                    tag(term, None);
                    self.end(exit);
                } else if let Some(command) = self.over.take().and_then(|id| self.commands.get_mut(&id)) {
                    // the command a line of its own was asked for after: this is how it ended
                    command.exit = exit;
                    command.left = false;
                    self.ended += 1;
                    self.failed += command.failed() as u64;
                }
            },
            Mark::Notify(_) => {},
        }
    }
}

/// Gives the terminal another size. Narrower lines are more lines, and a
/// smaller screen has kept more of them above it: the log is told what the
/// size was, so that what is left of a prompt is still found above it.
pub fn resize<L: EventListener, S: Dimensions>(log: &Shared, term: &mut Term<L>, size: S) {
    log.lock().unwrap().resized(term.columns(), term.screen_lines());
    term.resize(size);
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
        scanner.fed = scanner.fed.saturating_add(end - done);
        done = end;
        // Inside a synchronized update the parser holds the text back until the
        // update ends. The mark belongs between that text, so the update is ended
        // here: in order matters more than all at once. A note has no place in the text.
        if parser.sync_bytes_count() > 0 && !matches!(mark, Mark::Notify(_)) {
            parser.stop_sync(term);
        }
        let mut log = log.lock().unwrap();
        log.wrote(std::mem::take(&mut scanner.fed));
        log.apply(mark, term);
    }
    parser.advance(term, &bytes[done..]);
    scanner.fed = scanner.fed.saturating_add(bytes.len() - done);
}

/// The link that marks a cell as this part of command `id`.
fn link_of(id: u32, part: Part) -> String {
    let part = match part {
        Part::Prompt => 'p',
        Part::Input => 'i',
        Part::Left => 'x',
    };
    format!("{SCHEME}{id}:{part}")
}

/// Makes what is written from here on cells of this command's own lines, or
/// plain cells again.
fn tag<L: EventListener>(term: &mut Term<L>, cells: Option<(u32, Part)>) {
    match cells {
        Some((id, part)) => {
            let uri = link_of(id, part);
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

/// The cells before the cursor on its line that are marked as typed at the
/// prompt of command `id`. A line that is full has the cursor waiting on its
/// last cell: that cell is before it too.
fn typed_before<L>(term: &Term<L>, id: u32) -> Vec<Column> {
    let cursor = &term.grid().cursor;
    let end = cursor.point.column.0 + usize::from(cursor.input_needs_wrap);
    let typed = link_of(id, Part::Input);
    let row = &term.grid()[cursor.point.line];
    (0..end).map(Column).filter(|&column| row[column].hyperlink().is_some_and(|link| link.uri() == typed)).collect()
}

/// Makes these cells of the cursor's line part of the prompt of command
/// `id`, or cells of no command.
fn remark<L>(term: &mut Term<L>, columns: &[Column], id: Option<u32>) {
    let link = id.map(|id| {
        let uri = link_of(id, Part::Prompt);
        alacritty_terminal::term::cell::Hyperlink::new(Some(uri.clone()), uri)
    });
    let line = term.grid().cursor.point.line;
    for &column in columns {
        term.grid_mut()[line][column].set_hyperlink(link.clone());
    }
}

/// The command a line belongs to as part of its prompt or of what was typed:
/// the number in the first mark of one of the two on it.
pub fn owner<L>(term: &Term<L>, line: Line) -> Option<u32> {
    marks_on(term, line).find(|&(_, part)| part != Part::Left).map(|(id, _)| id)
}

/// Whether something on `line` is left of a prompt that was written again.
fn left_over<L>(term: &Term<L>, line: Line) -> bool {
    marks_on(term, line).any(|(_, part)| part == Part::Left)
}

/// The marks on the cells of a line, from left to right.
fn marks_on<L>(term: &Term<L>, line: Line) -> impl Iterator<Item = (u32, Part)> + '_ {
    let within = line >= term.topmost_line() && line <= term.bottommost_line();
    within.then(|| &term.grid()[line]).into_iter().flatten().filter_map(|cell| cell_of(cell.hyperlink()?.uri()))
}

/// Marks as left over what is marked for command `id` on the lines above
/// `line`: all of it, or only what is marked as this `part`. They are the
/// lines of an earlier copy of its prompt, with what stood typed at it and
/// what the shell printed below it (a list of completions).
///
/// Lines that are marked for no command lie between the copies where a
/// program has printed with links of its own, or where the shell left lines
/// empty. They don't end the search. A line of another command does: the
/// command is older, and nothing of this one stands above it. Without one,
/// the search goes `reach` lines up: as far as a copy can have been moved
/// by what was written since. (A program that scrolls the screen again and
/// again while a prompt is open can move a copy further than that. Shells
/// don't.)
fn forget_above<L>(term: &mut Term<L>, line: Line, id: u32, part: Option<Part>, reach: usize) {
    let reach = i32::try_from(reach).unwrap_or(i32::MAX);
    let top = term.topmost_line().max(Line(line.0.saturating_sub(reach)));
    let columns = term.columns();
    let uri = link_of(id, Part::Left);
    let left = alacritty_terminal::term::cell::Hyperlink::new(Some(uri.clone()), uri);
    let mut above = line - 1;
    while above >= top {
        match owner(term, above) {
            Some(other) if other == id => {
                let row = &mut term.grid_mut()[above];
                for column in (0..columns).map(Column) {
                    let marked = row[column].hyperlink().and_then(|link| cell_of(link.uri()));
                    let wanted = |of: Part| of != Part::Left && part.is_none_or(|part| part == of);
                    if marked.is_some_and(|(other, of)| other == id && wanted(of)) {
                        row[column].set_hyperlink(Some(left.clone()));
                    }
                }
            },
            Some(_) => break,
            None => {},
        }
        above -= 1;
    }
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

/// The command that printed `line`, or whose own line it is. None for what
/// is left of a prompt that was written again: no command printed that.
pub fn printed<L>(term: &Term<L>, line: Line) -> Option<Place> {
    let top = term.topmost_line();
    let mut line = line;
    while line >= top {
        // each line is read once: this runs for every line up to the command's
        let mut left = false;
        for (id, part) in marks_on(term, line) {
            if part != Part::Left {
                let (first, last) = own_lines(term, line, id);
                return Some(Place { id, first, last });
            }
            left = true;
        }
        if left {
            return None;
        }
        line -= 1;
    }
    None
}

/// The nearest command above `line` that `wanted` says yes to.
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
/// prompt, or to above what is left of an earlier copy of that prompt,
/// without the empty lines at the end. None if it printed nothing.
pub fn output<L>(term: &Term<L>, place: Place) -> Option<(Line, Line)> {
    let start: Line = place.last + 1;
    let mut end = below(term, place.last, |_| true).map_or(term.bottommost_line(), |next| next.first - 1);
    if let Some(left) = (start.0..=end.0).map(Line).find(|&line| left_over(term, line)) {
        end = left - 1;
    }
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
        assert_eq!(marks(&[b"\x1b]133;L\x07"]), [(8, Mark::FreshLine)]);
        assert_eq!(marks(&[b"\x1b]133;D;130;aid=7\x07"]), [(18, Mark::End { exit: Some(130) })]);
    }

    #[test]
    fn a_prompt_says_its_kind() {
        let found = marks(&[b"\x1b]133;P;k=r\x07\x1b]133;P;k=c\x07\x1b]133;P;k=s\x07\x1b]133;P;k=i\x07\x1b]133;P\x07"]);
        let found: Vec<Mark> = found.into_iter().map(|(_, mark)| mark).collect();
        let (right, line) = (Mark::Aside { right: true }, Mark::Aside { right: false });
        assert_eq!(found, [right, line.clone(), line, Mark::Prompt, Mark::Prompt]);
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

        /// The window is made this many cells wide.
        fn resize(&mut self, cols: usize) -> &mut Screen {
            resize(&self.log, &mut self.term, TermSize { cols, rows: 8, cell_width: 8., cell_height: 16. });
            self
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

    /// How many commands have ended on a screen, and how many of them failed.
    fn ended(screen: &Screen) -> (u64, u64) {
        screen.log.lock().unwrap().ended()
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
        assert_eq!(log.ended().0, 2);
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
        // its line is erased and the prompt written again a line further down:
        // nothing of it is left above, so it has moved, and is the same prompt still
        screen.print("\x1b[2K\r\n\x1b]133;A\x07~> \x1b]133;B\x07");
        assert_eq!(screen.places(), [(0, 1, 1)]);
        // A line is sent off and no command is said to run: the prompt below it is the
        // same one still, and what is left above it is no prompt.
        screen.print("x\r\n\x1b]133;A\x07~> \x1b]133;B\x07");
        assert_eq!(screen.places(), [(0, 2, 2)]);
        assert_eq!(owner(&screen.term, Line(1)), None);
        assert_eq!(screen.command(0).status(), Status::Typed);
        // a prompt that begins with an empty line
        screen.print("\r\n\r\n\x1b]133;A\x07~> \x1b]133;B\x07");
        assert_eq!(screen.places(), [(0, 4, 4)]);
        // what is sent off at last is what was typed at the prompt as it stands
        screen.print("ls\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("ls"));
        assert_eq!(screen.places(), [(0, 4, 4)]);
    }

    /// The marks of bash 5.3 with lumen's lines read in, as they were captured.
    #[test]
    fn bash_writes_its_prompt_again() {
        let mut screen = Screen::new();
        // bash says how the last command ended in every prompt, also in one it writes again
        let prompt = "\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07";
        screen.print("\x1b]133;L\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        screen.print("true\r\n\x1b]133;C\x07\x1b]133;L\x07").print(prompt).print("ls");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 1, 1)]);
        assert_eq!(screen.command(0).status(), Status::Ended(Some(0)));
        // the window is resized: bash erases the line and writes it again
        screen.print("\r\x1b[K\r").print(prompt).print("ls");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 1, 1)]);
        // The screen is cleared. What stood on it is kept above, the prompt with
        // it. bash asked for no line of its own, so the prompt it writes on the
        // empty screen is the open one, and the copy above is a prompt no longer.
        screen.print("\x1b[H\x1b[2J").print(prompt).print("ls");
        assert_eq!(screen.places(), [(0, -2, -2), (1, 0, 0)]);
        assert_eq!((screen.line(-1), screen.line(0)), ("$ ls".to_string(), "$ ls".to_string()));
        // an empty line sent off, and Ctrl+C at the prompt: a new prompt each time
        screen.print("\x15\r\n\x1b]133;L\x07").print(prompt);
        screen.print("ls^C\r\n\x1b]133;L\x07\x1b]133;D;130\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        assert_eq!(screen.places()[1..], [(1, 0, 0), (2, 1, 1), (3, 2, 2)]);
        // none of that ended a command
        assert_eq!(ended(&screen), (1, 0));
        screen.print("ls\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(3).text.as_deref(), Some("ls"));
        assert_eq!(screen.command(1).status(), Status::Typed);
        // the command is over when bash asks for the prompt's line, and the prompt says how it ended
        screen.print("ls: no such thing\r\n\x1b]133;L\x07");
        assert!(screen.log.lock().unwrap().running().is_none());
        assert_eq!(ended(&screen), (1, 0));
        screen.print("\x1b]133;D;2\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        assert_eq!(screen.command(3).status(), Status::Ended(Some(2)));
        assert!(!screen.command(3).left);
        assert_eq!(ended(&screen), (2, 1));
    }

    /// bash 5.3 with a list of completions, as captured.
    #[test]
    fn a_list_of_completions_under_a_prompt() {
        let mut screen = Screen::new();
        let prompt = "\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07";
        screen.print("\x1b]133;L\x07\x1b]133;A\x07$ \x1b]133;B\x07ls foo");
        // the list is printed below the prompt, and the prompt written again below the list
        screen.print("\r\nfoo1    foo2    foobar\r\n").print(prompt).print("ls foo");
        // the prompt is where it was written last: its copy above and the list are no command
        assert_eq!(screen.places(), [(0, 2, 2)]);
        assert_eq!((owner(&screen.term, Line(0)), owner(&screen.term, Line(1))), (None, None));
        assert_eq!(screen.line(0), "$ ls foo");
        screen.print("bar\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("ls foobar"));
        // the list belongs to nothing that ran
        assert_eq!(above(&screen.term, Line(2), |_| true), None);
    }

    #[test]
    fn what_is_left_of_a_prompt_is_not_what_the_command_before_printed() {
        let mut screen = Screen::new();
        let prompt = "\x1b]133;L\x07\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07";
        screen.print(prompt).print("seq 2\r\n\x1b]133;C\x071\r\n2\r\n").print(prompt).print("ls foo");
        // a list of completions, and the prompt again below it
        let again = "\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07";
        screen.print("\r\nfoo1    foo2\r\n").print(again).print("ls foo");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 5, 5)]);
        let first = place_at(&screen.term, Line(0)).unwrap();
        assert_eq!(text(&screen.term, output(&screen.term, first).unwrap()), "1\n2");
        // the output was printed by the command, the earlier prompt and the list by none
        assert_eq!(printed(&screen.term, Line(2)).map(|place| place.id), Some(0));
        assert_eq!(printed(&screen.term, Line(3)), None);
        assert_eq!(printed(&screen.term, Line(4)), None);
        assert_eq!(printed(&screen.term, Line(5)).map(|place| place.id), Some(1));
        // going from command to command passes over it
        assert_eq!(above(&screen.term, Line(5), |_| true).map(|place| place.id), Some(0));
        // the same with a screen that was cleared: the prompt above it is no output
        let mut screen = Screen::new();
        screen.print(prompt).print("seq 2\r\n\x1b]133;C\x071\r\n2\r\n").print(prompt).print("ls");
        screen.print("\x1b[H\x1b[2J").print(again).print("ls");
        assert_eq!(screen.places(), [(0, -4, -4), (1, 0, 0)]);
        let first = place_at(&screen.term, Line(-4)).unwrap();
        assert_eq!(text(&screen.term, output(&screen.term, first).unwrap()), "1\n2");
    }

    /// bash 5.3 in a window that is made narrower, as captured.
    #[test]
    fn a_prompt_in_a_window_made_narrower() {
        let mut screen = Screen::new();
        let prompt = "\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07";
        let typed = "echo aaaaaaaaaaaaaaaaaaaa";
        screen.print("\x1b]133;L\x07\x1b]133;A\x07$ \x1b]133;B\x07").print(typed);
        // The terminal wraps the line anew, into two. bash knows of one: it erases
        // the one the cursor is on, and writes prompt and command again from there.
        screen.resize(15).print("\r\x1b[K\r").print(prompt).print(typed);
        assert_eq!(screen.line(-1), "$ echo aaaaaaaa");
        assert_eq!(screen.line(0), "$ echo aaaaaaaa");
        // what is left of the first copy is no prompt, and nothing typed
        assert_eq!(owner(&screen.term, Line(-1)), None);
        assert_eq!(screen.places(), [(0, 0, 1)]);
        screen.print("\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some(typed));
        // a prompt of two lines, of which bash writes only the last one again
        let mut screen = Screen::new();
        screen.print("\x1b]133;L\x07\x1b]133;A\x07~/code\r\r\n$ \x1b]133;B\x07").print(typed);
        screen.resize(15).print("\r\x1b[K\r$ \x1b]133;B\x07").print(typed).print("\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some(typed));
        // its first line, what is left of the second one, and the second one as it is now
        assert_eq!(screen.places(), [(0, -1, 2)]);
    }

    /// A window made much narrower below lines that took few bytes for their
    /// length: each of them is many rows now, and no byte was written for it.
    #[test]
    fn a_prompt_far_below_its_copy_after_lines_were_made_narrower() {
        let mut screen = Screen::new();
        let prompt = "\x1b]133;A\x07$ \x1b]133;B\x07";
        screen.print("\x1b]133;L\x07").print(prompt).print("cd fo");
        // something writes below the prompt, at the right end of each line, and tells of nothing
        for _ in 0..400 {
            screen.print("\r\n\x1b[28Cx");
        }
        screen.print("\r\n");
        // Three cells wide, every one of those lines is ten rows. The shell clears
        // what is below the cursor and writes its prompt again.
        screen.resize(3).print("\r\x1b[J").print(prompt).print("cd fo");
        let copy = (screen.term.topmost_line().0..0).find(|&line| screen.line(line) == "$ c").unwrap();
        assert_eq!((copy, owner(&screen.term, Line(copy))), (-3998, None));
        assert_eq!(screen.places(), [(0, 5, 7)]);
    }

    /// How far up what is left of a prompt is looked for.
    #[test]
    fn how_far_up_a_copy_can_stand() {
        let mut screen = Screen::new();
        let reach = |screen: &Screen, bytes| screen.log.lock().unwrap().reach(&screen.term, bytes);
        let counts = |screen: &Screen| {
            let log = screen.log.lock().unwrap();
            (log.written, log.typed)
        };
        screen.print("\x1b]133;L\x07\x1b]133;A\x07~/code\r\r\n$ \x1b]133;B\x07").print("ls");
        // two screens of eight lines and the cursor's own, and a line for each byte
        assert_eq!((reach(&screen, 0), reach(&screen, 100)), (17, 117));
        // A job in the background prints much while the prompt is open, and the
        // shell writes the end of its prompt again: what stands typed above it is
        // looked for as far up as was written since the last time.
        screen.print(&"x".repeat(5000)).print("\r\n$ \x1b]133;B\x07").print("ls");
        assert_eq!(counts(&screen), (5033, 0));
        // Narrower lines are more rows: 30 cells are up to three rows of 15 that
        // end with a wide character. Again narrower, they count from their longest.
        screen.resize(15);
        assert_eq!(reach(&screen, 100), 117 * 3);
        screen.resize(11);
        assert_eq!(reach(&screen, 100), 117 * 3);
        screen.resize(10);
        assert_eq!(reach(&screen, 100), 117 * 4);
        // as wide as they were, they are the rows they were
        screen.resize(30);
        assert_eq!(reach(&screen, 100), 117);
        // a smaller screen has kept the lines of the larger one above it
        resize(&screen.log, &mut screen.term, TermSize { cols: 30, rows: 4, cell_width: 8., cell_height: 16. });
        assert_eq!(reach(&screen, 100), 117);
        // a prompt from its beginning starts all of it anew: its own 19 bytes are all there is
        screen.print("\r\x1b[A\x1b[J\x1b]133;A\x07~/code\r\r\n$ \x1b]133;B\x07");
        assert_eq!((counts(&screen), reach(&screen, 0)), ((19, 0), 9));
    }

    /// zsh 5.9 with a prompt at the right, as captured.
    #[test]
    fn a_prompt_at_the_right_of_the_line() {
        let mut screen = Screen::new();
        screen.print("\x1b]133;L\x07\r\x1b[J\x1b]133;A\x07% \x1b]133;B\x07\x1b[K");
        screen.print("\x1b[20C\x1b]133;P;k=r\x07[right]\x1b]133;B\x07\x1b[27D").print("ls");
        assert_eq!(screen.line(0), "% ls                  [right]");
        // a command too long for zsh to name is read from the screen: without the prompt beside it
        screen.print("\r\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("ls"));
        // the right prompt is on a line of its own once the window is narrower, and still the command's
        screen.print("a\r\n\x1b]133;D;0\x07").resize(20);
        assert_eq!((screen.line(-1), screen.line(0)), ("% ls".to_string(), "  [right]".to_string()));
        assert_eq!(screen.places(), [(0, -1, 0)]);
        let first = place_at(&screen.term, Line(0)).unwrap();
        assert_eq!(text(&screen.term, output(&screen.term, first).unwrap()), "a");
    }

    #[test]
    fn a_command_of_several_lines_with_a_prompt_on_each() {
        let mut screen = Screen::new();
        // bash, whose further lines begin with PS2
        let more = "\x1b]133;P;k=c\x07> \x1b]133;B\x07";
        screen.print("\x1b]133;L\x07\x1b]133;A\x07$ \x1b]133;B\x07for x in 1 2\r\n");
        screen.print(more).print("do echo $x\r\n").print(more);
        // the line that is open is written again, as after a resize
        screen.print("\r\x1b[K\r").print(more).print("done\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("for x in 1 2\ndo echo $x\ndone"));
        assert_eq!(screen.places(), [(0, 0, 2)]);
        // a prompt of that kind with no prompt open says nothing
        let mut screen = Screen::new();
        screen.print(more).print("x");
        assert_eq!(screen.places(), []);
        // readline's mode, which it writes before the prompt of every line, is not typed either
        let mut screen = Screen::new();
        let more = "(ins)\x1b]133;P;k=c\x07> \x1b]133;B\x07";
        screen.print("\x1b]133;L\x07(ins)\x1b]133;A\x07$ \x1b]133;B\x07for x in 1 2\r\n");
        screen.print(more).print("do echo $x\r\n").print(more).print("done\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("for x in 1 2\ndo echo $x\ndone"));
    }

    /// bash with lumen's lines, started in fish and left with Ctrl+D: in fish 4.9,
    /// and in an older fish with lumen's lines, whose mark for a prompt is bash's.
    #[test]
    fn a_shell_around_one_that_ended_at_its_prompt() {
        for fish in ["\x1b]133;A;click_events=1\x07> \x1b]133;B\x07", "\x1b]133;A\x07> "] {
            let mut screen = Screen::new();
            screen.print(fish).print("bash\r\n\x1b]133;C;cmdline_url=bash\x07");
            screen.print("\x1b]133;L\x07\x1b]133;A\x07$ \x1b]133;B\x07exit\r\n");
            // fish says how bash ended and writes its prompt, and once more after an empty
            // line. Nothing ran at the prompt bash left open: it is the open prompt still,
            // where fish has written its own.
            screen.print("\x1b]133;D;0\x07").print(fish).print("\r\n").print(fish);
            assert_eq!(screen.places(), [(0, 0, 0), (1, 3, 3)]);
            // what bash left behind is not what `bash` printed
            let outer = place_at(&screen.term, Line(0)).unwrap();
            assert_eq!(output(&screen.term, outer), None);
            // a command in fish is where it was typed, and the prompt after it is a new one
            screen.print("ls\r\n\x1b]133;C;cmdline_url=ls\x07a\r\n\x1b]133;D;0\x07").print(fish);
            assert_eq!(screen.places(), [(0, 0, 0), (1, 3, 3), (2, 5, 5)]);
            assert_eq!(screen.command(1).text.as_deref(), Some("ls"));
            assert_eq!(screen.command(1).status(), Status::Ended(Some(0)));
            let ran = place_at(&screen.term, Line(3)).unwrap();
            assert_eq!(text(&screen.term, output(&screen.term, ran).unwrap()), "a");
        }
    }

    /// fish 4.9, which asks for no line of its own before a prompt, as captured.
    #[test]
    fn a_shell_that_does_not_say_when_a_prompt_is_new() {
        let mut screen = Screen::new();
        let prompt = "\x1b]133;A\x07> \x1b]133;B\x07";
        screen.print(prompt).print("echo abc");
        // the screen is cleared: what stood on it is kept above, and the prompt is written again
        screen.print("\x1b[H\x1b[2J").print(prompt).print("echo abc");
        assert_eq!(screen.places(), [(0, 0, 0)]);
        assert_eq!(screen.line(-1), "> echo abc");
        // an empty line sent off: nothing ran, so the prompt below it is the same one still
        screen.print("\x15\r\n").print(prompt);
        assert_eq!(screen.places(), [(0, 1, 1)]);
        // a command, and the screen cleared by one: the prompt after it is a new one
        screen.print("clear\r\n\x1b]133;C\x07\x1b[H\x1b[2J\x1b]133;D;0\x07").print(prompt);
        assert_eq!(screen.places().last(), Some(&(1, 0, 0)));
        assert_eq!(screen.command(0).text.as_deref(), Some("clear"));
    }

    /// fish with a key that lists the folder under the prompt, by an `ls` that
    /// names files with links.
    #[test]
    fn a_prompt_below_what_was_printed_with_links_of_its_own() {
        let mut screen = Screen::new();
        let prompt = "\x1b]133;A;click_events=1\x07> \x1b]133;B\x07";
        screen.print(prompt).print("true\r\n\x1b]133;C\x07\x1b]133;D;0\x07").print(prompt).print("cd fo");
        // After the first name's link has ended, the listing is marked for nothing.
        // fish writes its prompt again below it.
        screen.print("\r\n\x1b]8;;file:///foo1\x07foo1\x1b]8;;\x07  foo2\r\nfoo3\r\n").print(prompt).print("cd fo");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 4, 4)]);
        assert_eq!(owner(&screen.term, Line(1)), None);
        // the listing was printed by no command, and is not what `true` printed
        assert_eq!(printed(&screen.term, Line(3)), None);
        assert_eq!(output(&screen.term, place_at(&screen.term, Line(0)).unwrap()), None);
        // the line is emptied and sent off, and a command typed: it is where it stands
        screen.print("\x15\r\n").print(prompt).print("ls\r\n\x1b]133;C\x07");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 5, 5)]);
        assert_eq!(screen.command(1).text.as_deref(), Some("ls"));
        // empty lines between two copies of a prompt, more than a prompt begins with
        let mut screen = Screen::new();
        screen.print(prompt).print("\r\n\r\n\r\n\r\n").print(prompt);
        assert_eq!(screen.places(), [(0, 4, 4)]);
        // A listing of more lines than a few screens are high, and empty lines sent
        // off after it: the copy above the listing is found still, however far up.
        let mut screen = Screen::new();
        screen.print(prompt).print("cd fo\r\n\x1b]8;;file:///foo1\x07foo1\x1b]8;;\x07\r\n");
        for _ in 0..40 {
            screen.print("name\r\n");
        }
        screen.print(prompt).print("cd fo");
        for _ in 0..12 {
            screen.print("\x15\r\n").print(prompt);
        }
        screen.print("ls\r\n\x1b]133;C\x07");
        assert_eq!(screen.places(), [(0, 6, 6)]);
        assert_eq!(screen.command(0).text.as_deref(), Some("ls"));
        // a prompt of another command ends the search: it is that command's still
        let mut screen = Screen::new();
        screen
            .print(prompt)
            .print("true\r\n\x1b]133;C\x07a\r\n\x1b]133;D;0\x07")
            .print(prompt)
            .print("\r\n")
            .print(prompt);
        assert_eq!(screen.places(), [(0, 0, 0), (1, 3, 3)]);
        assert_eq!(text(&screen.term, output(&screen.term, place_at(&screen.term, Line(0)).unwrap()).unwrap()), "a");
    }

    /// fish 4.9 with a prompt of two lines, in a window that is made narrower, as captured.
    #[test]
    fn a_prompt_of_two_lines_that_is_written_again_from_its_second() {
        let mut screen = Screen::new();
        let prompt = "\x1b]133;A;click_events=1\x07~\r\n> \x1b]133;B\x07";
        let typed = "echo aaaaaaaaaaaaaaaaaaaa";
        screen.print("\x1b]133;A\x07> \x1b]133;B\x07true\r\n\x1b]133;C\x07\x1b]133;D;0\x07");
        screen.print(prompt).print(typed);
        assert_eq!(screen.places(), [(0, 0, 0), (1, 1, 2)]);
        // The terminal wraps the prompt's second line anew, into two. fish knows of one:
        // it goes up one line, which is that second line now, and writes all of its
        // prompt from there. Its first line is left above.
        screen.resize(20).print("\r\r\x1b[A\x1b[K").print(prompt).print(typed).print("\x1b[J\r\x1b[7C");
        // (wrapped anew, the lines have moved up by one)
        let lines: Vec<String> = (-1..4).map(|line| screen.line(line)).collect();
        assert_eq!(lines, ["> true", "~", "~", "> echo aaaaaaaaaaaaa", "aaaaaaa"]);
        // one prompt still, where it stands now: the line left above is none
        assert_eq!(screen.places(), [(0, -1, -1), (1, 1, 3)]);
        assert_eq!(owner(&screen.term, Line(0)), None);
        screen.print("\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(1).text.as_deref(), Some(typed));
        // and that line is not what the command before printed
        let first = above(&screen.term, Line(1), |_| true).unwrap();
        assert_eq!((first.id, output(&screen.term, first)), (0, None));
    }

    #[test]
    fn a_prompt_that_lost_its_marks() {
        let mut screen = Screen::new();
        // bash, where a theme sets the prompt anew after lumen's lines: no A, no B, no D
        screen.print("\x1b]133;L\x07$ sleep 1\r\n\x1b]133;C\x07");
        assert!(screen.log.lock().unwrap().running().is_some());
        screen.print("\x1b]133;L\x07$ ");
        // nothing is left running, and nobody is told of an end that was not said
        assert!(screen.log.lock().unwrap().running().is_none());
        assert_eq!(ended(&screen), (0, 0));
        assert!(screen.command(0).left);
        // a status that comes later is not this command's
        screen.print("true\r\n\x1b]133;C\x07\x1b]133;D;1\x07");
        assert_eq!(screen.command(0).status(), Status::Ended(None));
        assert_eq!(screen.command(1).status(), Status::Ended(Some(1)));
    }

    #[test]
    fn what_readline_puts_before_a_prompt() {
        let mut screen = Screen::new();
        // bash with `show-mode-in-prompt`: the mode stands before all of the prompt
        screen.print("\x1b]133;L\x07(ins)\x1b]133;A\x07$ \x1b]133;B\x07");
        screen.print("printf hello\r\n\x1b]133;C\x07hello");
        screen.print("\x1b]133;L\x07(ins)\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        assert_eq!(screen.line(1), "hello");
        assert_eq!(screen.line(2), "(ins)$");
        let first = place_at(&screen.term, Line(0)).unwrap();
        assert_eq!(text(&screen.term, output(&screen.term, first).unwrap()), "hello");
        screen.print("ls");
        // the mode changes: the prompt is written again where it stands, twice
        screen.print("\r(cmd)\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07\x1b[C");
        screen.print("\r(ins)\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07\x1b[C");
        // and after a resize
        screen.print("\r\x1b[K\r(ins)\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07ls");
        assert_eq!(screen.line(2), "(ins)$ ls");
        assert_eq!(screen.line(3), "");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 2, 2)]);
        // the mode is not part of what was typed
        screen.print("\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(1).text.as_deref(), Some("ls"));
    }

    /// More of bash 5.3 with `show-mode-in-prompt`, as captured: prompts that follow one no command was run at.
    #[test]
    fn readlines_mode_is_not_typed_at_the_prompt_before() {
        let mut screen = Screen::new();
        screen.print("\x1b]133;L\x07(ins)\x1b]133;A\x07$ \x1b]133;B\x07");
        // an empty line sent off, then Ctrl+C on a line with something typed
        screen.print("\r\n\x1b]133;L\x07(ins)\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        screen.print("ls^C\r\r\n\x1b]133;L\x07(ins)\x1b]133;D;130\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        screen.print("printf hi\r\n\x1b]133;C\x07hi");
        // each prompt is on a line of its own command, and the last one was run
        assert_eq!(screen.places(), [(0, 0, 0), (1, 1, 1), (2, 2, 2)]);
        assert_eq!(screen.command(2).text.as_deref(), Some("printf hi"));
        assert_eq!(screen.command(2).status(), Status::Running);

        // emacs mode, where the mode is `@`, and the screen cleared with a command typed
        let mut screen = Screen::new();
        screen.print("\x1b]133;L\x07@\x1b]133;A\x07$ \x1b]133;B\x07true\r\n\x1b]133;C\x07");
        screen.print("\x1b]133;L\x07@\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07ls");
        screen.print("\x1b[H\x1b[2J@\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07ls");
        // the prompt on the cleared screen is the open one, as it is without the mode
        assert_eq!(screen.places(), [(0, -2, -2), (1, 0, 0)]);
        screen.print("\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(1).text.as_deref(), Some("ls"));
    }

    #[test]
    fn a_prompt_as_wide_as_the_screen_written_again() {
        let mut screen = Screen::new();
        // 30 cells: the cursor waits on the last one
        let prompt = "~/a/folder/with/a/long/name =>";
        screen.print("\x1b]133;L\x07\x1b]133;A\x07").print(prompt).print("\x1b]133;B\x07ls");
        assert_eq!(screen.line(1), "ls");
        // written again from its start while `ls` stands typed
        screen.print("\x1b[A\r").print(prompt).print("\x1b]133;B\x07ls -l\r\n\x1b]133;C\x07");
        assert_eq!(screen.places(), [(0, 0, 1)]);
        assert_eq!(screen.command(0).text.as_deref(), Some("ls -l"));
    }

    #[test]
    fn the_last_line_of_a_prompt_written_again() {
        let mut screen = Screen::new();
        // a prompt of two lines: after a resize bash writes only the second one again
        screen.print("\x1b]133;L\x07\x1b]133;A\x07~/code\r\r\n$ \x1b]133;B\x07ls");
        screen.print("\r\x1b[K\r$ \x1b]133;B\x07ls -l");
        assert_eq!(screen.places(), [(0, 0, 1)]);
        screen.print("\r\n\x1b]133;C\x07");
        assert_eq!(screen.command(0).text.as_deref(), Some("ls -l"));
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
        assert_eq!(screen.command(0).describe().unwrap().split_once(" before ").unwrap().1, "the next prompt");
        assert_eq!(screen.places(), [(0, 0, 0), (1, 1, 1)]);
        // nobody saw it end, so nobody is told that it has
        assert_eq!(screen.log.lock().unwrap().ended(), (0, 0));
        // an end without a command before it says nothing
        let mut screen = Screen::new();
        screen.print("\x1b]133;D;0\x07\x1b]133;A\x07~> ");
        assert_eq!(screen.log.lock().unwrap().ended(), (0, 0));
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
    fn a_prompt_begins_on_a_line_of_its_own() {
        let mut screen = Screen::new();
        // bash writes its prompt where the output stopped, and asks for a line of its own first
        let next = "\x1b]133;L\x07\x1b]133;D;0\x07\x1b]133;A\x07";
        screen.print("\x1b]133;L\x07\x1b]133;A\x07~> \x1b]133;B\x07printf hello\r\n\x1b]133;C\x07hello");
        screen.print(next).print("~> \x1b]133;B\x07printf 'a\\nb'\r\n\x1b]133;C\x07a\r\nb");
        screen.print(next).print("$ \x1b]133;B\x07");
        assert_eq!(screen.line(1), "hello");
        assert_eq!(screen.line(2), "~> printf 'a\\nb'");
        assert_eq!(screen.line(4), "b");
        assert_eq!(screen.line(5), "$");
        let first = place_at(&screen.term, Line(0)).unwrap();
        assert_eq!(text(&screen.term, output(&screen.term, first).unwrap()), "hello");
        let second = place_at(&screen.term, Line(2)).unwrap();
        assert_eq!(text(&screen.term, output(&screen.term, second).unwrap()), "a\nb");
        // A prompt's own mark moves nothing: what a shell has written before it on
        // its line stays there, and is the shell's to see to.
        screen.print("ls\r\n\x1b]133;C\x07x\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07");
        assert_eq!(screen.line(6), "x$");
        assert_eq!(screen.line(7), "");
    }

    #[test]
    fn several_commands_end_and_one_of_them_fails() {
        let mut screen = Screen::new();
        screen.run("false", "", 1).run("true", "", 0);
        assert_eq!(screen.log.lock().unwrap().ended(), (2, 1));
        // stopped from the keyboard is not a failure
        screen.run("sleep 9", "^C", 130);
        assert_eq!(screen.log.lock().unwrap().ended(), (3, 1));
    }

    #[test]
    fn a_shell_inside_a_command_takes_over() {
        let mut screen = Screen::new();
        // `ssh host` runs, and the shell over there shows its prompt and runs a command
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07ssh host\r\n\x1b]133;C\x07Welcome\r\n");
        screen.run("ls", "a  b\r\n", 0);
        let outer = screen.command(0);
        assert_eq!(outer.status(), Status::Ended(None));
        assert!(outer.left && !outer.failed());
        // only the command over there ended for all to see
        assert_eq!(screen.log.lock().unwrap().ended(), (1, 0));
        assert_eq!(screen.places().len(), 2);
    }

    #[test]
    fn a_full_screen_program_has_no_commands() {
        let mut screen = Screen::new();
        screen.run("vim", "\x1b[?1049h\x1b]133;A\x07~> \x1b]133;B\x07x\x1b]133;C\x07\x1b]133;D;1\x07\x1b[?1049l", 0);
        assert_eq!(screen.places(), [(0, 0, 0)]);
        assert_eq!(screen.command(0).status(), Status::Ended(Some(0)));
        assert_eq!(screen.log.lock().unwrap().ended().0, 1);
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
    fn a_program_has_something_to_tell() {
        let note = |title: &str, body: &str| Mark::Notify(Note { title: title.into(), body: body.into() });
        // what `pulse run` says at the end of a long run
        let found = marks(&[b"done\r\n\x1b]777;notify;cargo test;exit 0 after 48.2 s, peak 8.9 GB\x1b\\"]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, note("cargo test", "exit 0 after 48.2 s, peak 8.9 GB"));
        // a text of its own may hold the sign that separates the fields
        let found = marks(&[b"\x1b]777;notify;build;", b"2 errors; 1 warning\x07"]);
        assert_eq!(found[0].1, note("build", "2 errors; 1 warning"));
        // a text alone
        assert_eq!(marks(&[b"\x1b]9;Tests are through\x07"])[0].1, note("", "Tests are through"));
        assert_eq!(marks(&[b"\x1b]777;notify;Only a title\x07"])[0].1, note("Only a title", ""));
        // not for the user: a progress bar and a folder as ConEmu says them, other
        // kinds of OSC 777, and notes with nothing in them
        for other in [
            &b"\x1b]9;4;1;50\x07"[..],
            b"\x1b]9;9;/home\x07",
            b"\x1b]9;12\x07",
            b"\x1b]9;\x07",
            b"\x1b]777;precmd\x07",
            b"\x1b]777;notify;;\x07",
            b"\x1b]99;i=1;Hello\x07",
        ] {
            assert_eq!(marks(&[other]), [], "{}", String::from_utf8_lossy(other));
        }
        // what a notification can't show is left out
        let found = marks(&["\x1b]777;notify;a\tb\u{9b}c;  line\none  \x07".as_bytes()]);
        assert_eq!(found[0].1, note("abc", "lineone"));
        let long = format!("\x1b]9;{}\x07", "é".repeat(1000));
        match &marks(&[long.as_bytes()])[0].1 {
            Mark::Notify(note) => assert_eq!(note.body.chars().count(), 400),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn notes_wait_to_be_taken() {
        let mut screen = Screen::new();
        screen.print("\x1b]777;notify;one;first\x07 text \x1b]9;second\x07");
        // the text around a note is written as if the note were not there
        assert_eq!(screen.line(0), " text");
        let notes = screen.log.lock().unwrap().take_notes();
        assert_eq!(notes.iter().map(|note| note.body.as_str()).collect::<Vec<_>>(), ["first", "second"]);
        assert_eq!(screen.log.lock().unwrap().take_notes(), []);
        // a full-screen program is heard too
        screen.print("\x1b[?1049h\x1b]9;from the other screen\x07\x1b[?1049l");
        assert_eq!(screen.log.lock().unwrap().take_notes().len(), 1);
        // only the last few are kept for someone who never asks
        for n in 0..10 {
            screen.print(&format!("\x1b]9;note {n}\x07"));
        }
        let notes = screen.log.lock().unwrap().take_notes();
        assert_eq!(notes.len(), MAX_NOTES);
        assert_eq!(notes.last().unwrap().body, "note 9");
    }

    #[test]
    fn a_command_keeps_the_most_memory_it_was_seen_with() {
        let mut screen = Screen::new();
        // nothing runs: nothing to keep it for
        screen.log.lock().unwrap().sample(1 << 30);
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07cargo build\r\n\x1b]133;C\x07");
        for memory in [2u64 << 30, 9 << 30, 3 << 30] {
            screen.log.lock().unwrap().sample(memory);
        }
        // while it runs, what it uses now is the word, not what it used at most
        assert_eq!(screen.command(0).peak(), None);
        screen.print("\x1b]133;D;0\x07");
        screen.log.lock().unwrap().sample(1 << 30);
        assert_eq!(screen.command(0).peak(), Some(9 << 30));
        // a shell's own few megabytes are not worth a word
        screen.print("\x1b]133;A\x07~> \x1b]133;B\x07true\r\n\x1b]133;C\x07");
        screen.log.lock().unwrap().sample(40 << 20);
        screen.print("\x1b]133;D;0\x07");
        assert_eq!(screen.command(1).peak(), None);
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
        assert_eq!(cell_of("lumen-mark:3:x"), Some((3, Part::Left)));
        assert_eq!(cell_of("lumen-mark:7"), None);
        assert_eq!(cell_of("lumen-mark:x:p"), None);
        assert_eq!(cell_of("lumen-image:7:0:0"), None);
        assert_eq!(cell_of("https://example.com"), None);
    }
}
