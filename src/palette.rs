//! From the colors a program asks for to the colors of the theme.

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors as Overrides;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::Hsla;

use crate::config::{Colors, Rgb};

/// Color `index` of the 256-color palette: the theme's 16, the 6×6×6 cube,
/// the gray ramp. Above that are the named colors (foreground, background,
/// cursor and the dim variants).
pub fn indexed(index: usize, colors: &Colors) -> Rgb {
    // the theme's own for one of the 240 past the sixteen, the last it names for it
    if (16..=255).contains(&index)
        && let Some(own) = colors.indexed_colors.iter().rfind(|own| usize::from(own.index) == index)
    {
        return own.color;
    }
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
    rgb(color, overrides, colors).hsla()
}

/// The same, as red, green and blue.
pub fn rgb(color: Color, overrides: &Overrides, colors: &Colors) -> Rgb {
    match color {
        Color::Spec(rgb) => Rgb(rgb.r, rgb.g, rgb.b),
        Color::Named(name) => match overrides[name] {
            Some(rgb) => Rgb(rgb.r, rgb.g, rgb.b),
            None => indexed(name as usize, colors),
        },
        Color::Indexed(i) => match overrides[i as usize] {
            Some(rgb) => Rgb(rgb.r, rgb.g, rgb.b),
            None => indexed(i as usize, colors),
        },
    }
}

/// The colors a cell's text and its background are drawn in, of the two it
/// was written with: bold text in the bright color where the theme asks for
/// that, and the two the other way round for inverse text.
pub fn drawn(fg: Color, bg: Color, flags: Flags, colors: &Colors) -> (Color, Color) {
    let fg = if flags.contains(Flags::BOLD) && colors.draw_bold_text_with_bright_colors { brighten(fg) } else { fg };
    if flags.contains(Flags::INVERSE) { (bg, fg) } else { (fg, bg) }
}

/// Whether a cell's background is the pane's own. Nothing is painted for
/// such a cell: what shows there is what the pane stands on ([`ground`]).
pub fn bare(bg: Color) -> bool {
    bg == Color::Named(NamedColor::Background)
}

/// The background a program has set for the whole pane (OSC 11), where it
/// is not the theme's. The pane is filled with it, under everything else.
pub fn ground(overrides: &Overrides, colors: &Colors) -> Option<Rgb> {
    let set = overrides[NamedColor::Background].map(|rgb| Rgb(rgb.r, rgb.g, rgb.b));
    set.filter(|&set| set != colors.primary.background)
}

/// The color of text nothing else is said about: the theme's, or the one a
/// program has set (OSC 10).
pub fn ink(overrides: &Overrides, colors: &Colors) -> Rgb {
    rgb(Color::Named(NamedColor::Foreground), overrides, colors)
}

/// The cursor's color, and the color of what stands under a block cursor.
///
/// A cursor color a program has set (OSC 12) is the cursor's, with the
/// pane's background on it. Otherwise the cursor is the theme's. On a
/// background a program has set, a theme's cursor may be as light or as dark
/// as that background: then the cursor is in the color of the text, as it
/// is in a theme that names no cursor color.
pub fn cursor(overrides: &Overrides, colors: &Colors) -> (Rgb, Rgb) {
    /// Less contrast than this, and a cursor is hard to find on its background.
    const LEAST: f32 = 3.;
    let theme = colors.cursor.cursor.unwrap_or(colors.primary.foreground);
    let ground = ground(overrides, colors);
    let behind = ground.unwrap_or(colors.primary.background);
    match overrides[NamedColor::Cursor] {
        Some(set) => (Rgb(set.r, set.g, set.b), behind),
        None if ground.is_some_and(|ground| contrast(theme, ground) < LEAST) => (ink(overrides, colors), behind),
        None => (theme, colors.cursor.text.unwrap_or(behind)),
    }
}

