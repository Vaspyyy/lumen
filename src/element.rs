//! Draws a terminal's grid.
//!
//! Each frame the visible cells are read from the terminal under its lock
//! into plain runs of same-styled text and background rectangles; the lock
//! is released before any text is shaped.

use std::cell::Cell as StdCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::Match;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use gpui::{
    App, BorderStyle, Bounds, BoxShadow, Corners, CursorStyle, DispatchPhase, Element, ElementId, ElementInputHandler,
    Entity, FocusHandle, Font, FontStyle, FontWeight, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    RenderImage, ScrollEvent, ShapedLine, Size, StrikethroughStyle, Style, TextAlign, TextRun, UnderlineStyle, Window,
    fill, font, point, px, quad, relative, size, transparent_black,
};

use crate::blocks::{self, snap};
use crate::config::Config;
use crate::graphics;
use crate::palette;
use crate::terminal::{TermSize, Terminal};
use crate::view::TerminalView;

/// Where the grid is on screen: what turns a mouse position into a cell.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    /// The pane.
    pub bounds: Bounds<Pixels>,
    /// The top left of the grid, which is inset from the pane's.
    pub origin: Point<Pixels>,
    pub cell: Size<Pixels>,
    pub cols: usize,
    pub rows: usize,
    /// The cursor's cell, while it is on screen.
    pub cursor: Option<Bounds<Pixels>>,
}

impl Geometry {
    /// The cell under `position` (clamped into the grid) and whether the
    /// position is in the cell's right half.
    pub fn cell_at(&self, position: Point<Pixels>) -> (usize, usize, bool) {
        let x = ((position.x - self.origin.x) / self.cell.width).max(0.);
        let y = ((position.y - self.origin.y) / self.cell.height).max(0.);
        let col = (x as usize).min(self.cols.saturating_sub(1));
        let row = (y as usize).min(self.rows.saturating_sub(1));
        (col, row, x.fract() >= 0.5)
    }
}

/// The font and the size of a cell, for a config and a pane's zoom.
pub struct Metrics {
    pub font: Font,
    pub font_size: Pixels,
    pub cell: Size<Pixels>,
}

impl Metrics {
    pub fn new(config: &Config, zoom: f32, window: &Window) -> Metrics {
        let text_system = window.text_system();
        let font_size = px(config.font.size * 4. / 3. * zoom);
        let font = font(crate::fonts::grid(&config.font.family, text_system));
        let font_id = text_system.resolve_font(&font);
        let width = text_system.advance(font_id, font_size, 'm').map(|s| s.width).unwrap_or(font_size * 0.6);
        let ascent = f32::from(text_system.ascent(font_id, font_size));
        let descent = f32::from(text_system.descent(font_id, font_size));
        let height = px(((ascent + descent.abs()) * config.font.line_height).ceil().max(1.));
        Metrics { font, font_size, cell: size(width, height) }
    }

    /// The grid that fits into an area of this size.
    pub fn grid(&self, area: Size<Pixels>, config: &Config) -> TermSize {
        let pad = px(config.window.padding);
        TermSize {
            cols: (((area.width - pad * 2.) / self.cell.width) as usize).max(2),
            rows: (((area.height - pad * 2.) / self.cell.height) as usize).max(1),
            cell_width: self.cell.width.into(),
            cell_height: self.cell.height.into(),
        }
    }
}

/// The cursor on its way from one cell to another.
#[derive(Clone, Copy, Default)]
pub struct Motion {
    from: (f32, f32),
    to: (f32, f32),
    start: Option<Instant>,
    /// Whether the cursor was on screen the frame before: it glides from
    /// where it was, and appears where it is.
    shown: bool,
}

const GLIDE: Duration = Duration::from_millis(70);
const FADE: Duration = Duration::from_millis(140);

impl Motion {
    fn at(&self, now: Instant) -> ((f32, f32), bool) {
        let Some(start) = self.start else { return (self.to, false) };
        let t = now.duration_since(start).as_secs_f32() / GLIDE.as_secs_f32();
        if t >= 1. {
            return (self.to, false);
        }
        // fast out of the old cell, soft into the new one
        let eased = 1. - (1. - t).powi(3);
        let lerp = |a: f32, b: f32| a + (b - a) * eased;
        ((lerp(self.from.0, self.to.0), lerp(self.from.1, self.to.1)), true)
    }

