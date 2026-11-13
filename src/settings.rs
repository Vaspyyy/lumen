//! `lumen settings`: lumen's settings in a menu, in the terminal. Each change is
//! written to `lumen.toml` at once, and a running lumen takes it on as it does any
//! save of the file, so the window behind the menu shows it. What the file says
//! besides (your comments, the order, the settings the menu doesn't show) stays.

use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use toml_edit::DocumentMut;

use crate::config::{Config, TabFolder};

pub const USAGE: &str = "\
SETTINGS
    lumen settings                      change lumen's settings in a menu; each change
                                        shows at once. /settings at a prompt opens it
";

/// Runs the command if `args` is one, and gives its exit status.
pub fn run(args: &[String]) -> Option<i32> {
    let (command, rest) = args.split_first()?;
    if command != "settings" {
        return None;
    }
    if let Some(extra) = rest.first() {
        eprintln!("lumen: settings takes nothing more, not {extra:?} (see lumen --help)");
        return Some(2);
    }
    // SAFETY: isatty on the process's own descriptors
    if unsafe { libc::isatty(0) == 0 || libc::isatty(1) == 0 } {
        eprintln!("lumen: settings needs a terminal; the settings are in {}", Config::path().display());
        return Some(1);
    }
    let mut menu = match Menu::open() {
        Ok(menu) => menu,
        Err(why) => {
            eprintln!("lumen: {why}");
            return Some(1);
        },
    };
    let raw = match Raw::enter() {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!("lumen: can't take the terminal for the menu: {e}");
            return Some(1);
        },
    };
    menu.run();
    drop(raw);
    Some(0)
}

/// What kind of value a setting has, and so how it is changed.
#[derive(Clone, Copy)]
enum Kind {
    Bool,
    /// One of these words.
    Choice(&'static [&'static str]),
    /// A number, changed in steps, kept between the two ends.
    Number {
        step: f64,
        min: f64,
        max: f64,
        decimals: usize,
    },
    /// Any text: a font's name.
    Text,
    /// `new_tab_folder`: current, home, `/`, or a folder typed in.
    Folder,
}

/// What a setting is now.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Bool(bool),
    Number(f64),
    Text(String),
}

struct Item {
    /// The table the setting is in (`font`), or none for one at the top.
    table: Option<&'static str>,
    key: &'static str,
    label: &'static str,
    help: &'static str,
    kind: Kind,
    get: fn(&Config) -> Value,
}

