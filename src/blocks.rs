//! The block elements, U+2580 to U+259F, as the parts of a cell they fill.
//!
//! A font draws these characters for its own line, not for the cell they
//! stand in, and smooths their edges: next to each other they leave seams,
//! which shows in a QR code, a bar or a picture made of half blocks. lumen
//! fills the parts itself, on the cell's own edges, so that they join.

use gpui::{Bounds, Pixels, point, px};

/// A part of a cell, in eighths of its width and height: left, top, right, bottom.
pub type Part = (u8, u8, u8, u8);

/// What a block element fills of its cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub parts: &'static [Part],
    /// How much of the color shows: a shade is the whole cell, seen through.
    pub cover: f32,
}

const UPPER_LEFT: Part = (0, 0, 4, 4);
const UPPER_RIGHT: Part = (4, 0, 8, 4);
const LOWER_LEFT: Part = (0, 4, 4, 8);
const LOWER_RIGHT: Part = (4, 4, 8, 8);
const UPPER: Part = (0, 0, 8, 4);
const LOWER: Part = (0, 4, 8, 8);
const WHOLE: Part = (0, 0, 8, 8);

/// The characters in the order of their code points, from U+2580.
static SHAPES: [(&[Part], f32); 32] = [
    (&[UPPER], 1.),                   // ▀
    (&[(0, 7, 8, 8)], 1.),            // ▁
    (&[(0, 6, 8, 8)], 1.),            // ▂
    (&[(0, 5, 8, 8)], 1.),            // ▃
    (&[LOWER], 1.),                   // ▄
    (&[(0, 3, 8, 8)], 1.),            // ▅
    (&[(0, 2, 8, 8)], 1.),            // ▆
    (&[(0, 1, 8, 8)], 1.),            // ▇
    (&[WHOLE], 1.),                   // █
    (&[(0, 0, 7, 8)], 1.),            // ▉
    (&[(0, 0, 6, 8)], 1.),            // ▊
    (&[(0, 0, 5, 8)], 1.),            // ▋
    (&[(0, 0, 4, 8)], 1.),            // ▌
    (&[(0, 0, 3, 8)], 1.),            // ▍
    (&[(0, 0, 2, 8)], 1.),            // ▎
    (&[(0, 0, 1, 8)], 1.),            // ▏
    (&[(4, 0, 8, 8)], 1.),            // ▐
    (&[WHOLE], 0.25),                 // ░
    (&[WHOLE], 0.5),                  // ▒
    (&[WHOLE], 0.75),                 // ▓
    (&[(0, 0, 8, 1)], 1.),            // ▔
    (&[(7, 0, 8, 8)], 1.),            // ▕
    (&[LOWER_LEFT], 1.),              // ▖
    (&[LOWER_RIGHT], 1.),             // ▗
    (&[UPPER_LEFT], 1.),              // ▘
    (&[UPPER_LEFT, LOWER], 1.),       // ▙
    (&[UPPER_LEFT, LOWER_RIGHT], 1.), // ▚
    (&[UPPER, LOWER_LEFT], 1.),       // ▛
    (&[UPPER, LOWER_RIGHT], 1.),      // ▜
    (&[UPPER_RIGHT], 1.),             // ▝
    (&[UPPER_RIGHT, LOWER_LEFT], 1.), // ▞
    (&[UPPER_RIGHT, LOWER], 1.),      // ▟
];

/// The shape of a block element, and nothing for every other character.
pub fn shape(c: char) -> Option<Shape> {
    let index = (c as u32).checked_sub(0x2580)? as usize;
    SHAPES.get(index).map(|&(parts, cover)| Shape { parts, cover })
}

/// `value` on the nearest whole pixel of a screen that draws `scale` pixels
/// for one of ours. Edges that are snapped like this meet exactly: two
/// rectangles that share one neither overlap nor leave a gap.
pub fn snap(value: Pixels, scale: f32) -> Pixels {
    px((f32::from(value) * scale).round() / scale)
}

impl Shape {
    /// Whether the shape is the same in every column of its cell: a row of
    /// such cells in one color is one shape as wide as the row.
    pub fn spans_width(&self) -> bool {
        self.parts.iter().all(|part| part.0 == 0 && part.2 == 8)
    }

