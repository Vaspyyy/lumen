//! Inline images: the kitty graphics protocol.
//!
//! A program sends an image inside an escape sequence, `ESC _ G ... ESC \`.
//! The parser alacritty_terminal uses drops those, so they are taken out of
//! the program's output before it gets there ([`Filter`]).
//!
//! An image that is put on screen occupies cells. Each of them is marked
//! with a link only lumen makes (`lumen-image:...`), so the image scrolls,
//! is erased and is overwritten with the text around it, and the grid needs
//! to know nothing about images. What is drawn in those cells is looked up
//! in the [`Store`] by the number in the link.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Hyperlink};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::Handler;
use base64::Engine;
use gpui::RenderImage;

/// What the link of a cell that shows part of an image starts with.
const SCHEME: &str = "lumen-image:";

/// The image part a cell shows, if its link is one of lumen's: the
/// placement, and the cell's column and row within it.
pub fn cell_of(uri: &str) -> Option<(u32, usize, usize)> {
    let mut parts = uri.strip_prefix(SCHEME)?.split(':');
    let placement = parts.next()?.parse().ok()?;
    let col = parts.next()?.parse().ok()?;
    let row = parts.next()?.parse().ok()?;
    Some((placement, col, row))
}

/// A piece of a program's output.
pub enum Piece<'a> {
    /// What the terminal's own parser gets.
    Text(&'a [u8]),
    /// A graphics command: what was between `ESC _ G` and `ESC \`.
    Command(Vec<u8>),
}

#[derive(Default, Clone, Copy, PartialEq)]
enum State {
    #[default]
    Text,
    /// After ESC.
    Esc,
    /// After `ESC _`.
    Start,
    Command,
    /// After ESC inside a command.
    CommandEsc,
}

/// Takes graphics commands out of a stream of output that arrives in
/// arbitrary pieces.
#[derive(Default)]
pub struct Filter {
    state: State,
    command: Vec<u8>,
    /// How many bytes of a possible command start (`ESC`, `ESC _`) were at
    /// the end of the input before this one, and are still held back.
    held: usize,
    /// The command got too long and is being dropped.
    overflow: bool,
}

/// One escape sequence is one chunk of an image, at most 4096 bytes by the
/// protocol. Anything far beyond that is not an image being sent.
const MAX_COMMAND: usize = 4 << 20;

impl Filter {
    pub fn feed<'a>(&mut self, input: &'a [u8], mut out: impl FnMut(Piece<'a>)) {
        const START: &[u8] = b"\x1b_";
        // input[text..] is output not handed on yet
        let mut text = 0;
        // where the ESC that may start a command is, when it is in this input
        let mut esc_at = 0;
        let mut i = 0;
        while i < input.len() {
            match self.state {
                State::Text => match input[i..].iter().position(|&b| b == 0x1b) {
                    Some(at) => {
                        esc_at = i + at;
                        i = esc_at + 1;
                        self.state = State::Esc;
                    },
                    None => i = input.len(),
                },
                State::Esc | State::Start => {
                    let wanted = if self.state == State::Esc { b'_' } else { b'G' };
                    if input[i] != wanted {
                        // some other escape sequence: it is text, and so is what was held back of it
                        if self.held > 0 {
                            out(Piece::Text(&START[..self.held]));
                            self.held = 0;
                            // what was held went out just now, with the part of it that is in this input
                            text = i;
                        }
                        self.state = State::Text;
                        // this byte is looked at again: it may be the next ESC
                        continue;
                    }
                    i += 1;
                    if self.state == State::Esc {
                        self.state = State::Start;
                        if self.held > 0 {
                            self.held = 2;
                        }
                    } else {
                        if self.held == 0 && esc_at > text {
                            out(Piece::Text(&input[text..esc_at]));
                        }
                        self.held = 0;
                        self.command.clear();
                        self.overflow = false;
                        self.state = State::Command;
                    }
                },
                State::Command => {
                    let end = input[i..].iter().position(|&b| b == 0x1b).map_or(input.len(), |at| i + at);
                    if self.command.len() + (end - i) > MAX_COMMAND {
                        self.overflow = true;
                    } else {
                        self.command.extend_from_slice(&input[i..end]);
                    }
                    i = end;
                    if i < input.len() {
                        i += 1;
                        self.state = State::CommandEsc;
                    }
                },
                State::CommandEsc => {
                    if input[i] == b'\\' {
                        i += 1;
                        text = i;
                        self.state = State::Text;
                        let command = std::mem::take(&mut self.command);
                        if !self.overflow {
                            out(Piece::Command(command));
                        }
                    } else {
                        // an ESC that doesn't end the command starts something else: the command is void
                        self.command.clear();
                        self.state = State::Esc;
                        if i == 0 {
                            self.held = 1;
                        } else {
                            esc_at = i - 1;
                        }
                        text = i.saturating_sub(1);
                    }
                },
            }
        }
        match self.state {
            State::Text => {
                if text < input.len() {
                    out(Piece::Text(&input[text..]));
                }
            },
            // the input ends in what may be the start of a command: hold that back
            State::Esc | State::Start => {
                if self.held == 0 {
                    if esc_at > text {
                        out(Piece::Text(&input[text..esc_at]));
                    }
                    self.held = input.len() - esc_at;
                }
            },
            State::Command | State::CommandEsc => {},
        }
    }
}

