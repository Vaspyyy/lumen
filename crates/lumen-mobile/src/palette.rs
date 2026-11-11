//! From the colors a program asks for to the colors of lumen's theme: a
//! night sky with a warm lamp in it. The same colors as the desktop's
//! defaults (`src/config.rs` there).

use alacritty_terminal::term::color::Colors as Overrides;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// A color as `0xRRGGBB`.
pub type Hue = u32;

pub const BACKGROUND: Hue = 0x12111a;
pub const FOREGROUND: Hue = 0xe4e1ee;
/// The cursor is the lamp.
pub const CURSOR: Hue = 0xffd9a0;
pub const CURSOR_TEXT: Hue = BACKGROUND;
pub const ACCENT: Hue = 0xffc97a;

const NORMAL: [Hue; 8] = [0x232130, 0xf27a93, 0x8fdfa8, 0xffd08a, 0x8cb4ff, 0xc9a4ff, 0x7fd9e6, 0xd5d1e3];
const BRIGHT: [Hue; 8] = [0x6c6880, 0xff9db0, 0xb0f0c2, 0xffe0b0, 0xb0ccff, 0xdcc4ff, 0xa8e9f2, 0xf6f4fc];

/// Color `index` of the 256-color palette: the theme's 16, the 6×6×6 cube,
/// the gray ramp. Above that are the named colors (foreground, background,
/// cursor and the dim variants).
pub fn indexed(index: usize) -> Hue {
    match index {
        0..=7 => NORMAL[index],
        8..=15 => BRIGHT[index - 8],
        16..=231 => {
            let i = index - 16;
            let level = |n: usize| if n == 0 { 0 } else { (55 + 40 * n) as u32 };
            level(i / 36) << 16 | level(i / 6 % 6) << 8 | level(i % 6)
        },
        232..=255 => {
            let v = (8 + 10 * (index - 232)) as u32;
            v << 16 | v << 8 | v
        },
        _ => named(index),
    }
}

fn named(index: usize) -> Hue {
    const FOREGROUND_NAME: usize = NamedColor::Foreground as usize;
    const BACKGROUND_NAME: usize = NamedColor::Background as usize;
    const CURSOR_NAME: usize = NamedColor::Cursor as usize;
    const DIM_BLACK: usize = NamedColor::DimBlack as usize;
    const DIM_WHITE: usize = NamedColor::DimWhite as usize;
    const BRIGHT_FOREGROUND: usize = NamedColor::BrightForeground as usize;
    const DIM_FOREGROUND: usize = NamedColor::DimForeground as usize;
    match index {
        FOREGROUND_NAME | BRIGHT_FOREGROUND => FOREGROUND,
        BACKGROUND_NAME => BACKGROUND,
        CURSOR_NAME => CURSOR,
        DIM_BLACK..=DIM_WHITE => dim(NORMAL[index - DIM_BLACK]),
        DIM_FOREGROUND => dim(FOREGROUND),
        _ => FOREGROUND,
    }
}

/// A color at two thirds of its light, for dim text.
pub fn dim(hue: Hue) -> Hue {
    let part = |shift: u32| ((hue >> shift & 0xff) as f32 * 0.66) as u32;
    part(16) << 16 | part(8) << 8 | part(0)
}

/// `over` laid on `under` at `share` of full strength.
pub fn blend(under: Hue, over: Hue, share: f32) -> Hue {
    let part = |shift: u32| {
        let (a, b) = ((under >> shift & 0xff) as f32, (over >> shift & 0xff) as f32);
        (a + (b - a) * share).round() as u32
    };
    part(16) << 16 | part(8) << 8 | part(0)
}

fn hue(rgb: Rgb) -> Hue {
    (rgb.r as u32) << 16 | (rgb.g as u32) << 8 | rgb.b as u32
}

/// A cell's color: what the program changed at run time (OSC 4, 10, 11)
/// wins over the theme.
pub fn resolve(color: Color, overrides: &Overrides) -> Hue {
    match color {
        Color::Spec(rgb) => hue(rgb),
        Color::Named(name) => overrides[name].map(hue).unwrap_or_else(|| indexed(name as usize)),
        Color::Indexed(i) => overrides[i as usize].map(hue).unwrap_or_else(|| indexed(i as usize)),
    }
}

/// What a program is told when it asks for color `index` (OSC 4, 10, 11).
pub fn answer(index: usize, overrides: &Overrides) -> Rgb {
    overrides[index].unwrap_or_else(|| {
        let hue = indexed(index);
        Rgb { r: (hue >> 16) as u8, g: (hue >> 8) as u8, b: hue as u8 }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_256_colors() {
        assert_eq!(indexed(1), 0xf27a93);
        assert_eq!(indexed(9), 0xff9db0);
        assert_eq!(indexed(16), 0x000000);
        assert_eq!(indexed(196), 0xff0000);
        assert_eq!(indexed(231), 0xffffff);
        assert_eq!(indexed(232), 0x080808);
        assert_eq!(indexed(255), 0xeeeeee);
        assert_eq!(indexed(NamedColor::Background as usize), BACKGROUND);
        assert_eq!(indexed(NamedColor::Cursor as usize), CURSOR);
    }

    #[test]
    fn blending() {
        assert_eq!(blend(0x000000, 0xffffff, 0.0), 0x000000);
        assert_eq!(blend(0x000000, 0xffffff, 1.0), 0xffffff);
        assert_eq!(blend(0x000000, 0xff8000, 0.5), 0x804000);
        assert_eq!(dim(0xffffff), 0xa8a8a8);
    }
}