    /// The rectangles that fill this shape in `cells`, whose own edges are
    /// on whole screen pixels. `cells` is one cell, or a row of them for a
    /// shape that spans their width.
    pub fn rects(self, cells: Bounds<Pixels>, scale: f32) -> impl Iterator<Item = Bounds<Pixels>> {
        let (left, top) = (cells.origin.x, cells.origin.y);
        let (right, bottom) = (left + cells.size.width, top + cells.size.height);
        let pixel = px(1. / scale);
        self.parts.iter().map(move |&(x0, y0, x1, y1)| {
            let (x0, x1) = span(left, right, x0, x1, scale, pixel);
            let (y0, y1) = span(top, bottom, y0, y1, scale, pixel);
            Bounds::from_corners(point(x0, y0), point(x1, y1))
        })
    }
}

/// The stretch between two eighths of what goes from `start` to `end`, on
/// whole screen pixels. A stretch that came out empty in a small cell is
/// one pixel, at the end it belongs to.
fn span(start: Pixels, end: Pixels, from: u8, to: u8, scale: f32, pixel: Pixels) -> (Pixels, Pixels) {
    let at = |eighth: u8| match eighth {
        0 => start,
        8 => end,
        eighth => snap(start + (end - start) * (eighth as f32 / 8.), scale),
    };
    let (mut from_at, mut to_at) = (at(from), at(to));
    if to_at <= from_at && end - start >= pixel {
        if to == 8 {
            from_at = to_at - pixel;
        } else {
            to_at = from_at + pixel;
        }
    }
    (from_at, to_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which of a cell's 8 by 8 squares a shape fills.
    fn filled(shape: Shape) -> [[bool; 8]; 8] {
        let mut squares = [[false; 8]; 8];
        for &(x0, y0, x1, y1) in shape.parts {
            for row in &mut squares[y0 as usize..y1 as usize] {
                for square in &mut row[x0 as usize..x1 as usize] {
                    assert!(!*square, "two parts of a shape overlap");
                    *square = true;
                }
            }
        }
        squares
    }

    fn count(shape: Shape) -> usize {
        filled(shape).iter().flatten().filter(|&&on| on).count()
    }

    #[test]
    fn only_block_elements_have_a_shape() {
        for c in '\u{2580}'..='\u{259f}' {
            let shape = shape(c).unwrap_or_else(|| panic!("{c} has no shape"));
            assert!(!shape.parts.is_empty());
            for &(x0, y0, x1, y1) in shape.parts {
                assert!(x0 < x1 && x1 <= 8 && y0 < y1 && y1 <= 8, "{c} has a part outside its cell");
            }
        }
        for c in ['\u{257f}', '\u{25a0}', ' ', 'a', '\u{2500}', '\u{1fb00}'] {
            assert_eq!(shape(c), None);
        }
    }

    #[test]
    fn the_shapes_fill_what_their_names_say() {
        let fills = |c: char| count(shape(c).unwrap());
        // the lower blocks grow by an eighth each, up to the full block
        for (eighths, c) in ('\u{2581}'..='\u{2588}').enumerate() {
            assert_eq!(fills(c), (eighths + 1) * 8, "{c}");
            assert!(filled(shape(c).unwrap())[7][0], "{c} does not stand on the cell's bottom");
        }
        // the left blocks shrink by an eighth each, from the full block
        for (eighths, c) in ('\u{2588}'..='\u{258f}').enumerate() {
            assert_eq!(fills(c), (8 - eighths) * 8, "{c}");
            assert!(filled(shape(c).unwrap())[0][0], "{c} does not start at the cell's left");
        }
        // halves and quadrants that are each other's rest
        for (a, b) in [('▀', '▄'), ('▌', '▐'), ('▚', '▞'), ('▖', '▜'), ('▗', '▛'), ('▘', '▟'), ('▝', '▙')]
        {
            let (one, other) = (filled(shape(a).unwrap()), filled(shape(b).unwrap()));
            assert!(one.iter().flatten().zip(other.iter().flatten()).all(|(one, other)| one != other), "{a} and {b}");
        }
        assert!(filled(shape('▀').unwrap())[0][0]);
        assert!(filled(shape('▐').unwrap())[0][7]);
        assert!(filled(shape('▔').unwrap())[0][7] && fills('▔') == 8);
        assert!(filled(shape('▕').unwrap())[7][7] && fills('▕') == 8);
        assert!(filled(shape('▘').unwrap())[0][0] && fills('▘') == 16);
        assert!(filled(shape('▗').unwrap())[7][7] && fills('▗') == 16);
        // a shade is the whole cell, the darker the more of it shows
        let covers: Vec<f32> = ['░', '▒', '▓', '█'].iter().map(|&c| shape(c).unwrap().cover).collect();
        assert!(covers.windows(2).all(|pair| pair[0] < pair[1]) && covers[3] == 1.);
        assert!(['░', '▒', '▓'].iter().all(|&c| fills(c) == 64));
    }

    #[test]
    fn shapes_that_span_the_width_are_known() {
        for c in ['▀', '▁', '▄', '▇', '█', '░', '▓', '▔'] {
            assert!(shape(c).unwrap().spans_width(), "{c}");
        }
        for c in ['▉', '▌', '▏', '▐', '▕', '▖', '▚', '▙', '▟'] {
            assert!(!shape(c).unwrap().spans_width(), "{c}");
        }
    }

    /// A row of cells whose width is no whole number of pixels, as a grid
    /// lays them out: every edge snapped.
    fn row_of_cells(scale: f32, width: f32, height: f32) -> Vec<Bounds<Pixels>> {
        let edge = |col: usize| snap(px(10. + width * col as f32), scale);
        let (top, bottom) = (snap(px(7.), scale), snap(px(7. + height), scale));
        (0..40).map(|col| Bounds::from_corners(point(edge(col), top), point(edge(col + 1), bottom))).collect()
    }

    /// The screen pixel an edge is on, which it has to be on exactly.
    fn pixel(value: Pixels, scale: f32) -> i32 {
        let on_screen = f32::from(value) * scale;
        assert!((on_screen - on_screen.round()).abs() < 1e-3, "{value:?} is between two pixels at scale {scale}");
        on_screen.round() as i32
    }

    /// A rectangle's edges in screen pixels: left, top, right, bottom.
    fn pixels(rect: Bounds<Pixels>, scale: f32) -> (i32, i32, i32, i32) {
        let on = |value| pixel(value, scale);
        (on(rect.left()), on(rect.top()), on(rect.right()), on(rect.bottom()))
    }

    #[test]
    fn rectangles_are_on_whole_pixels_and_join() {
        for scale in [1., 1.25, 1.5, 2.] {
            for (width, height) in [(9.6, 22.), (7.7, 17.), (8.5, 19.), (3., 5.)] {
                let cells = row_of_cells(scale, width, height);
                for c in '\u{2580}'..='\u{259f}' {
                    let shape = shape(c).unwrap();
                    for pair in cells.windows(2) {
                        let cell = pixels(pair[0], scale);
                        let rects: Vec<_> = shape.rects(pair[0], scale).map(|rect| pixels(rect, scale)).collect();
                        for &(left, top, right, bottom) in &rects {
                            assert!(left < right && top < bottom, "{c} at scale {scale} fills nothing");
                            assert!(left >= cell.0 && top >= cell.1 && right <= cell.2 && bottom <= cell.3, "{c}");
                        }
                        if shape.spans_width() {
                            // it ends where the same shape in the next cell begins
                            let next = pixels(shape.rects(pair[1], scale).next().unwrap(), scale);
                            assert_eq!(rects[0].2, next.0, "{c} at scale {scale}");
                            assert_eq!((rects[0].1, rects[0].3), (next.1, next.3), "{c} at scale {scale}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn halves_meet_in_the_middle() {
        for scale in [1., 1.5] {
            let cell = row_of_cells(scale, 9.6, 21.)[4];
            let whole = pixels(cell, scale);
            let one = |c: char| pixels(shape(c).unwrap().rects(cell, scale).next().unwrap(), scale);
            let (upper, lower, left, right) = (one('▀'), one('▄'), one('▌'), one('▐'));
            assert_eq!(upper.3, lower.1);
            assert_eq!(left.2, right.0);
            assert_eq!((upper.0, upper.1, upper.2), (whole.0, whole.1, whole.2));
            assert_eq!((lower.0, lower.2, lower.3), (whole.0, whole.2, whole.3));
            assert_eq!((left.0, left.1, left.3), (whole.0, whole.1, whole.3));
            assert_eq!((right.1, right.2, right.3), (whole.1, whole.2, whole.3));
            assert_eq!(one('█'), whole);
            // the halves are as even as the pixels allow
            assert!(((upper.3 - upper.1) - (lower.3 - lower.1)).abs() <= 1);
            assert!(((left.2 - left.0) - (right.2 - right.0)).abs() <= 1);
        }
    }

    #[test]
    fn a_row_of_cells_is_one_rectangle() {
        let scale = 1.;
        let cells = row_of_cells(scale, 9.6, 22.);
        let row = Bounds::from_corners(cells[3].origin, cells[8].bottom_right());
        let rects: Vec<_> = shape('▀').unwrap().rects(row, scale).map(|rect| pixels(rect, scale)).collect();
        let (first, last) = (pixels(cells[3], scale), pixels(cells[8], scale));
        let alone = pixels(shape('▀').unwrap().rects(cells[3], scale).next().unwrap(), scale);
        assert_eq!(rects, [(first.0, first.1, last.2, alone.3)]);
    }
}