/// The keys of a graphics command.
#[derive(Clone, Debug, Default, PartialEq)]
struct Control {
    /// `a`: t (transmit), T (transmit and show), p (show), d (delete), q (query).
    action: u8,
    /// `f`: 24 (RGB), 32 (RGBA), 100 (PNG).
    format: u32,
    /// `t`: d (in the command), f (a file), t (a temporary file), s (shared memory).
    medium: u8,
    /// `o`: z when the data is zlib compressed.
    compression: u8,
    /// `m`: more chunks of this image follow.
    more: bool,
    /// `q`: 1 keeps the OK to itself, 2 errors too.
    quiet: u32,
    /// `i`, `I`, `p`: the image's id, its number, the placement's id.
    id: u32,
    number: u32,
    placement: u32,
    /// `s`, `v`: the size of raw pixel data.
    width: u32,
    height: u32,
    /// `c`, `r`: the cells to show the image in.
    cols: u32,
    rows: u32,
    /// `x`, `y`, `w`, `h`: the part of the image to show.
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    /// `C`: 1 leaves the cursor where it is.
    stay: bool,
    /// `d`: what to delete.
    delete: u8,
    /// `S`, `O`: how much of a file to read, and from where.
    size: u32,
    offset: u32,
    /// `U`: the image is placed by placeholder characters, which lumen doesn't do.
    placeholder: bool,
}

/// Splits a command into its keys and its payload. The last value says
/// whether the command says nothing about an image itself (it has no keys
/// but `m` and `q`), which is what a further chunk of one looks like.
fn parse(raw: &[u8]) -> (Control, &[u8], bool) {
    let (keys, payload) = match raw.iter().position(|&b| b == b';') {
        Some(at) => (&raw[..at], &raw[at + 1..]),
        None => (raw, &raw[raw.len()..]),
    };
    let mut c = Control { action: b't', format: 32, medium: b'd', ..Control::default() };
    let mut bare = true;
    for pair in keys.split(|&b| b == b',') {
        let (Some(&key), Some(b'=')) = (pair.first(), pair.get(1)) else { continue };
        let value = &pair[2..];
        let letter = value.first().copied().unwrap_or(0);
        let number = std::str::from_utf8(value).ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        bare &= matches!(key, b'm' | b'q');
        match key {
            b'a' => c.action = letter,
            b'f' => c.format = number,
            b't' => c.medium = letter,
            b'o' => c.compression = letter,
            b'm' => c.more = number == 1,
            b'q' => c.quiet = number,
            b'i' => c.id = number,
            b'I' => c.number = number,
            b'p' => c.placement = number,
            b's' => c.width = number,
            b'v' => c.height = number,
            b'c' => c.cols = number,
            b'r' => c.rows = number,
            b'x' => c.x = number,
            b'y' => c.y = number,
            b'w' => c.w = number,
            b'h' => c.h = number,
            b'C' => c.stay = number == 1,
            b'd' => c.delete = letter,
            b'S' => c.size = number,
            b'O' => c.offset = number,
            b'U' => c.placeholder = number == 1,
            _ => {},
        }
    }
    (c, payload, bare)
}

pub struct Image {
    pub render: Arc<RenderImage>,
    pub width: u32,
    pub height: u32,
    bytes: usize,
}

/// An image where it was put on screen.
#[derive(Clone)]
pub struct Placement {
    pub image: Arc<Image>,
    /// The id the program gave the image, 0 when it gave none.
    image_id: u32,
    /// The id the program gave this placement of it, 0 when it gave none.
    user_id: u32,
    /// It was put on the alternate screen, and is gone when that is left.
    alt: bool,
    /// The image's size on screen, in cells, from the top left of its
    /// first cell. It can end inside its last cells.
    pub extent: (f32, f32),
}