/// How far two colors are apart in lightness, as the contrast ratio the
/// web's guidelines count with: 1 for the same lightness, 21 for black on
/// white.
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let light = |color: Rgb| {
        let part = |value: u8| {
            let value = f32::from(value) / 255.;
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * part(color.0) + 0.7152 * part(color.1) + 0.0722 * part(color.2)
    };
    let (a, b) = (light(a), light(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// One color over another, as much of it as `alpha` says.
pub fn over(top: Rgb, alpha: f32, under: Rgb) -> Rgb {
    let mix = |top: u8, under: u8| (f32::from(under) + (f32::from(top) - f32::from(under)) * alpha).round() as u8;
    Rgb(mix(top.0, under.0), mix(top.1, under.1), mix(top.2, under.2))
}

/// The color selected text is drawn in, where the theme names none: its
/// own. But text in the color of the pane's background (`behind`) stood on
/// a block of its own, as inverse text does, and the selection has taken the
/// place of that block. Such text is drawn in the color of the text (`ink`)
/// where that is seen better on what the selection puts behind it (`on`).
pub fn selected(text: Rgb, behind: Rgb, ink: Rgb, on: Rgb) -> Rgb {
    if !apart(text, behind, false) && contrast(ink, on) > contrast(text, on) { ink } else { text }
}

/// Whether text in one color can be told from the color behind it. Two
/// colors that differ less than this are the same to anyone who is not
/// looking for a difference. `dim` text is drawn partly see-through.
pub fn apart(text: Rgb, behind: Rgb, dim: bool) -> bool {
    const LEAST: f32 = 24.;
    let most = text.0.abs_diff(behind.0).max(text.1.abs_diff(behind.1)).max(text.2.abs_diff(behind.2));
    f32::from(most) * if dim { 0.6 } else { 1. } >= LEAST
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

    #[test]
    fn what_a_program_sets_for_the_whole_pane() {
        let colors = Colors::default();
        let mut overrides = Overrides::default();
        assert_eq!(ground(&overrides, &colors), None);
        assert_eq!(ink(&overrides, &colors), colors.primary.foreground);
        // a light theme, set from inside the terminal: OSC 11 and OSC 10
        overrides[NamedColor::Background] = Some(alacritty_terminal::vte::ansi::Rgb { r: 250, g: 244, b: 237 });
        overrides[NamedColor::Foreground] = Some(alacritty_terminal::vte::ansi::Rgb { r: 87, g: 82, b: 121 });
        assert_eq!(ground(&overrides, &colors), Some(Rgb(250, 244, 237)));
        assert_eq!(ink(&overrides, &colors), Rgb(87, 82, 121));
        // the theme's own background, set again, is nothing to paint
        let theme = colors.primary.background;
        overrides[NamedColor::Background] =
            Some(alacritty_terminal::vte::ansi::Rgb { r: theme.0, g: theme.1, b: theme.2 });
        assert_eq!(ground(&overrides, &colors), None);
    }

    #[test]
    fn a_cursor_that_is_seen_on_what_a_program_sets() {
        let set = |r, g, b| Some(alacritty_terminal::vte::ansi::Rgb { r, g, b });
        let colors = Colors::default();
        let (lamp, on_lamp) = (colors.cursor.cursor.unwrap(), colors.cursor.text.unwrap());
        assert!((contrast(Rgb(0, 0, 0), Rgb(255, 255, 255)) - 21.).abs() < 0.01);
        assert!((contrast(lamp, lamp) - 1.).abs() < 0.01);
        // nothing set: the theme's cursor, and its color for what is under it
        let mut overrides = Overrides::default();
        assert_eq!(cursor(&overrides, &colors), (lamp, on_lamp));
        // another dark background: the lamp is still seen on it
        overrides[NamedColor::Background] = set(30, 30, 46);
        assert_eq!(cursor(&overrides, &colors), (lamp, on_lamp));
        // A light background, with nothing said of the cursor: the lamp is lost on
        // it, so the cursor is in the color of the text.
        overrides[NamedColor::Background] = set(250, 244, 237);
        overrides[NamedColor::Foreground] = set(87, 82, 121);
        assert_eq!(cursor(&overrides, &colors), (Rgb(87, 82, 121), Rgb(250, 244, 237)));
        // a cursor the program has set: that one, and the pane's background on it
        overrides[NamedColor::Cursor] = set(215, 130, 126);
        assert_eq!(cursor(&overrides, &colors), (Rgb(215, 130, 126), Rgb(250, 244, 237)));
        // set on the theme's own background
        overrides[NamedColor::Background] = None;
        assert_eq!(cursor(&overrides, &colors), (Rgb(215, 130, 126), colors.primary.background));
    }

    #[test]
    fn selected_text_that_stood_on_a_block_of_its_own() {
        // the theme's own colors, and a selection that is a wash of the text's color
        let (ground, ink) = (Rgb(0x12, 0x11, 0x1a), Rgb(0xe4, 0xe1, 0xee));
        let wash = over(ink, 0.28, ground);
        assert_eq!(over(Rgb(255, 255, 255), 0.5, Rgb(0, 0, 0)), Rgb(128, 128, 128));
        // inverse text was in the background's color on a block: on the wash it is lost
        assert_eq!(selected(ground, ground, ink, wash), ink);
        // so is text in a color written out that is close to the background's
        assert_eq!(selected(Rgb(0x1a, 0x1a, 0x22), ground, ink, wash), ink);
        // text in any other color keeps it
        let red = Rgb(0xf2, 0x7a, 0x93);
        assert_eq!(selected(red, ground, ink, wash), red);
        // A theme with a light color for the selection: dark text on a block is
        // seen on it as it is, and would be lost in the color of the text.
        let (ground, ink, light) = (Rgb(0x1e, 0x1e, 0x2e), Rgb(0xcd, 0xd6, 0xf4), Rgb(0xf5, 0xe0, 0xdc));
        assert_eq!(selected(Rgb(0x26, 0x26, 0x26), ground, ink, light), Rgb(0x26, 0x26, 0x26));
        assert_eq!(selected(ground, ground, ink, light), ground);
        // and one with a dark color for it
        assert_eq!(selected(ground, ground, ink, Rgb(0x3a, 0x35, 0x50)), ink);
    }

    #[test]
    fn text_that_is_not_seen_on_its_background() {
        let colors = Colors::default();
        let overrides = Overrides::default();
        let seen = |fg: Color, bg: Color, flags: Flags| {
            let (fg, bg) = drawn(fg, bg, flags, &colors);
            apart(rgb(fg, &overrides, &colors), rgb(bg, &overrides, &colors), flags.contains(Flags::DIM))
        };
        let (text, behind) = (Color::Named(NamedColor::Foreground), Color::Named(NamedColor::Background));
        assert!(seen(text, behind, Flags::empty()));
        assert!(seen(text, behind, Flags::INVERSE | Flags::DIM));
        // the same color, written in three ways
        let black = colors.normal.black;
        let spec = Color::Spec(alacritty_terminal::vte::ansi::Rgb { r: black.0, g: black.1, b: black.2 });
        assert!(!seen(Color::Indexed(0), Color::Named(NamedColor::Black), Flags::empty()));
        assert!(!seen(spec, Color::Named(NamedColor::Black), Flags::empty()));
        assert!(!seen(Color::Named(NamedColor::Black), spec, Flags::INVERSE));
        // the theme's background, asked for and written as the text's color
        let ground = colors.primary.background;
        let like = |by: u8| {
            Color::Spec(alacritty_terminal::vte::ansi::Rgb { r: ground.0 + by, g: ground.1 + by, b: ground.2 + by })
        };
        assert!(!seen(like(0), behind, Flags::empty()));
        assert!(!seen(like(9), behind, Flags::empty()));
        assert!(seen(like(60), behind, Flags::empty()));
        // A background a program has set since is what the pane is filled with: text
        // in that color is not seen on it, and text in the theme's is.
        let white = alacritty_terminal::vte::ansi::Rgb { r: 255, g: 255, b: 255 };
        let mut changed = Overrides::default();
        changed[NamedColor::Background] = Some(white);
        changed[1] = Some(white);
        let on = |fg: Color, bg: Color, flags: Flags| {
            let (fg, bg) = drawn(fg, bg, flags, &colors);
            apart(rgb(fg, &changed, &colors), rgb(bg, &changed, &colors), false)
        };
        let white = Color::Spec(white);
        assert!(on(like(0), behind, Flags::empty()));
        assert!(!on(white, behind, Flags::empty()));
        // inverse text is drawn in the color that was set, on a background of its own
        assert!(seen(white, behind, Flags::INVERSE));
        assert!(!on(white, behind, Flags::INVERSE));
        assert!(on(like(60), behind, Flags::INVERSE));
        // and a color of the palette a program has changed (the red, above) is drawn as it was changed
        assert!(!on(Color::Indexed(1), behind, Flags::empty()));
        assert!(!on(Color::Named(NamedColor::Red), behind, Flags::empty()));
    }
}