/// A heading, or a setting under it.
enum Row {
    Heading(&'static str),
    Setting(Item),
}

fn rows() -> Vec<Row> {
    use Kind::*;
    use Row::{Heading, Setting};
    let item = |table, key, label, help, kind, get| Setting(Item { table, key, label, help, kind, get });
    vec![
        Heading("Tabs"),
        item(
            None,
            "new_tab_folder",
            "New tabs open in",
            "current: the folder of the tab you are in. Or home, /, or a folder you type in (Enter).",
            Folder,
            |c| Value::Text(c.new_tab_folder.written().to_string()),
        ),
        item(
            None,
            "launch",
            "Starting lumen again opens a",
            "What lumen does when you start it while one runs: a tab, a window, or a lumen of its own.",
            Choice(&["tab", "window", "process"]),
            |c| {
                Value::Text(
                    match c.launch {
                        crate::config::Launch::Tab => "tab",
                        crate::config::Launch::Window => "window",
                        crate::config::Launch::Process => "process",
                    }
                    .into(),
                )
            },
        ),
        item(
            None,
            "prewarm",
            "Start a shell ahead of time",
            "Keeps one shell started in the background, so that a new tab is there at once.",
            Bool,
            |c| Value::Bool(c.prewarm),
        ),
        Heading("Font"),
        item(
            Some("font"),
            "family",
            "Family",
            "The font's name. monospace is the system's fixed-width font. Enter to type one.",
            Text,
            |c| Value::Text(c.font.family.clone()),
        ),
        item(
            Some("font"),
            "size",
            "Size",
            "In points, as in your desktop's settings.",
            Number { step: 0.5, min: 4., max: 96., decimals: 1 },
            |c| Value::Number(c.font.size.into()),
        ),
        item(
            Some("font"),
            "line_height",
            "Line height",
            "Row height as a multiple of the font's: 1.2 gives the lines more air.",
            Number { step: 0.05, min: 0.8, max: 3., decimals: 2 },
            |c| Value::Number(c.font.line_height.into()),
        ),
        Heading("Window"),
        item(
            Some("window"),
            "opacity",
            "Opacity",
            "Of the background: 1 is solid, lower lets the desktop show through.",
            Number { step: 0.05, min: 0., max: 1., decimals: 2 },
            |c| Value::Number(c.window.opacity.into()),
        ),
        item(
            Some("window"),
            "blur",
            "Blur behind",
            "Blur what is behind a see-through window, where the desktop can.",
            Bool,
            |c| Value::Bool(c.window.blur),
        ),
        item(
            Some("window"),
            "padding",
            "Padding",
            "Space around a pane's text, in pixels.",
            Number { step: 1., min: 0., max: 200., decimals: 0 },
            |c| Value::Number(c.window.padding.into()),
        ),
        Heading("Cursor"),
        item(
            Some("cursor"),
            "style",
            "Shape",
            "A block, a thin beam, or a line under the character.",
            Choice(&["block", "beam", "underline"]),
            |c| {
                Value::Text(
                    match c.cursor.style {
                        crate::config::CursorShape::Block => "block",
                        crate::config::CursorShape::Beam => "beam",
                        crate::config::CursorShape::Underline => "underline",
                    }
                    .into(),
                )
            },
        ),
        item(Some("cursor"), "blink", "Blink", "The cursor dims and comes back.", Bool, |c| {
            Value::Bool(c.cursor.blink)
        }),
        item(Some("cursor"), "glow", "Glow", "The cursor gives off light.", Bool, |c| Value::Bool(c.cursor.glow)),
        item(Some("cursor"), "smooth", "Glide", "The cursor slides to where it goes instead of jumping.", Bool, |c| {
            Value::Bool(c.cursor.smooth)
        }),
        Heading("Behavior"),
        item(
            None,
            "copy_on_select",
            "Copy on select",
            "Selecting text copies it, as well as putting it in the middle-click selection.",
            Bool,
            |c| Value::Bool(c.copy_on_select),
        ),
        item(
            None,
            "notifications",
            "Notifications",
            "A program may post a notification on the desktop, for a tab you aren't looking at.",
            Bool,
            |c| Value::Bool(c.notifications),
        ),
        item(
            None,
            "scrollback",
            "Scrollback lines",
            "How many lines are kept above the screen.",
            Number { step: 1000., min: 0., max: 1_000_000., decimals: 0 },
            |c| Value::Number(c.scrollback as f64),
        ),
        item(
            Some("pulse"),
            "enabled",
            "Machine load",
            "Show what pulse says: the light in the title bar, and what each tab uses.",
            Bool,
            |c| Value::Bool(c.pulse.enabled),
        ),
    ]
}

/// A line being typed: a font's name, a folder.
struct Typing {
    text: String,
}

struct Menu {
    rows: Vec<Row>,
    /// The file the settings are written to: a link's target, so that a link (into a
    /// folder of dotfiles, say) stays a link.
    path: PathBuf,
    /// The file as it was last read or written: one changed by hand since is read again
    /// before a change, so that the change doesn't write over it.
    text: String,
    doc: DocumentMut,
    /// The row the cursor is on: always a setting.
    at: usize,
    /// The first row on screen, for a list longer than the terminal.
    top: usize,
    typing: Option<Typing>,
    /// What the last change did, or why it didn't.
    said: Option<(String, bool)>,
    accent: (u8, u8, u8),
}

impl Menu {
    fn open() -> Result<Menu, String> {
        // the file a link points at, so that the link stays, also where it isn't made yet
        let path = followed(Config::path());
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("can't read {}: {e}", path.display())),
        };
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|e| format!("{} isn't valid TOML, so the menu can't change it: {e}", path.display()))?;
        let accent = toml::from_str::<Config>(&text).unwrap_or_default().colors.accent;
        let rows = rows();
        let at = rows.iter().position(|row| matches!(row, Row::Setting(_))).unwrap_or(0);
        Ok(Menu { rows, path, text, doc, at, top: 0, typing: None, said: None, accent: (accent.0, accent.1, accent.2) })
    }

    fn run(&mut self) {
        let mut size = terminal_size();
        self.draw(size);
        loop {
            let mut pending = [libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 }];
            // SAFETY: poll on one descriptor this array holds
            let ready = unsafe { libc::poll(pending.as_mut_ptr(), 1, 250) };
            let now = terminal_size();
            if ready <= 0 {
                if now != size {
                    size = now;
                    self.draw(size);
                }
                continue;
            }
            let mut buf = [0u8; 256];
            // SAFETY: read into a buffer of its own length
            let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                return;
            }
            let keys = parse_keys(&buf[..n as usize]);
            for key in keys {
                if !self.key(key) {
                    return;
                }
            }
            size = now;
            self.draw(size);
        }
    }

    fn item(&self) -> &Item {
        match &self.rows[self.at] {
            Row::Setting(item) => item,
            Row::Heading(_) => unreachable!("the cursor is always on a setting"),
        }
    }

    /// What the settings are now, the file's and the defaults.
    fn config(&self) -> Config {
        toml::from_str(&self.doc.to_string()).unwrap_or_default()
    }

    /// Whether the file itself sets this one (not left to the default).
    fn is_set(&self, item: &Item) -> bool {
        match item.table {
            Some(table) => self.doc.get(table).and_then(|t| t.get(item.key)).is_some(),
            None => self.doc.get(item.key).is_some(),
        }
    }

    /// Handles one key; false to leave.
    fn key(&mut self, key: Key) -> bool {
        if let Some(typing) = &mut self.typing {
            match key {
                Key::Char(c) => typing.text.push(c),
                Key::Space => typing.text.push(' '),
                Key::Backspace => {
                    typing.text.pop();
                },
                Key::ClearLine => typing.text.clear(),
                Key::Enter => {
                    let text = typing.text.trim().to_string();
                    self.typing = None;
                    self.typed(text);
                },
                Key::Escape | Key::Quit => self.typing = None,
                _ => {},
            }
            return true;
        }
        match key {
            Key::Quit | Key::Escape | Key::Char('q') => return false,
            Key::Up | Key::Char('k') => self.step_cursor(-1),
            Key::Down | Key::Char('j') => self.step_cursor(1),
            Key::Left | Key::Char('h') => self.change(-1),
            Key::Right | Key::Char('l') => self.change(1),
            Key::Enter | Key::Space => self.enter(),
            Key::Char('r') => self.reset(),
            _ => {},
        }
        true
    }

    fn step_cursor(&mut self, by: isize) {
        let mut at = self.at as isize;
        loop {
            at += by;
            if at < 0 || at >= self.rows.len() as isize {
                return;
            }
            if matches!(self.rows[at as usize], Row::Setting(_)) {
                self.at = at as usize;
                return;
            }
        }
    }

    /// ← and →: the next value one way or the other.
    fn change(&mut self, by: isize) {
        let item = self.item();
        let now = (item.get)(&self.config());
        let next = match (item.kind, now) {
            (Kind::Bool, Value::Bool(on)) => toml_edit::value(!on),
            (Kind::Choice(words), Value::Text(word)) => toml_edit::value(cycle(words, &word, by)),
            (Kind::Number { step, min, max, decimals }, Value::Number(n)) => {
                number(((n / step).round() + by as f64) * step, min, max, decimals)
            },
            (Kind::Folder, Value::Text(written)) => {
                let mut words = vec!["current", "home", "/"];
                if !words.contains(&written.as_str()) {
                    words.push(&written);
                }
                toml_edit::value(cycle(&words, &written, by))
            },
            // a font's name has no next: Enter types one
            _ => return,
        };
        self.set(next);
    }

    /// Enter: a switch is flipped, a choice goes on, and text or a folder is typed.
    fn enter(&mut self) {
        let item = self.item();
        let now = (item.get)(&self.config());
        match (item.kind, now) {
            (Kind::Bool | Kind::Choice(_), _) => self.change(1),
            (Kind::Number { decimals, .. }, Value::Number(n)) => {
                self.typing = Some(Typing { text: format!("{n:.decimals$}") })
            },
            (Kind::Folder, Value::Text(written)) => {
                let text = if matches!(written.as_str(), "current" | "home") { String::new() } else { written };
                self.typing = Some(Typing { text });
            },
            (Kind::Text, Value::Text(text)) => self.typing = Some(Typing { text }),
            _ => {},
        }
    }

    /// What was typed for the setting under the cursor.
    fn typed(&mut self, text: String) {
        let item = self.item();
        let value = match item.kind {
            Kind::Number { min, max, decimals, .. } => match text.parse::<f64>() {
                Ok(n) if n.is_finite() => number(n, min, max, decimals),
                _ => return self.say(format!("{text:?} isn't a number"), false),
            },
            Kind::Folder => {
                if text.is_empty() {
                    return;
                }
                if let Err(why) = TabFolder::parse(&text) {
                    return self.say(why, false);
                }
                if TabFolder::parse(&text).ok().and_then(|folder| folder.folder()).is_none() {
                    return self.say(format!("there is no folder {text}"), false);
                }
                toml_edit::value(text)
            },
            Kind::Text if text.is_empty() => return,
            _ => toml_edit::value(text),
        };
        self.set(value);
    }

    /// `r`: the setting goes back to its default, by leaving it out of the file.
    fn reset(&mut self) {
        let (table, key) = (self.item().table, self.item().key);
        if !self.is_set(self.item()) {
            return self.say("that is the default already".into(), true);
        }
        self.write(|doc| {
            match table {
                Some(table) => {
                    if let Some(t) = doc.get_mut(table).and_then(|t| t.as_table_like_mut()) {
                        t.remove(key);
                    }
                },
                None => {
                    doc.remove(key);
                },
            };
        });
    }

    fn set(&mut self, value: toml_edit::Item) {
        let (table, key) = (self.item().table, self.item().key);
        self.write(|doc| match table {
            Some(table) => {
                let t = doc.entry(table).or_insert(toml_edit::table());
                if let Some(t) = t.as_table_like_mut() {
                    t.insert(key, value);
                }
            },
            None => {
                doc.insert(key, value);
            },
        });
    }

    /// Makes a change to the file and writes it, if what comes out is a config lumen
    /// takes. A file changed by hand since it was read is read again first.
    fn write(&mut self, change: impl FnOnce(&mut DocumentMut)) {
        let on_disk = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            // removed meanwhile: the change goes into a file of its own
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return self.say(format!("can't read {} again: {e}", self.path.display()), false),
        };
        if on_disk != self.text {
            match on_disk.parse::<DocumentMut>() {
                Ok(doc) => (self.doc, self.text) = (doc, on_disk),
                Err(e) => return self.say(format!("the file was changed and isn't valid TOML now: {e}"), false),
            }
        }
        let mut doc = self.doc.clone();
        change(&mut doc);
        let text = doc.to_string();
        if let Err(e) = toml::from_str::<Config>(&text) {
            return self.say(format!("lumen wouldn't take that: {}", e.message()), false);
        }
        match write_file(&self.path, &text) {
            Ok(()) => {
                (self.doc, self.text) = (doc, text);
                let item = self.item();
                let value = shown((item.get)(&self.config()));
                self.say(format!("{} is {value} now", item.label), true);
            },
            Err(e) => self.say(format!("can't write {}: {e}", self.path.display()), false),
        }
    }

    fn say(&mut self, text: String, good: bool) {
        self.said = Some((text, good));
    }

    fn draw(&mut self, (columns, lines): (usize, usize)) {
        let (r, g, b) = self.accent;
        let accent = format!("\x1b[38;2;{r};{g};{b}m");
        let config = self.config();
        let mut out = String::from("\x1b[H");
        let line = |out: &mut String, text: &str| {
            out.push_str(text);
            out.push_str("\x1b[0m\x1b[K\r\n");
        };
        line(&mut out, "");
        line(&mut out, &format!("  {accent}\x1b[1m✦ lumen settings\x1b[0m"));
        let home = dirs::home_dir().unwrap_or_default();
        let shown_path = match self.path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => self.path.display().to_string(),
        };
        line(
            &mut out,
            &format!(
                "  \x1b[2m{} · each change is saved and shows at once",
                clip(&shown_path, columns.saturating_sub(40))
            ),
        );
        line(&mut out, "");
        // the list, as much of it as fits, with the cursor's row in view
        let room = lines.saturating_sub(10).max(3);
        if self.at < self.top + 1 {
            self.top = self.at.saturating_sub(1);
        }
        if self.at >= self.top + room {
            self.top = self.at + 1 - room;
        }
        let label_width = 30;
        for (index, row) in self.rows.iter().enumerate().skip(self.top).take(room) {
            match row {
                Row::Heading(name) => line(&mut out, &format!("  \x1b[1m{name}")),
                Row::Setting(item) => {
                    let value = shown((item.get)(&config));
                    let value = if self.is_set(item) { value } else { format!("\x1b[2m{value}") };
                    let label = format!("{:<label_width$}", item.label);
                    if index == self.at {
                        line(&mut out, &format!("  {accent}\x1b[1m› {label}\x1b[0m {accent}{value}"));
                    } else {
                        line(&mut out, &format!("    {label} {value}"));
                    }
                },
            }
        }
        for _ in self.rows.len().saturating_sub(self.top).min(room)..room {
            line(&mut out, "");
        }
        line(&mut out, "");
        let item = self.item();
        line(&mut out, &format!("  \x1b[2m{}", clip(item.help, columns.saturating_sub(4))));
        match &self.typing {
            Some(typing) => {
                line(&mut out, &format!("  {accent}{}: \x1b[0m{}\x1b[7m \x1b[0m", item.label, typing.text));
                line(&mut out, "  \x1b[2menter  keep it    esc  leave it as it was");
            },
            None => {
                match &self.said {
                    Some((text, true)) => {
                        line(&mut out, &format!("  {accent}{}", clip(text, columns.saturating_sub(4))))
                    },
                    Some((text, false)) => {
                        line(&mut out, &format!("  \x1b[31m{}", clip(text, columns.saturating_sub(4))))
                    },
                    None => line(&mut out, ""),
                }
                line(&mut out, "  \x1b[2m↑↓ move   ←→ change   enter switch or type   r default   q leave");
            },
        }
        out.push_str("\x1b[J");
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(out.as_bytes());
        let _ = stdout.flush();
    }
}