/// The images of one terminal. The thread that reads the program's output
/// fills it, the window draws from it.
#[derive(Default)]
pub struct Store {
    /// Images the program can refer to again, by the id it gave them.
    images: HashMap<u32, Arc<Image>>,
    /// The bytes of the images in `images`.
    named_bytes: usize,
    /// Image numbers (`I=`) and the ids they stand for.
    numbers: HashMap<u32, u32>,
    /// By lumen's own number for them: the oldest first.
    placements: BTreeMap<u32, Placement>,
    /// The placements a program named: (image id, placement id) to lumen's number.
    named_placements: HashMap<(u32, u32), u32>,
    /// The bytes of the placements' images, an image counted once for each.
    placed_bytes: usize,
    next_placement: u32,
    next_id: u32,
    /// Images that are gone, whose textures the window still has to free.
    dropped: Vec<Arc<RenderImage>>,
}

pub type Shared = Arc<Mutex<Store>>;

impl Store {
    pub fn placement(&self, id: u32) -> Option<&Placement> {
        self.placements.get(&id)
    }

    /// The images to free on the GPU: those that went away since the last
    /// call, or with `all`, every one (the terminal is closing).
    pub fn take_dropped(&mut self, all: bool) -> Vec<Arc<RenderImage>> {
        if all {
            self.dropped.extend(self.images.drain().map(|(_, image)| image.render.clone()));
            self.dropped.extend(std::mem::take(&mut self.placements).into_values().map(|p| p.image.render.clone()));
            self.numbers.clear();
            self.named_placements.clear();
            (self.named_bytes, self.placed_bytes) = (0, 0);
        }
        let mut dropped = std::mem::take(&mut self.dropped);
        dropped.sort_by_key(|image| Arc::as_ptr(image) as usize);
        dropped.dedup_by(|a, b| Arc::ptr_eq(a, b));
        dropped
    }

    /// Forgets a placement. The image goes with its last user. Returns the
    /// bytes that freed.
    fn remove_placement(&mut self, id: u32) -> usize {
        let Some(placement) = self.placements.remove(&id) else { return 0 };
        self.placed_bytes = self.placed_bytes.saturating_sub(placement.image.bytes);
        if placement.user_id != 0 {
            self.named_placements.remove(&(placement.image_id, placement.user_id));
        }
        self.release(placement.image)
    }

    /// Forgets an image the program named. Returns the bytes that freed.
    fn remove_image(&mut self, id: u32) -> usize {
        let Some(image) = self.images.remove(&id) else { return 0 };
        self.named_bytes = self.named_bytes.saturating_sub(image.bytes);
        self.numbers.retain(|_, v| *v != id);
        self.release(image)
    }

    fn release(&mut self, image: Arc<Image>) -> usize {
        if Arc::strong_count(&image) > 1 {
            return 0;
        }
        self.dropped.push(image.render.clone());
        image.bytes
    }

    /// What all images take, each counted once.
    fn bytes(&self) -> usize {
        let mut seen = HashSet::new();
        let all = self.images.values().chain(self.placements.values().map(|p| &p.image));
        all.filter(|image| seen.insert(Arc::as_ptr(image))).map(|image| image.bytes).sum()
    }
}

/// An image command that is ready to act on the grid.
pub struct Step {
    control: Control,
    /// The image the command sent, or why it could not be read.
    image: Option<Result<Arc<Image>, String>>,
}

/// An image with a longer side than this, or more pixels, is refused.
const MAX_SIDE: u32 = 16_384;
const MAX_PIXELS: u64 = 64_000_000;
/// An image is kept no larger than this: more can't be seen in a terminal,
/// and is more than a texture can hold.
const KEPT_SIDE: u32 = 4096;
/// More bytes than this is not read from a file or out of a compressed stream.
const MAX_DATA: usize = 256 << 20;
/// Images a program sent with an id are kept until it deletes them, up to this many bytes.
const MAX_NAMED: usize = 160 << 20;
/// All images of a terminal together take no more than this: the oldest go.
const MAX_BYTES: usize = 320 << 20;
/// After this many bytes of new images, the ones no cell shows any more are looked for.
const COLLECT_EVERY: usize = 48 << 20;

/// The reading side of a terminal's images.
pub struct Graphics {
    store: Shared,
    /// The image whose chunks are still arriving: its keys and its data so far.
    pending: Option<(Control, Vec<u8>)>,
    /// How many placements there may be, and how many bytes in them, before
    /// the grid is searched for the ones no cell shows any more.
    collect_at: usize,
    collect_bytes: usize,
}

impl Graphics {
    pub fn new(store: Shared) -> Graphics {
        Graphics { store, pending: None, collect_at: 256, collect_bytes: COLLECT_EVERY }
    }