    /// Where the cursor is drawn now that it belongs at `target`, and
    /// whether it is still moving.
    fn toward(&mut self, target: (f32, f32), now: Instant) -> ((f32, f32), bool) {
        if !self.shown {
            *self = Motion { from: target, to: target, start: None, shown: true };
        } else if self.to != target {
            let (here, _) = self.at(now);
            *self = Motion { from: here, to: target, start: Some(now), shown: true };
        }
        self.at(now)
    }
}

pub struct TerminalElement {
    pub terminal: Entity<Terminal>,
    pub view: Entity<TerminalView>,
    pub focus: FocusHandle,
    pub geometry: Rc<StdCell<Option<Geometry>>>,
    /// Whether the window and this terminal have the keyboard.
    pub focused: bool,
    /// The blink's current half, and when it began.
    pub cursor_on: bool,
    pub blink_at: Instant,
    pub motion: Rc<StdCell<Motion>>,
    pub zoom: f32,
    /// The cells of the link a click would follow: row, column, length.
    pub link: Vec<(usize, usize, usize)>,
    /// What a search found, and which match it is on.
    pub search: Option<(Rc<[Match]>, Option<usize>)>,
    /// Text an input method is composing, shown at the cursor.
    pub composing: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Line {
    None,
    Single,
    Double,
    Curl,
}

#[derive(Clone, PartialEq)]
struct RunStyle {
    fg: Hsla,
    bold: bool,
    italic: bool,
    underline: Line,
    underline_color: Option<Hsla>,
    strike: bool,
}

/// Cells in a row that share a style, as one piece of text.
struct Run {
    row: usize,
    col: usize,
    /// How many cells the text covers so far.
    cells: usize,
    text: String,
    style: RunStyle,
}

struct CursorLayout {
    bounds: Bounds<Pixels>,
    shape: CursorShape,
    color: Hsla,
    /// The cursor gives off light.
    glow: bool,
    /// The character under a block cursor, in the cursor's text color.
    text: Option<ShapedLine>,
    /// A block element under a block cursor, in that color.
    block: Option<(blocks::Shape, Hsla)>,
}

/// A part of an image, as far as its cells are on screen.
struct ImageLayout {
    /// The cells.
    clip: Bounds<Pixels>,
    /// The whole image, of which the cells show a part.
    bounds: Bounds<Pixels>,
    image: Arc<RenderImage>,
}

pub struct Layout {
    hitbox: Hitbox,
    cell: Size<Pixels>,
    rects: Vec<(Bounds<Pixels>, Hsla)>,
    images: Vec<ImageLayout>,
    /// The block elements, which are filled and not written.
    fills: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    /// What is drawn over the text: the line under a link, text being composed.
    marks: Vec<(Bounds<Pixels>, Hsla)>,
    composing: Option<(Point<Pixels>, ShapedLine)>,
    cursor: Option<CursorLayout>,
    pointer: CursorStyle,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl TerminalElement {
    fn text_run(&self, base: &Font, style: &RunStyle, len: usize, thickness: Pixels) -> TextRun {
        TextRun {
            len,
            font: Font {
                weight: if style.bold { FontWeight::BOLD } else { FontWeight::NORMAL },
                style: if style.italic { FontStyle::Italic } else { FontStyle::Normal },
                ..base.clone()
            },
            color: style.fg,
            background_color: None,
            underline: match style.underline {
                Line::None => None,
                // a second line can't be drawn by the text run: a thicker one stands in
                Line::Single => Some(UnderlineStyle { thickness, color: style.underline_color, wavy: false }),
                Line::Double => {
                    Some(UnderlineStyle { thickness: thickness * 2., color: style.underline_color, wavy: false })
                },
                Line::Curl => Some(UnderlineStyle { thickness, color: style.underline_color, wavy: true }),
            },
            strikethrough: style.strike.then_some(StrikethroughStyle { thickness, color: Some(style.fg) }),
        }
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Layout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Layout {
        let config = cx.global::<Config>().clone();
        let colors = &config.colors;
        let text_system = window.text_system().clone();
        let Metrics { font: base, font_size, cell } = Metrics::new(&config, self.zoom, window);
        let (cell_width, cell_height) = (cell.width, cell.height);
        let thickness = px((f32::from(font_size) / 14.).round().max(1.));

        let pad = px(config.window.padding);
        let term_size = Metrics { font: base.clone(), font_size, cell }.grid(bounds.size, &config);
        let (cols, rows) = (term_size.cols, term_size.rows);
        let origin = point((bounds.origin.x + pad).round(), (bounds.origin.y + pad).round());
        self.terminal.update(cx, |terminal, _| terminal.resize(term_size));
        let mut geometry = Geometry { bounds, origin, cell, cols, rows, cursor: None };
        self.geometry.set(Some(geometry));
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);

        // Cells are filled on whole screen pixels, each up to where the next begins:
        // what is filled in cells next to each other joins, without a seam or an overlap.
        let scale = window.scale_factor();
        let cell_bounds = |col: usize, row: usize, cells: usize| {
            let x = |col: usize| snap(origin.x + cell_width * col as f32, scale);
            let y = |row: usize| snap(origin.y + cell_height * row as f32, scale);
            Bounds::from_corners(point(x(col), y(row)), point(x(col + cells), y(row + 1)))
        };

        // Read the grid. Nothing slow happens while the lock is held.
        let mut backgrounds: Vec<(usize, usize, usize, Hsla)> = Vec::new();
        let mut runs: Vec<Run> = Vec::new();
        // the block elements: row, column, cells, shape, color
        let mut block_cells: Vec<(usize, usize, usize, blocks::Shape, Hsla)> = Vec::new();
        // the cells that show part of an image: row, column, placement, column and row in the image
        let mut image_cells: Vec<(usize, usize, u32, usize, usize)> = Vec::new();
        let mut found: Vec<(usize, usize, usize, Hsla)> = Vec::new();
        let accent = config.accent();
        let cursor;
        let pointer;
        {
            let terminal = self.terminal.read(cx);
            let term = terminal.term.lock();
            let content = term.renderable_content();
            let offset = content.display_offset as i32;
            let overrides = content.colors;
            let selection_bg = colors.selection.background.map(|c| c.hsla());
            let selection_fg = colors.selection.text.map(|c| c.hsla());
            let mut run: Option<Run> = None;
            pointer =
                if content.mode.intersects(TermMode::MOUSE_MODE) { CursorStyle::Arrow } else { CursorStyle::IBeam };

            // what the search found, as far as it is on screen
            let mut on_match = None;
            if let Some((matches, current)) = &self.search {
                on_match = current.and_then(|i| matches.get(i)).cloned();
                let (top, bottom) = (-offset, rows as i32 - 1 - offset);
                let first = matches.partition_point(|m| m.end().line.0 < top);
                for found_match in matches[first..].iter().take_while(|m| m.start().line.0 <= bottom) {
                    if Some(found_match) == on_match.as_ref() {
                        continue;
                    }
                    let (start, end) = (found_match.start(), found_match.end());
                    for line in start.line.0.max(top)..=end.line.0.min(bottom) {
                        let from = if line == start.line.0 { start.column.0 } else { 0 };
                        let to = if line == end.line.0 { end.column.0 } else { cols.saturating_sub(1) };
                        found.push(((line + offset) as usize, from, to + 1 - from.min(to), accent.opacity(0.3)));
                    }
                }
            }

            for indexed in content.display_iter {
                let row = indexed.point.line.0 + offset;
                if row < 0 || row as usize >= rows {
                    continue;
                }
                let (row, col) = (row as usize, indexed.point.column.0);
                let cell = indexed.cell;
                if let Some(link) = cell.hyperlink()
                    && let Some((placement, image_col, image_row)) = graphics::cell_of(link.uri())
                {
                    image_cells.push((row, col, placement, image_col, image_row));
                    continue;
                }
                let flags = cell.flags;
                if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                let wide = flags.contains(Flags::WIDE_CHAR);
                let width = if wide { 2 } else { 1 };

                let (mut fg, mut bg) = (cell.fg, cell.bg);
                if flags.contains(Flags::BOLD) && colors.draw_bold_text_with_bright_colors {
                    fg = palette::brighten(fg);
                }
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let selected = content.selection.is_some_and(|s| s.contains(indexed.point));
                let mut fg_color = palette::resolve(fg, overrides, colors);
                if flags.contains(Flags::DIM) {
                    fg_color.a *= 0.6;
                }
                let default_bg = bg == Color::Named(NamedColor::Background);
                if on_match.as_ref().is_some_and(|m| m.contains(&indexed.point)) {
                    // the match the search is on: lit like the lamp, with the text dark on it
                    push_background(&mut backgrounds, row, col, width, accent);
                    fg_color = colors.primary.background.hsla();
                } else if selected {
                    // a theme without a selection color gets a see-through wash of the text color
                    let wash = selection_bg.unwrap_or_else(|| Hsla { a: 0.28, ..colors.primary.foreground.hsla() });
                    push_background(&mut backgrounds, row, col, width, wash);
                    if let Some(color) = selection_fg {
                        fg_color = color;
                    }
                } else if !default_bg {
                    push_background(&mut backgrounds, row, col, width, palette::resolve(bg, overrides, colors));
                }

                let underline = if flags
                    .intersects(Flags::UNDERCURL | Flags::DOTTED_UNDERLINE | Flags::DASHED_UNDERLINE)
                    && flags.contains(Flags::UNDERCURL)
                {
                    Line::Curl
                } else if flags.contains(Flags::DOUBLE_UNDERLINE) {
                    Line::Double
                } else if flags.intersects(Flags::ALL_UNDERLINES) {
                    Line::Single
                } else {
                    Line::None
                };
                let strike = flags.contains(Flags::STRIKEOUT);
                // a block element is filled, not written: its cell is a blank one to the text
                let shape = blocks::shape(cell.c)
                    .filter(|_| !wide && !flags.contains(Flags::HIDDEN) && cell.zerowidth().is_none());
                if let Some(shape) = shape {
                    push_block(&mut block_cells, row, col, shape, fg_color);
                }
                let c = if shape.is_some() { ' ' } else { cell.c };
                let blank =
                    (c == ' ' || c == '\0' || flags.contains(Flags::HIDDEN)) && underline == Line::None && !strike;
                let style = RunStyle {
                    fg: fg_color,
                    bold: flags.contains(Flags::BOLD),
                    italic: flags.contains(Flags::ITALIC),
                    underline,
                    underline_color: cell.underline_color().map(|c| palette::resolve(c, overrides, colors)),
                    strike,
                };

                // A run goes on while the next cell is the next column in the same style.
                // Blank cells join it as spaces, and are cut off again if nothing follows;
                // not where the run is underlined or struck through, or the line would go on.
                let continues = run.as_ref().is_some_and(|r| {
                    let plain = r.style.underline == Line::None && !r.style.strike;
                    r.row == row && r.col + r.cells == col && ((blank && plain) || r.style == style) && !wide
                });
                if !continues {
                    if let Some(done) = run.take() {
                        finish(&mut runs, done);
                    }
                    if blank {
                        continue;
                    }
                    run = Some(Run { row, col, cells: 0, text: String::new(), style });
                }
                let current = run.as_mut().unwrap();
                current.text.push(if blank { ' ' } else { c });
                if !blank {
                    for &c in cell.zerowidth().into_iter().flatten() {
                        current.text.push(c);
                    }
                }
                current.cells += width;
                // a wide character is a run of its own: its glyph spans two cells,
                // and so do marks stacked on a character
                if wide || cell.zerowidth().is_some() {
                    finish(&mut runs, run.take().unwrap());
                }
            }
            if let Some(done) = run.take() {
                finish(&mut runs, done);
            }

            let point = content.cursor.point;
            let row = point.line.0 + offset;
            let mut shape = content.cursor.shape;
            if !self.focused && shape != CursorShape::Hidden {
                shape = CursorShape::HollowBlock;
            }
            let visible = shape != CursorShape::Hidden
                && row >= 0
                && (row as usize) < rows
                && content.mode.contains(TermMode::SHOW_CURSOR);
            cursor = visible.then(|| {
                let under = &term.grid()[point];
                let wide = under.flags.contains(Flags::WIDE_CHAR);
                (row as usize, point.column.0, if wide { 2 } else { 1 }, shape, under.c, under.flags)
            });
        }

        let mut rects = Vec::with_capacity(backgrounds.len() + found.len());
        for (row, col, cells, color) in backgrounds.into_iter().chain(found) {
            rects.push((cell_bounds(col, row, cells), color));
        }

        let mut fills = Vec::with_capacity(block_cells.len());
        for (row, col, cells, shape, color) in block_cells {
            let color = color.opacity(shape.cover);
            fills.extend(shape.rects(cell_bounds(col, row, cells), scale).map(|rect| (rect, color)));
        }

        // Cells of one image that are next to each other are one piece of it.
        // A piece: row, column, columns, rows, placement, column and row in the image.
        let mut pieces: Vec<(usize, usize, usize, usize, u32, usize, usize)> = Vec::new();
        for (row, col, placement, image_col, image_row) in image_cells {
            if let Some(p) = pieces.last_mut()
                && (p.0, p.3, p.4, p.6) == (row, 1, placement, image_row)
                && p.1 + p.2 == col
                && p.5 + p.2 == image_col
            {
                p.2 += 1;
                continue;
            }
            pieces.push((row, col, 1, 1, placement, image_col, image_row));
        }
        let mut blocks: Vec<(usize, usize, usize, usize, u32, usize, usize)> = Vec::new();
        for piece in pieces {
            let above = blocks.iter_mut().find(|b| {
                (b.1, b.2, b.4, b.5) == (piece.1, piece.2, piece.4, piece.5)
                    && b.0 + b.3 == piece.0
                    && b.6 + b.3 == piece.6
            });
            match above {
                Some(block) => block.3 += 1,
                None => blocks.push(piece),
            }
        }
        let mut images = Vec::with_capacity(blocks.len());
        if !blocks.is_empty() {
            let terminal = self.terminal.read(cx);
            let store = terminal.images.lock().unwrap();
            for (row, col, width, height, placement, image_col, image_row) in blocks {
                let Some(placement) = store.placement(placement) else { continue };
                let cell_origin = |col: f32, row: f32| point(origin.x + cell_width * col, origin.y + cell_height * row);
                images.push(ImageLayout {
                    clip: Bounds::new(
                        cell_origin(col as f32, row as f32),
                        size(cell_width * width as f32, cell_height * height as f32),
                    ),
                    bounds: Bounds::new(
                        cell_origin(col as f32 - image_col as f32, row as f32 - image_row as f32),
                        size(cell_width * placement.extent.0, cell_height * placement.extent.1),
                    ),
                    image: placement.image.render.clone(),
                });
            }
        }

        // the link a click would follow is underlined
        let mut marks = Vec::new();
        for &(row, col, len) in &self.link {
            let cells = cell_bounds(col, row, len);
            let line = Bounds::new(
                point(cells.origin.x, cells.origin.y + cell_height - thickness - px(1.)),
                size(cells.size.width, thickness),
            );
            marks.push((line, accent));
        }
        let mut lines = Vec::with_capacity(runs.len());
        for run in runs {
            let glyphs = run.text.chars().count();
            // each glyph is placed on the grid: one cell apart, or two for a wide character
            let advance = cell_width * (run.cells as f32 / glyphs.max(1) as f32).max(1.);
            let text_run = self.text_run(&base, &run.style, run.text.len(), thickness);
            let line = text_system.shape_line(run.text.into(), font_size, &[text_run], Some(advance));
            lines.push((point(origin.x + cell_width * run.col as f32, origin.y + cell_height * run.row as f32), line));
        }

        // Text an input method is composing stands at the cursor, which moves on behind it.
        let mut composing = None;
        let mut composed = px(0.);
        if let (Some(text), Some((row, col, ..))) = (&self.composing, cursor) {
            let style = RunStyle {
                fg: colors.primary.foreground.hsla(),
                bold: false,
                italic: false,
                underline: Line::Single,
                underline_color: Some(accent),
                strike: false,
            };
            let run = self.text_run(&base, &style, text.len(), thickness);
            let line = text_system.shape_line(text.clone().into(), font_size, &[run], None);
            let at = cell_bounds(col, row, 1).origin;
            composed = (line.width / cell_width).ceil() * cell_width;
            marks.push((Bounds::new(at, size(composed, cell_height)), colors.primary.background.hsla()));
            composing = Some((at, line));
        }

        if let Some((row, col, cells, ..)) = cursor {
            geometry.cursor = Some(cell_bounds(col, row, cells));
            self.geometry.set(Some(geometry));
        }

        let now = Instant::now();
        let mut motion = self.motion.get();
        if cursor.is_none() {
            motion.shown = false;
        }
        let cursor = cursor.map(|(row, col, cells, shape, c, flags)| {
            // the blink fades between its halves; a pane without the keyboard doesn't blink
            let since = now.duration_since(self.blink_at).as_secs_f32() / FADE.as_secs_f32();
            // it dims instead of going out, so it can always be found
            const DIM: f32 = 0.22;
            let lit = if !self.focused {
                1.
            } else if self.cursor_on {
                DIM + (1. - DIM) * since.min(1.)
            } else {
                1. - (1. - DIM) * since.min(1.)
            };
            if self.focused && since < 1. {
                window.request_animation_frame();
            }
            let color = config.cursor_color().opacity(lit);
            let mut bounds = cell_bounds(col, row, cells);
            bounds.origin.x += composed;
            if config.cursor.smooth {
                let target = (f32::from(bounds.origin.x - origin.x), f32::from(bounds.origin.y - origin.y));
                let ((x, y), moving) = motion.toward(target, now);
                bounds.origin = point(origin.x + px(x), origin.y + px(y));
                if moving {
                    window.request_animation_frame();
                }
            }
            let ink = colors.cursor.text.unwrap_or(colors.primary.background).hsla().opacity(lit);
            let block = blocks::shape(c)
                .filter(|_| shape == CursorShape::Block && cells == 1 && !flags.contains(Flags::HIDDEN))
                .map(|block| (block, ink.opacity(block.cover)));
            let text = (shape == CursorShape::Block && c != ' ' && c != '\0' && block.is_none()).then(|| {
                let style = RunStyle {
                    fg: ink,
                    bold: flags.contains(Flags::BOLD),
                    italic: flags.contains(Flags::ITALIC),
                    underline: Line::None,
                    underline_color: None,
                    strike: false,
                };
                let text = c.to_string();
                let run = self.text_run(&base, &style, text.len(), thickness);
                text_system.shape_line(text.into(), font_size, &[run], Some(cell_width * cells as f32))
            });
            let glow = config.cursor.glow && self.focused && shape != CursorShape::HollowBlock;
            CursorLayout { bounds, shape, color, glow, text, block }
        });
        self.motion.set(motion);

        Layout { hitbox, cell, rects, images, fills, lines, marks, composing, cursor, pointer }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        layout: &mut Layout,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(&self.focus, ElementInputHandler::new(bounds, self.view.clone()), cx);

        window.set_cursor_style(layout.pointer, &layout.hitbox);

        for (rect, color) in &layout.rects {
            window.paint_quad(fill(*rect, *color));
        }
        for image in &layout.images {
            let _ = window.paint_image(image.clip, image.bounds, Corners::default(), image.image.clone(), 0, false);
        }
        for (rect, color) in &layout.fills {
            window.paint_quad(fill(*rect, *color));
        }
        for (origin, line) in &layout.lines {
            let _ = line.paint(*origin, layout.cell.height, TextAlign::Left, None, window, cx);
        }
        for (rect, color) in &layout.marks {
            window.paint_quad(fill(*rect, *color));
        }
        if let Some((origin, line)) = &layout.composing {
            let _ = line.paint(*origin, layout.cell.height, TextAlign::Left, None, window, cx);
        }
        if let Some(cursor) = &layout.cursor {
            let b = cursor.bounds;
            let height = f32::from(layout.cell.height);
            let stroke = px((height / 9.).round().max(1.));
            // the part of the cell the cursor's shape fills
            let lit = match cursor.shape {
                CursorShape::Beam => Bounds::new(b.origin, size(stroke, b.size.height)),
                CursorShape::Underline => {
                    Bounds::new(point(b.origin.x, b.origin.y + b.size.height - stroke), size(b.size.width, stroke))
                },
                _ => b,
            };
            if cursor.glow {
                // lumen's cursor is a lamp: a bright rim close to it, a soft light further out
                let light = |strength: f32, reach: f32| BoxShadow {
                    color: cursor.color.opacity(strength),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(reach),
                    spread_radius: px(0.),
                    inset: false,
                };
                let lights = [light(0.5, height * 0.3), light(0.3, height * 1.2)];
                window.paint_drop_shadows(lit, Corners::all(px(2.)), &lights);
            }
            let solid = |bounds: Bounds<Pixels>, radius: f32| {
                quad(bounds, px(radius), cursor.color, px(0.), transparent_black(), BorderStyle::default())
            };
            match cursor.shape {
                CursorShape::Block => {
                    window.paint_quad(solid(lit, 2.));
                    if let Some(text) = &cursor.text {
                        let _ = text.paint(b.origin, layout.cell.height, TextAlign::Left, None, window, cx);
                    }
                    if let Some((block, ink)) = cursor.block {
                        for rect in block.rects(b, window.scale_factor()) {
                            window.paint_quad(fill(rect, ink));
                        }
                    }
                },
                CursorShape::Beam | CursorShape::Underline => window.paint_quad(solid(lit, 1.)),
                CursorShape::HollowBlock => {
                    let line = px(1.);
                    let (o, s) = (b.origin, b.size);
                    for edge in [
                        Bounds::new(o, size(s.width, line)),
                        Bounds::new(point(o.x, o.y + s.height - line), size(s.width, line)),
                        Bounds::new(o, size(line, s.height)),
                        Bounds::new(point(o.x + s.width - line, o.y), size(line, s.height)),
                    ] {
                        window.paint_quad(fill(edge, cursor.color));
                    }
                },
                CursorShape::Hidden => {},
            }
        }

        let view = self.view.clone();
        let hitbox = layout.hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                view.update(cx, |view, cx| view.mouse_down(event, window, cx));
            }
        });
        let view = self.view.clone();
        let hitbox = layout.hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                let hovered = hitbox.is_hovered(window);
                view.update(cx, |view, cx| view.mouse_move(event, hovered, window, cx));
            }
        });
        let view = self.view.clone();
        let hitbox = layout.hitbox.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                let hovered = hitbox.is_hovered(window);
                view.update(cx, |view, cx| view.mouse_up(event, hovered, window, cx));
            }
        });
        let view = self.view.clone();
        let hitbox = layout.hitbox.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
                view.update(cx, |view, cx| view.scroll_wheel(event, window, cx));
            }
        });
    }
}