/// The next of `words` after `now` (or before it, for a negative `by`), around the end.
fn cycle<'a>(words: &[&'a str], now: &str, by: isize) -> &'a str {
    let at = words.iter().position(|word| *word == now).unwrap_or(0) as isize;
    words[(at + by).rem_euclid(words.len() as isize) as usize]
}

/// A number for the file: kept between the ends, rounded to its decimals, and a whole
/// one for a setting that counts (scrollback).
fn number(n: f64, min: f64, max: f64, decimals: usize) -> toml_edit::Item {
    let n = n.clamp(min, max);
    let scale = 10f64.powi(decimals as i32);
    let n = (n * scale).round() / scale;
    if decimals == 0 && max > 1000. { toml_edit::value(n as i64) } else { toml_edit::value(n) }
}

fn shown(value: Value) -> String {
    match value {
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::Number(n) if n.fract() == 0. => format!("{n:.0}"),
        Value::Number(n) => format!("{n}"),
        Value::Text(text) => text,
    }
}

fn clip(text: &str, most: usize) -> String {
    let most = most.max(10);
    if text.chars().count() <= most {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(most - 1).collect();
    clipped.push('…');
    clipped
}

/// The file `path` names, through links, also one that a link names but that isn't made
/// yet: writing there keeps the link.
fn followed(path: PathBuf) -> PathBuf {
    let mut path = path;
    // as many links as the system follows itself
    for _ in 0..40 {
        let Ok(target) = std::fs::read_link(&path) else { break };
        path = match path.parent() {
            Some(dir) if target.is_relative() => dir.join(target),
            _ => target,
        };
    }
    path
}

/// Writes the file through one beside it that takes its place at once, so that a
/// running lumen never reads half of it, with the mode the file had. The file beside it
/// has a name of its own, made anew: never one that was there.
fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let next = path.with_file_name(format!(".lumen.toml.settings-{}-{stamp}", std::process::id()));
    let put = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&next)?;
        file.write_all(text.as_bytes())?;
        if let Ok(meta) = std::fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        std::fs::rename(&next, path)
    })();
    if put.is_err() {
        let _ = std::fs::remove_file(&next);
    }
    put
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Space,
    Backspace,
    ClearLine,
    Escape,
    Quit,
    Char(char),
}