    /// Takes in a command. What can be done without the grid, reading and
    /// decoding the image, is done here; returns what is left to do, if
    /// the command is complete.
    pub fn receive(&mut self, raw: &[u8]) -> Option<Step> {
        let (control, payload, bare) = parse(raw);
        // A further chunk of an image says nothing but that it is one. A program that was
        // stopped in the middle of an image never sends the rest: what comes then is a
        // command of its own, and must not be taken for more of that image.
        let further = self.pending.as_ref().is_some_and(|(first, _)| {
            bare || Control { more: first.more, quiet: first.quiet, ..control.clone() } == *first
        });
        let (control, data) = if further {
            let (first, mut data) = self.pending.take()?;
            data.extend_from_slice(payload);
            if data.len() > MAX_DATA {
                return None;
            }
            (Control { more: control.more, ..first }, data)
        } else {
            if !matches!(control.action, b't' | b'T' | b'q') {
                // nothing to do with an image that is still arriving
                return Some(Step { control, image: None });
            }
            self.pending = None;
            (control, payload.to_vec())
        };
        if control.more {
            self.pending = Some((control, data));
            return None;
        }
        let image = Some(load(&control, &data));
        Some(Step { control, image })
    }

    /// Does what a command asks of the grid. Returns the answer for the
    /// program, if it gets one.
    pub fn apply<L: EventListener>(&mut self, step: Step, term: &mut Term<L>, cell: (f32, f32)) -> Option<Vec<u8>> {
        let Step { control, image } = step;
        let mut store = self.store.lock().unwrap();
        // the id the image goes by: the program's, or one made up for its number
        let mut id = control.id;
        let result: Result<(), String> = match (control.action, image) {
            (b'q', Some(image)) => image.map(|_| ()),
            (b't' | b'T', Some(Err(e))) => Err(e),
            (b't' | b'T', Some(Ok(image))) => {
                if control.number != 0 && id == 0 {
                    id = loop {
                        store.next_id = store.next_id.wrapping_add(1);
                        let id = 0x4000_0000 | (store.next_id & 0x3fff_ffff);
                        if !store.images.contains_key(&id) {
                            break id;
                        }
                    };
                    store.numbers.insert(control.number, id);
                }
                if id != 0 {
                    store.remove_image(id);
                    store.named_bytes += image.bytes;
                    store.images.insert(id, image.clone());
                    while store.named_bytes > MAX_NAMED {
                        let Some(&oldest) = store.images.keys().filter(|&&k| k != id).min() else { break };
                        store.remove_image(oldest);
                    }
                }
                if control.action == b'T' && !control.placeholder {
                    show(&mut store, term, &control, id, image, cell);
                }
                Ok(())
            },
            (b'p', _) => {
                if id == 0 {
                    id = store.numbers.get(&control.number).copied().unwrap_or(0);
                }
                match store.images.get(&id).cloned() {
                    Some(image) => {
                        if !control.placeholder {
                            show(&mut store, term, &control, id, image, cell);
                        }
                        Ok(())
                    },
                    None => Err("ENOENT:no image with that id".into()),
                }
            },
            (b'd', _) => {
                delete(&mut store, term, &control);
                return None;
            },
            // animation and the rest: not here
            _ => Err("ENOTSUP:this is not supported".into()),
        };
        let full = store.placements.len() > self.collect_at || store.placed_bytes > self.collect_bytes;
        drop(store);
        if full {
            self.collect(term);
        }

        // an image the program can't refer to gets no answer, as there is nothing to say it about
        if id == 0 && control.number == 0 {
            return None;
        }
        let said = match &result {
            Ok(()) if control.quiet >= 1 => return None,
            Err(_) if control.quiet >= 2 => return None,
            Ok(()) => "OK",
            Err(e) => e.as_str(),
        };
        let mut keys = Vec::new();
        if id != 0 {
            keys.push(format!("i={id}"));
        }
        if control.number != 0 {
            keys.push(format!("I={}", control.number));
        }
        if control.placement != 0 {
            keys.push(format!("p={}", control.placement));
        }
        Some(format!("\x1b_G{};{said}\x1b\\", keys.join(",")).into_bytes())
    }

    /// Forgets the placements no cell shows any more (scrolled out of the
    /// history, overwritten, cleared), and then the oldest ones if the
    /// images still take too much.
    fn collect<L: EventListener>(&mut self, term: &Term<L>) {
        let alt = term.mode().contains(TermMode::ALT_SCREEN);
        let mut shown = HashSet::new();
        let grid = term.grid();
        for line in term.topmost_line().0..=term.bottommost_line().0 {
            let row = &grid[Line(line)];
            for col in 0..term.columns() {
                if let Some(link) = row[Column(col)].hyperlink()
                    && let Some((placement, _, _)) = cell_of(link.uri())
                {
                    shown.insert(placement);
                }
            }
        }
        let mut store = self.store.lock().unwrap();
        // Only the screen the program is on can be looked through. From the alternate
        // screen, the main one's images are left alone; back on the main screen, what
        // was put on the alternate one is gone with it.
        let gone: Vec<u32> = store
            .placements
            .iter()
            .filter(|(id, p)| if alt { p.alt && !shown.contains(id) } else { p.alt || !shown.contains(id) })
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            store.remove_placement(id);
        }
        let mut bytes = store.bytes();
        while bytes > MAX_BYTES {
            let freed = match store.placements.keys().next().copied() {
                Some(oldest) => store.remove_placement(oldest),
                None => match store.images.keys().min().copied() {
                    Some(oldest) => store.remove_image(oldest),
                    None => break,
                },
            };
            bytes = bytes.saturating_sub(freed);
        }
        self.collect_at = (store.placements.len() * 2).max(256);
        self.collect_bytes = store.placed_bytes + COLLECT_EVERY;
    }
}

