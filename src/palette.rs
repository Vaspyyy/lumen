//! From the colors a program asks for to the colors of the theme.

use alacritty_terminal::term::color::Colors as Overrides;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::Hsla;

use crate::config::{Colors, Rgb};

/// Color `index` of the 256-color palette: the theme's 16, the 6×6×6 cube,
/// the gray ramp. Above that are the named colors (foreground, background,
/// cursor and the dim variants).
pub fn indexed(index: usize, colors: &Colors) -> Rgb {
    match index {
        0..=7 => colors.normal.list()[index],
        8..=15 => colors.bright.list()[index - 8],
        16..=231 => {
            let i = index - 16;
            let level = |n: usize| if n == 0 { 0 } else { (55 + 40 * n) as u8 };
            Rgb(level(i / 36), level(i / 6 % 6), level(i % 6))
        },
        232..=255 => {
            let v = (8 + 10 * (index - 232)) as u8;
            Rgb(v, v, v)
        },
        _ => named(index, colors),
    }
}

fn named(index: usize, colors: &Colors) -> Rgb {
    const FOREGROUND: usize = NamedColor::Foreground as usize;
    const BACKGROUND: usize = NamedColor::Background as usize;
    const CURSOR: usize = NamedColor::Cursor as usize;
    const DIM_BLACK: usize = NamedColor::DimBlack as usize;
    const DIM_WHITE: usize = NamedColor::DimWhite as usize;
    const BRIGHT_FOREGROUND: usize = NamedColor::BrightForeground as usize;
    const DIM_FOREGROUND: usize = NamedColor::DimForeground as usize;
    let dim = |c: Rgb| Rgb((c.0 as f32 * 0.66) as u8, (c.1 as f32 * 0.66) as u8, (c.2 as f32 * 0.66) as u8);
    match index {
        FOREGROUND | BRIGHT_FOREGROUND => colors.primary.foreground,
        BACKGROUND => colors.primary.background,
        CURSOR => colors.cursor.cursor.unwrap_or(colors.primary.foreground),
        DIM_BLACK..=DIM_WHITE => dim(colors.normal.list()[index - DIM_BLACK]),
        DIM_FOREGROUND => dim(colors.primary.foreground),
        _ => colors.primary.foreground,
    }
}

/// A cell's color: what the program changed at run time (OSC 4, 10, 11)
/// wins over the theme.
pub fn resolve(color: Color, overrides: &Overrides, colors: &Colors) -> Hsla {
    let rgb = match color {
        Color::Spec(rgb) => Rgb(rgb.r, rgb.g, rgb.b),
        Color::Named(name) => match overrides[name] {
            Some(rgb) => Rgb(rgb.r, rgb.g, rgb.b),
            None => indexed(name as usize, colors),
        },
        Color::Indexed(i) => match overrides[i as usize] {
            Some(rgb) => Rgb(rgb.r, rgb.g, rgb.b),
            None => indexed(i as usize, colors),
        },
    };
    rgb.hsla()
}

/// The bright sibling of one of the eight plain colors, for bold text
/// where the theme asks for that.
pub fn brighten(color: Color) -> Color {
    match color {
        Color::Named(name) if (name as usize) < 8 => Color::Indexed(name as u8 + 8),
        Color::Indexed(i) if i < 8 => Color::Indexed(i + 8),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_256_colors() {
        let colors = Colors::default();
        assert_eq!(indexed(1, &colors), colors.normal.red);
        assert_eq!(indexed(9, &colors), colors.bright.red);
        assert_eq!(indexed(16, &colors), Rgb(0, 0, 0));
        assert_eq!(indexed(196, &colors), Rgb(255, 0, 0));
        assert_eq!(indexed(231, &colors), Rgb(255, 255, 255));
        assert_eq!(indexed(232, &colors), Rgb(8, 8, 8));
        assert_eq!(indexed(255, &colors), Rgb(238, 238, 238));
        assert_eq!(indexed(NamedColor::Background as usize, &colors), colors.primary.background);
    }
}