/// The keys in what the terminal sent: arrows in both of their forms, and text.
fn parse_keys(bytes: &[u8]) -> Vec<Key> {
    let mut keys = Vec::new();
    let text = String::from_utf8_lossy(bytes);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let key = match c {
            '\x1b' => match (chars.peek().copied(), chars.clone().nth(1)) {
                (Some('[' | 'O'), Some(code @ ('A' | 'B' | 'C' | 'D'))) => {
                    chars.next();
                    chars.next();
                    match code {
                        'A' => Key::Up,
                        'B' => Key::Down,
                        'C' => Key::Right,
                        _ => Key::Left,
                    }
                },
                (Some('[' | 'O'), Some(_)) => {
                    // a sequence the menu has no use for: skipped to its end
                    chars.next();
                    while chars.next_if(|c| !c.is_ascii_alphabetic() && *c != '~').is_some() {}
                    chars.next();
                    continue;
                },
                _ => Key::Escape,
            },
            '\r' | '\n' => Key::Enter,
            ' ' => Key::Space,
            '\x7f' | '\x08' => Key::Backspace,
            '\x15' => Key::ClearLine,
            '\x03' | '\x04' => Key::Quit,
            c if !c.is_control() => Key::Char(c),
            _ => continue,
        };
        keys.push(key);
    }
    keys
}