/// Puts an image on screen at the cursor.
fn show<L: EventListener>(
    store: &mut Store,
    term: &mut Term<L>,
    control: &Control,
    image_id: u32,
    image: Arc<Image>,
    cell: (f32, f32),
) {
    // a placement the program named is moved when it is shown again
    let named = (image_id != 0 && control.placement != 0).then_some((image_id, control.placement));
    if let Some(old) = named.and_then(|key| store.named_placements.get(&key).copied()) {
        unmark(term, 0..=term.screen_lines() as i32 - 1, |placement| placement == old);
        store.remove_placement(old);
    }
    let image = match crop(&image, control) {
        Some(part) => Arc::new(part),
        None => image,
    };
    let (cols, rows, extent) = cells_for((image.width, image.height), (control.cols, control.rows), cell);
    store.next_placement += 1;
    let id = store.next_placement;
    store.placed_bytes += image.bytes;
    let alt = term.mode().contains(TermMode::ALT_SCREEN);
    store.placements.insert(id, Placement { image, image_id, user_id: control.placement, alt, extent });
    if let Some(key) = named {
        store.named_placements.insert(key, id);
    }
    place(term, id, cols, rows, control.stay);
}

/// An image is no more rows high than this.
pub const MAX_ROWS: usize = 2000;

/// The cells an image takes (columns, rows), and its size on screen in
/// cells: an image is shown at its own size unless the program says how
/// many columns or rows it gets, and stretched when it says both.
fn cells_for(image: (u32, u32), asked: (u32, u32), cell: (f32, f32)) -> (usize, usize, (f32, f32)) {
    let (iw, ih) = (image.0.max(1) as f32, image.1.max(1) as f32);
    let (cw, ch) = (cell.0.max(1.), cell.1.max(1.));
    // the image's size in cells, when it is this many pixels
    let (w, h) = match asked {
        (0, 0) => (iw / cw, ih / ch),
        (c, 0) => (c as f32, c as f32 * cw * ih / iw / ch),
        (0, r) => (r as f32 * ch * iw / ih / cw, r as f32),
        (c, r) => (c as f32, r as f32),
    };
    let (w, h) = (w.clamp(0.01, 2000.), h.clamp(0.01, MAX_ROWS as f32));
    (w.ceil() as usize, h.ceil() as usize, (w, h))
}

/// The part of an image a command asks for, when that is not all of it.
fn crop(image: &Image, control: &Control) -> Option<Image> {
    if control.x == 0 && control.y == 0 && control.w == 0 && control.h == 0 {
        return None;
    }
    let x = control.x.min(image.width.saturating_sub(1));
    let y = control.y.min(image.height.saturating_sub(1));
    let w = if control.w == 0 { image.width - x } else { control.w.min(image.width - x) };
    let h = if control.h == 0 { image.height - y } else { control.h.min(image.height - y) };
    let source = image.render.as_bytes(0)?;
    let (x, y, w, h, width) = (x as usize, y as usize, w as usize, h as usize, image.width as usize);
    let mut pixels = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        let start = (row * width + x) * 4;
        pixels.extend_from_slice(source.get(start..start + w * 4)?);
    }
    Some(render(pixels, w as u32, h as u32))
}

/// Marks the cells an image is shown in, from the cursor on.
fn place<L: EventListener>(term: &mut Term<L>, placement: u32, cols: usize, rows: usize, stay: bool) {
    let columns = term.columns();
    let start = term.grid().cursor.point;
    let mark = |term: &mut Term<L>, line: Line, row: usize| {
        for col in 0..cols.min(columns.saturating_sub(start.column.0)) {
            let cell = &mut term.grid_mut()[line][Column(start.column.0 + col)];
            *cell = Cell::default();
            cell.set_hyperlink(Some(Hyperlink::new(Some(placement), format!("{SCHEME}{placement}:{col}:{row}"))));
        }
    };
    if stay {
        // the cursor stays, and so does the screen: what doesn't fit is not shown
        let below = (term.screen_lines() as i32 - start.line.0).max(0) as usize;
        for row in 0..rows.min(below) {
            mark(term, Line(start.line.0 + row as i32), row);
        }
    } else {
        for row in 0..rows {
            let line = term.grid().cursor.point.line;
            mark(term, line, row);
            if row + 1 < rows {
                term.linefeed();
            }
        }
        // the cursor ends up behind the image's last row
        term.goto_col((start.column.0 + cols).min(columns.saturating_sub(1)));
    }
}