/// Adds a cell's background, growing the rectangle before it when that one
/// ends where this cell starts and has the same color.
fn push_background(list: &mut Vec<(usize, usize, usize, Hsla)>, row: usize, col: usize, cells: usize, color: Hsla) {
    if let Some(last) = list.last_mut()
        && last.0 == row
        && last.1 + last.2 == col
        && last.3 == color
    {
        last.2 += cells;
        return;
    }
    list.push((row, col, cells, color));
}

/// Adds a cell's block element. A row of cells with the same one in the same
/// color is one entry, when the shape is the same across the cell's width.
fn push_block(
    list: &mut Vec<(usize, usize, usize, blocks::Shape, Hsla)>,
    row: usize,
    col: usize,
    shape: blocks::Shape,
    color: Hsla,
) {
    if let Some(last) = list.last_mut()
        && last.0 == row
        && last.1 + last.2 == col
        && last.3 == shape
        && last.4 == color
        && shape.spans_width()
    {
        last.2 += 1;
        return;
    }
    list.push((row, col, 1, shape, color));
}

/// Ends a run: the blank cells at its end are not text.
fn finish(runs: &mut Vec<Run>, mut run: Run) {
    let trimmed = run.text.trim_end_matches(' ').len();
    if trimmed == 0 {
        return;
    }
    if run.style.underline == Line::None && !run.style.strike {
        run.cells -= run.text.len() - trimmed;
        run.text.truncate(trimmed);
    }
    runs.push(run);
}