fn terminal_size() -> (usize, usize) {
    // SAFETY: TIOCGWINSZ fills the winsize it is given
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(std::io::stdout().as_raw_fd(), libc::TIOCGWINSZ, &mut size) } == 0 && size.ws_col > 0 {
        (size.ws_col.into(), size.ws_row.into())
    } else {
        (80, 24)
    }
}

/// The terminal as it was before the menu took it, for a signal that ends the menu
/// before [`Raw`] is dropped.
static SAVED: std::sync::OnceLock<libc::termios> = std::sync::OnceLock::new();

/// Gives the terminal back, then ends as the signal would have: `kill` (SIGTERM), a
/// closed terminal (SIGHUP), or the other ends of a process.
extern "C" fn give_back(signal: libc::c_int) {
    const BACK: &[u8] = b"\x1b[?25h\x1b[?1049l";
    // SAFETY: write, tcsetattr and _exit are async-signal-safe; SAVED is set before this
    // handler is installed and never changes
    unsafe {
        libc::write(1, BACK.as_ptr().cast(), BACK.len());
        if let Some(saved) = SAVED.get() {
            libc::tcsetattr(0, libc::TCSANOW, saved);
        }
        libc::_exit(128 + signal);
    }
}

/// The terminal in raw mode, on its other screen, without a cursor, until dropped.
struct Raw {
    saved: libc::termios,
}