/// Takes the marks of the placements `gone` says yes to off the cells of
/// these lines.
fn unmark<L: EventListener>(
    term: &mut Term<L>,
    lines: std::ops::RangeInclusive<i32>,
    mut gone: impl FnMut(u32) -> bool,
) {
    let columns = term.columns();
    for line in lines {
        for col in 0..columns {
            let cell = &mut term.grid_mut()[Line(line)][Column(col)];
            let Some(link) = cell.hyperlink() else { continue };
            let Some((placement, _, _)) = cell_of(link.uri()) else { continue };
            if gone(placement) {
                cell.set_hyperlink(None);
            }
        }
    }
}

/// Takes images off the screen: `d=a` all that are visible, `d=i` the ones
/// of one image (or the one placement of it that `p` names). A capital
/// letter also forgets the images themselves.
fn delete<L: EventListener>(store: &mut Store, term: &mut Term<L>, control: &Control) {
    let what = if control.delete == 0 { b'a' } else { control.delete };
    let id = if control.id != 0 { control.id } else { store.numbers.get(&control.number).copied().unwrap_or(0) };
    // which lines to look through, and the one image to take off them (or every image)
    let (lines, only) = match what.to_ascii_lowercase() {
        b'a' => (0..=term.screen_lines() as i32 - 1, None),
        b'i' | b'n' if id != 0 => (term.topmost_line().0..=term.bottommost_line().0, Some(id)),
        _ => return,
    };
    let mut gone = HashSet::new();
    unmark(term, lines, |placement| {
        let hit = only.is_none_or(|id| {
            store
                .placements
                .get(&placement)
                .is_some_and(|p| p.image_id == id && (control.placement == 0 || p.user_id == control.placement))
        });
        if hit {
            gone.insert(placement);
        }
        hit
    });
    for placement in gone {
        store.remove_placement(placement);
    }
    if what.is_ascii_uppercase() {
        // the images that are nowhere on screen any more go too
        let shown: HashSet<u32> = store.placements.values().map(|p| p.image_id).collect();
        let unused: Vec<u32> =
            store.images.keys().filter(|&&k| if id != 0 { k == id } else { !shown.contains(&k) }).copied().collect();
        for image in unused {
            store.remove_image(image);
        }
    }
}

/// Reads the image a command sends.
fn load(control: &Control, payload: &[u8]) -> Result<Arc<Image>, String> {
    let decoded = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(payload.iter().copied().filter(|&b| b != b'=').collect::<Vec<u8>>())
        .map_err(|_| "EINVAL:the data is not base64")?;
    let mut data = match control.medium {
        b'd' => decoded,
        b'f' | b't' => {
            let path = String::from_utf8(decoded).map_err(|_| "EINVAL:the file name is not UTF-8")?;
            read_file(Path::new(&path), control.offset as usize, control.size as usize, control.medium == b't')?
        },
        _ => return Err("ENOTSUP:images are read from the command or from a file".into()),
    };
    if control.compression == b'z' {
        let mut plain = Vec::new();
        flate2::read::ZlibDecoder::new(data.as_slice())
            .take(MAX_DATA as u64)
            .read_to_end(&mut plain)
            .map_err(|_| "EINVAL:the data is not zlib")?;
        data = plain;
    }
    let fits = |width: u32, height: u32| {
        width > 0 && height > 0 && width <= MAX_SIDE && height <= MAX_SIDE && width as u64 * height as u64 <= MAX_PIXELS
    };
    let (pixels, width, height) = match control.format {
        100 => {
            let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&data), image::ImageFormat::Png);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(MAX_SIDE);
            limits.max_image_height = Some(MAX_SIDE);
            limits.max_alloc = Some(MAX_PIXELS * 8);
            reader.limits(limits);
            let (width, height) = reader.into_dimensions().map_err(|_| "EBADPNG:the data is not a PNG")?;
            if !fits(width, height) {
                return Err("EINVAL:the image is too large".into());
            }
            let image = image::load_from_memory_with_format(&data, image::ImageFormat::Png)
                .map_err(|_| "EBADPNG:the data is not a PNG")?
                .into_rgba8();
            (image.into_raw(), width, height)
        },
        24 | 32 => {
            let (width, height) = (control.width, control.height);
            if !fits(width, height) {
                return Err("EINVAL:the image has no size, or is too large".into());
            }
            let channels = control.format as usize / 8;
            let count = width as usize * height as usize;
            if data.len() < count * channels {
                return Err("EINVAL:the data is shorter than the size given".into());
            }
            let pixels = if channels == 4 {
                data[..count * 4].to_vec()
            } else {
                data.as_chunks::<3>().0.iter().take(count).flat_map(|p| [p[0], p[1], p[2], 255]).collect()
            };
            (pixels, width, height)
        },
        _ => return Err("EINVAL:unknown image format".into()),
    };
    let (mut pixels, mut width, mut height) = (pixels, width, height);
    if width.max(height) > KEPT_SIDE {
        // more than can be seen, and more than a texture holds: kept smaller
        let scale = KEPT_SIDE as f32 / width.max(height) as f32;
        let (w, h) = (((width as f32 * scale) as u32).max(1), ((height as f32 * scale) as u32).max(1));
        let full = image::RgbaImage::from_raw(width, height, pixels).ok_or("EINVAL:the data doesn't fit the size")?;
        pixels = image::imageops::thumbnail(&full, w, h).into_raw();
        (width, height) = (w, h);
    }
    // GPUI wants blue first
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Arc::new(render(pixels, width, height)))
}