impl Raw {
    fn enter() -> std::io::Result<Raw> {
        // SAFETY: tcgetattr and tcsetattr on stdin with termios structs this function owns
        let saved = unsafe {
            let mut saved: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut saved) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut raw = saved;
            libc::cfmakeraw(&mut raw);
            if libc::tcsetattr(0, libc::TCSANOW, &raw) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            saved
        };
        let _ = SAVED.set(saved);
        for signal in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT, libc::SIGQUIT] {
            // SAFETY: a handler that only does what is safe in one
            unsafe { libc::signal(signal, give_back as *const () as libc::sighandler_t) };
        }
        print!("\x1b[?1049h\x1b[?25l");
        let _ = std::io::stdout().flush();
        Ok(Raw { saved })
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        print!("\x1b[?25h\x1b[?1049l");
        let _ = std::io::stdout().flush();
        // SAFETY: the termios saved when the terminal was taken
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &self.saved) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A menu on a file of its own, as `lumen settings` opens it.
    fn menu_on(text: &str) -> (Menu, PathBuf) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lumen-settings-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lumen.toml");
        std::fs::write(&path, text).unwrap();
        let rows = rows();
        let at = rows.iter().position(|row| matches!(row, Row::Setting(_))).unwrap();
        let menu = Menu {
            rows,
            path: path.clone(),
            text: text.into(),
            doc: text.parse().unwrap(),
            at,
            top: 0,
            typing: None,
            said: None,
            accent: (255, 200, 120),
        };
        (menu, dir)
    }

    fn go_to(menu: &mut Menu, table: Option<&str>, key: &str) {
        menu.at = menu
            .rows
            .iter()
            .position(|row| matches!(row, Row::Setting(item) if item.table == table && item.key == key))
            .unwrap();
    }

    #[test]
    fn every_setting_reads_what_the_defaults_say() {
        let config = Config::default();
        for row in rows() {
            if let Row::Setting(item) = row {
                // each getter runs, and the defaults parse back
                let _ = (item.get)(&config);
            }
        }
        assert_eq!(toml::from_str::<Config>("").unwrap().new_tab_folder, TabFolder::Current);
    }

    #[test]
    fn a_change_is_written_and_keeps_the_rest_of_the_file() {
        let text = "# my own words\nscrollback = 5000 # mine\n\n[font]\nfamily = \"Lilex\"\n";
        let (mut menu, dir) = menu_on(text);
        go_to(&mut menu, Some("font"), "size");
        menu.key(Key::Right);
        go_to(&mut menu, Some("cursor"), "blink");
        menu.key(Key::Enter);
        go_to(&mut menu, None, "new_tab_folder");
        menu.key(Key::Right);
        let written = std::fs::read_to_string(dir.join("lumen.toml")).unwrap();
        assert!(written.starts_with("# my own words\nscrollback = 5000 # mine\n"), "{written}");
        let config: Config = toml::from_str(&written).unwrap();
        assert_eq!(config.font.family, "Lilex");
        assert_eq!(config.font.size, 12.5);
        assert!(!config.cursor.blink);
        assert_eq!(config.new_tab_folder, TabFolder::Home);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_folder_is_typed_and_checked_and_reset_goes_back_to_the_default() {
        let (mut menu, dir) = menu_on("");
        go_to(&mut menu, None, "new_tab_folder");
        menu.key(Key::Enter);
        for c in "/no/such/folder/here".chars() {
            menu.key(Key::Char(c));
        }
        menu.key(Key::Enter);
        assert!(matches!(&menu.said, Some((text, false)) if text.contains("no folder")));
        menu.key(Key::Enter);
        for c in "/".chars() {
            menu.key(Key::Char(c));
        }
        menu.key(Key::Enter);
        let config: Config = toml::from_str(&std::fs::read_to_string(dir.join("lumen.toml")).unwrap()).unwrap();
        assert_eq!(config.new_tab_folder, TabFolder::Folder("/".into()));
        menu.key(Key::Char('r'));
        assert!(!std::fs::read_to_string(dir.join("lumen.toml")).unwrap().contains("new_tab_folder"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_changed_by_hand_meanwhile_is_not_written_over() {
        let (mut menu, dir) = menu_on("scrollback = 1000\n");
        std::fs::write(dir.join("lumen.toml"), "scrollback = 2000\ncopy_on_select = true\n").unwrap();
        go_to(&mut menu, Some("cursor"), "glow");
        menu.key(Key::Enter);
        let config: Config = toml::from_str(&std::fs::read_to_string(dir.join("lumen.toml")).unwrap()).unwrap();
        assert_eq!(config.scrollback, 2000);
        assert!(config.copy_on_select);
        assert!(!config.cursor.glow);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_link_to_a_file_not_made_yet_stays_a_link() {
        let (mut menu, dir) = menu_on("");
        let real = dir.join("dotfiles/lumen.toml");
        let link = dir.join("link.toml");
        std::os::unix::fs::symlink("dotfiles/lumen.toml", &link).unwrap();
        menu.path = followed(link.clone());
        assert_eq!(menu.path, real);
        go_to(&mut menu, Some("cursor"), "glow");
        menu.key(Key::Enter);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(std::fs::read_to_string(&real).unwrap().contains("glow = false"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_beside_the_config_is_never_touched_and_a_removed_file_starts_anew() {
        let (mut menu, dir) = menu_on("scrollback = 1000\n");
        std::fs::write(dir.join(".lumen.toml.settings"), "mine").unwrap();
        std::fs::remove_file(dir.join("lumen.toml")).unwrap();
        go_to(&mut menu, Some("cursor"), "glow");
        menu.key(Key::Enter);
        let written = std::fs::read_to_string(dir.join("lumen.toml")).unwrap();
        assert!(!written.contains("scrollback"), "the removed file's settings came back: {written}");
        assert_eq!(std::fs::read_to_string(dir.join(".lumen.toml.settings")).unwrap(), "mine");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "nothing else is left beside it");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn numbers_stay_between_their_ends() {
        let (mut menu, dir) = menu_on("[window]\nopacity = 0.95\n");
        go_to(&mut menu, Some("window"), "opacity");
        menu.key(Key::Right);
        menu.key(Key::Right);
        assert_eq!(menu.config().window.opacity, 1.0);
        menu.key(Key::Enter);
        for _ in 0..4 {
            menu.key(Key::Backspace);
        }
        for c in "7".chars() {
            menu.key(Key::Char(c));
        }
        menu.key(Key::Enter);
        assert_eq!(menu.config().window.opacity, 1.0, "7 is kept at the end, 1");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keys_are_read_in_both_forms() {
        assert_eq!(parse_keys(b"\x1b[A\x1bOB\x1b[C\x1b[D"), [Key::Up, Key::Down, Key::Right, Key::Left]);
        assert_eq!(parse_keys(b"\x1b"), [Key::Escape]);
        assert_eq!(parse_keys(b"\x1b[5~j"), [Key::Char('j')]);
        assert_eq!(parse_keys("ü \r".as_bytes()), [Key::Char('ü'), Key::Space, Key::Enter]);
    }
}