/// An image from BGRA pixels.
fn render(pixels: Vec<u8>, width: u32, height: u32) -> Image {
    let bytes = pixels.len();
    let buffer = image::RgbaImage::from_raw(width, height, pixels).expect("as many pixels as the size says");
    Image { render: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])), width, height, bytes }
}

/// Reads the file a program names. The name is taken as the file it really
/// is, with links and `..` followed, so that the checks are of that.
fn read_file(path: &Path, offset: usize, size: usize, temporary: bool) -> Result<Vec<u8>, String> {
    let path = std::fs::canonicalize(path).map_err(|_| "EBADF:can't read the file")?;
    // not what the system makes up on the spot, nor a device, which may block or never end
    let system = ["/proc", "/sys", "/dev"].iter().any(|dir| path.starts_with(dir)) && !path.starts_with("/dev/shm");
    let meta = std::fs::metadata(&path).map_err(|_| "EBADF:can't read the file")?;
    if system || !meta.is_file() {
        return Err("EBADF:not a regular file".into());
    }
    let wanted = if size == 0 { meta.len() as usize } else { size };
    if wanted > MAX_DATA {
        return Err("EFBIG:the file is too large".into());
    }
    let mut file = std::fs::File::open(&path).map_err(|_| "EBADF:can't read the file")?;
    if offset > 0 {
        std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(offset as u64))
            .map_err(|_| "EBADF:can't read the file")?;
    }
    let mut data = Vec::new();
    file.take(wanted as u64).read_to_end(&mut data).map_err(|_| "EBADF:can't read the file")?;
    if temporary && is_temporary(&path) {
        let _ = std::fs::remove_file(&path);
    }
    Ok(data)
}

/// Whether a file (its real path) is one a program made for the terminal
/// to read once and delete: in a folder for temporary files, and named as
/// the protocol says.
fn is_temporary(path: &Path) -> bool {
    let named = path.file_name().is_some_and(|name| name.to_string_lossy().contains("tty-graphics-protocol"));
    let folders = [std::env::temp_dir(), "/tmp".into(), "/dev/shm".into()];
    named && folders.iter().filter_map(|dir| std::fs::canonicalize(dir).ok()).any(|dir| path.starts_with(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a filter makes of output that arrives in these pieces: the text
    /// that is left, and the commands.
    fn filtered(pieces: &[&[u8]]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut filter = Filter::default();
        let (mut text, mut commands) = (Vec::new(), Vec::new());
        for piece in pieces {
            filter.feed(piece, |piece| match piece {
                Piece::Text(bytes) => text.extend_from_slice(bytes),
                Piece::Command(command) => commands.push(command),
            });
        }
        (text, commands)
    }

    #[test]
    fn commands_are_taken_out_of_the_text() {
        let (text, commands) = filtered(&[b"one\x1b_Ga=q;AAAA\x1b\\two\x1b[1mthree"]);
        assert_eq!(text, b"onetwo\x1b[1mthree");
        assert_eq!(commands, vec![b"a=q;AAAA".to_vec()]);
    }

    #[test]
    fn however_the_output_is_cut_up() {
        let whole: &[u8] = b"a\x1b[0mb\x1b_Gi=1;QUJD\x1b\\c\x1b_other\x1b\\d\x1b";
        let (text, commands) = filtered(&[whole]);
        assert_eq!(text, b"a\x1b[0mbc\x1b_other\x1b\\d");
        assert_eq!(commands, vec![b"i=1;QUJD".to_vec()]);
        // the last ESC is held back until what follows it is known
        for cut in 1..whole.len() {
            let (a, b) = whole.split_at(cut);
            assert_eq!(filtered(&[a, b]), (text.clone(), commands.clone()), "cut at {cut}");
        }
        let bytes: Vec<&[u8]> = whole.chunks(1).collect();
        assert_eq!(filtered(&bytes), (text, commands));
    }

    #[test]
    fn an_escape_inside_a_command_voids_it() {
        let (text, commands) = filtered(&[b"x\x1b_Ga=T;AAAA\x1b[31my"]);
        assert_eq!(text, b"x\x1b[31my");
        assert!(commands.is_empty());
    }

    #[test]
    fn keys_and_payload() {
        let (control, payload, bare) = parse(b"a=T,f=100,q=2,C=1,c=12,r=4,m=1;iVBORw0KGgo");
        assert!(!bare);
        assert_eq!(control.action, b'T');
        assert_eq!((control.format, control.quiet, control.stay), (100, 2, true));
        assert_eq!((control.cols, control.rows, control.more), (12, 4, true));
        assert_eq!(payload, b"iVBORw0KGgo");
        let (control, payload, bare) = parse(b"m=0");
        assert_eq!((control.action, control.format, control.medium), (b't', 32, b'd'));
        assert!(payload.is_empty() && bare);
    }

    #[test]
    fn chunks_make_one_image() {
        let mut graphics = Graphics::new(Shared::default());
        // 2x1 RGB pixels, sent in two pieces
        assert!(graphics.receive(b"a=T,f=24,s=2,v=1,m=1;/wAA").is_none());
        let step = graphics.receive(b"m=0;AP8A").unwrap();
        let image = step.image.unwrap().unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        // red and green, blue first
        assert_eq!(image.render.as_bytes(0).unwrap(), [0, 0, 255, 255, 0, 255, 0, 255]);
        assert_eq!(step.control.action, b'T');
    }

    #[test]
    fn an_image_left_unfinished_doesnt_swallow_what_follows() {
        let mut graphics = Graphics::new(Shared::default());
        // the program was stopped after the first chunk
        assert!(graphics.receive(b"a=T,f=24,s=2,v=1,m=1;/wAA").is_none());
        // a delete is itself, and a new image starts over
        let step = graphics.receive(b"a=d,d=a").unwrap();
        assert_eq!(step.control.action, b'd');
        let step = graphics.receive(b"a=T,f=24,s=1,v=1;AP8A").unwrap();
        let image = step.image.unwrap().unwrap();
        assert_eq!((image.width, image.height), (1, 1));
    }

    #[test]
    fn sizes_that_are_lies_are_refused() {
        let mut graphics = Graphics::new(Shared::default());
        let step = graphics.receive(b"a=T,f=32,s=4294967295,v=4294967295;AAAA").unwrap();
        assert!(step.image.unwrap().is_err());
        let step = graphics.receive(b"a=T,f=24,s=0,v=5;AAAA").unwrap();
        assert!(step.image.unwrap().is_err());
    }

    #[test]
    fn only_real_files_are_read() {
        assert!(read_file(Path::new("/proc/self/environ"), 0, 100, false).is_err());
        assert!(read_file(Path::new("/dev/zero"), 0, 100, false).is_err());
        assert!(read_file(Path::new("/tmp/../proc/version"), 0, 100, false).is_err());
        // a name that only looks temporary is not one
        assert!(!is_temporary(Path::new("/home/tty-graphics-protocol.png")));
    }

    #[test]
    fn bad_data_is_an_error_not_a_crash() {
        let mut graphics = Graphics::new(Shared::default());
        let step = graphics.receive(b"a=T,f=100;AAAA").unwrap();
        assert!(step.image.unwrap().is_err());
        let step = graphics.receive(b"a=T,f=24,s=100,v=100;AAAA").unwrap();
        assert!(step.image.unwrap().is_err());
        let step = graphics.receive(b"a=T,t=s;AAAA").unwrap();
        assert!(step.image.unwrap().is_err());
    }

    #[test]
    fn an_image_takes_the_cells_it_needs() {
        let cell = (10., 20.);
        // its own size, rounded up to whole cells
        assert_eq!(cells_for((25, 41), (0, 0), cell), (3, 3, (2.5, 2.05)));
        // as wide as asked, as tall as its shape makes it
        assert_eq!(cells_for((100, 100), (4, 0), cell), (4, 2, (4., 2.)));
        assert_eq!(cells_for((100, 100), (0, 2), cell), (4, 2, (4., 2.)));
        // both given: stretched over them
        assert_eq!(cells_for((100, 100), (7, 2), cell), (7, 2, (7., 2.)));
    }

    #[test]
    fn links_of_image_cells() {
        assert_eq!(cell_of("lumen-image:12:3:4"), Some((12, 3, 4)));
        assert_eq!(cell_of("https://example.com"), None);
        assert_eq!(cell_of("lumen-image:x"), None);
    }
}
