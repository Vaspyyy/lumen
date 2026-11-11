//! `~/.config/lumen/lumen.toml`.
//!
//! The `[colors]` table has the shape of Alacritty's, so a theme written
//! for Alacritty can be pasted in as it is.

use std::path::PathBuf;

use gpui::{Global, Hsla, Rgba};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hsla(self) -> Hsla {
        Rgba { r: self.0 as f32 / 255., g: self.1 as f32 / 255., b: self.2 as f32 / 255., a: 1. }.into()
    }

    /// `#rrggbb`, `0xrrggbb` or `rrggbb`.
    pub fn parse(s: &str) -> Option<Rgb> {
        let hex = s.trim().trim_start_matches('#').trim_start_matches("0x");
        if hex.len() != 6 {
            return None;
        }
        let n = u32::from_str_radix(hex, 16).ok()?;
        Some(Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("not a color: {s:?} (want \"#rrggbb\")")))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorShape {
    #[default]
    Block,
    Beam,
    Underline,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Font {
    pub family: String,
    /// In points, as other terminals count it.
    pub size: f32,
    /// Row height as a multiple of the font's own line height.
    pub line_height: f32,
}

impl Default for Font {
    fn default() -> Self {
        Font { family: "monospace".into(), size: 12., line_height: 1.0 }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Window {
    /// Of the terminal's background: 1 is solid.
    pub opacity: f32,
    /// Blur what is behind a see-through window, where the desktop can.
    pub blur: bool,
    pub padding: f32,
    pub columns: usize,
    pub lines: usize,
}

impl Default for Window {
    fn default() -> Self {
        Window { opacity: 1., blur: true, padding: 10., columns: 100, lines: 30 }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cursor {
    pub style: CursorShape,
    pub blink: bool,
    /// The light the cursor gives off.
    pub glow: bool,
    /// The cursor slides to where it goes instead of jumping.
    pub smooth: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor { style: CursorShape::Block, blink: true, glow: true, smooth: true }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Primary {
    pub background: Rgb,
    pub foreground: Rgb,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CursorColors {
    pub cursor: Option<Rgb>,
    pub text: Option<Rgb>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SelectionColors {
    pub background: Option<Rgb>,
    pub text: Option<Rgb>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ansi {
    pub black: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub white: Rgb,
}

impl Ansi {
    pub fn list(&self) -> [Rgb; 8] {
        [self.black, self.red, self.green, self.yellow, self.blue, self.magenta, self.cyan, self.white]
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub primary: Primary,
    pub cursor: CursorColors,
    pub selection: SelectionColors,
    pub normal: Ansi,
    pub bright: Ansi,
    /// The color of lumen's own light: the cursor's glow, the active tab.
    pub accent: Rgb,
    pub draw_bold_text_with_bright_colors: bool,
}

// lumen's own theme: a night sky with a warm lamp in it.
impl Default for Primary {
    fn default() -> Self {
        Primary { background: Rgb(0x12, 0x11, 0x1a), foreground: Rgb(0xe4, 0xe1, 0xee) }
    }
}

impl Default for Ansi {
    fn default() -> Self {
        Ansi {
            black: Rgb(0x23, 0x21, 0x30),
            red: Rgb(0xf2, 0x7a, 0x93),
            green: Rgb(0x8f, 0xdf, 0xa8),
            yellow: Rgb(0xff, 0xd0, 0x8a),
            blue: Rgb(0x8c, 0xb4, 0xff),
            magenta: Rgb(0xc9, 0xa4, 0xff),
            cyan: Rgb(0x7f, 0xd9, 0xe6),
            white: Rgb(0xd5, 0xd1, 0xe3),
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            primary: Primary::default(),
            // the cursor is the lamp
            cursor: CursorColors { cursor: Some(Rgb(0xff, 0xd9, 0xa0)), text: Some(Primary::default().background) },
            selection: SelectionColors::default(),
            normal: Ansi::default(),
            bright: Ansi {
                black: Rgb(0x6c, 0x68, 0x80),
                red: Rgb(0xff, 0x9d, 0xb0),
                green: Rgb(0xb0, 0xf0, 0xc2),
                yellow: Rgb(0xff, 0xe0, 0xb0),
                blue: Rgb(0xb0, 0xcc, 0xff),
                magenta: Rgb(0xdc, 0xc4, 0xff),
                cyan: Rgb(0xa8, 0xe9, 0xf2),
                white: Rgb(0xf6, 0xf4, 0xfc),
            },
            accent: Rgb(0xff, 0xc9, 0x7a),
            draw_bold_text_with_bright_colors: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub font: Font,
    pub window: Window,
    pub cursor: Cursor,
    pub colors: Colors,
    /// Lines kept above the screen.
    pub scrollback: usize,
    /// Selecting text copies it, as well as putting it in the middle-click selection.
    pub copy_on_select: bool,
    /// The program a new terminal runs: your login shell if unset.
    pub shell: Option<String>,
    /// Keep one shell started in the background, so that a new tab is
    /// there at once instead of after the shell's own startup. The shell's
    /// startup files run for it before the tab is asked for.
    pub prewarm: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            font: Font::default(),
            window: Window::default(),
            cursor: Cursor::default(),
            colors: Colors::default(),
            scrollback: 10_000,
            copy_on_select: false,
            shell: None,
            prewarm: false,
        }
    }
}

impl Global for Config {}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("lumen").join("lumen.toml")
    }

    /// The config file, or the defaults where there is none. A file that
    /// doesn't parse is reported and the defaults are used: a terminal
    /// that won't start is how one gets locked out of fixing it.
    pub fn load() -> (Config, Option<String>) {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => (config.checked(), None),
                Err(e) => (Config::default().checked(), Some(format!("{}: {}", path.display(), e.message()))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default().checked(), None),
            Err(e) => (Config::default().checked(), Some(format!("{}: {e}", path.display()))),
        }
    }

    fn checked(mut self) -> Config {
        self.font.family = installed_family(&self.font.family);
        self.font.size = self.font.size.clamp(4., 96.);
        self.font.line_height = self.font.line_height.clamp(0.8, 3.);
        self.window.opacity = self.window.opacity.clamp(0., 1.);
        self.window.padding = self.window.padding.clamp(0., 200.);
        self.window.columns = self.window.columns.clamp(20, 1000);
        self.window.lines = self.window.lines.clamp(5, 500);
        self
    }

    pub fn cursor_color(&self) -> Hsla {
        self.colors.cursor.cursor.unwrap_or(self.colors.primary.foreground).hsla()
    }

    pub fn accent(&self) -> Hsla {
        self.colors.accent.hsla()
    }
}

/// The name the system knows a font family by. "monospace" is whatever the
/// desktop's fixed-width font is, and a name that is close to an installed
/// one (MapleMono for Maple Mono) is taken to mean it. A family that isn't
/// installed would be drawn in some proportional font, which no grid
/// survives, so that falls back to the desktop's fixed-width font too.
fn installed_family(wanted: &str) -> String {
    let ask = |pattern: &str| -> Option<String> {
        let out = std::process::Command::new("fc-match").args(["-f", "%{family}", pattern]).output().ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        // a font can have several names, the first is the one to ask for
        let first = text.split(',').next()?.trim();
        (!first.is_empty()).then(|| first.to_string())
    };
    let squash = |name: &str| name.to_lowercase().replace([' ', '-', '_'], "");
    let generic = wanted.is_empty() || wanted.eq_ignore_ascii_case("monospace");
    if !generic {
        match ask(wanted) {
            Some(found) if squash(&found) == squash(wanted) => return found,
            Some(_) => eprintln!("lumen: the font {wanted:?} isn't installed, using the system's fixed-width font"),
            // no fontconfig to ask: take the name as it is
            None => return wanted.to_string(),
        }
    }
    ask("monospace").unwrap_or_else(|| "DejaVu Sans Mono".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alacritty_theme_is_a_lumen_theme() {
        let config: Config = toml::from_str(
            r##"
            [colors.primary]
            background = "#241A24"
            foreground = "0xD8DEE9"

            [colors.normal]
            black = "0x3B4252"
            red = "0xBF616A"

            [colors.cursor]
            text = '#141218'
            cursor = '#d0bcff'
            "##,
        )
        .unwrap();
        assert_eq!(config.colors.primary.background, Rgb(0x24, 0x1a, 0x24));
        assert_eq!(config.colors.normal.red, Rgb(0xbf, 0x61, 0x6a));
        assert_eq!(config.colors.cursor.cursor, Some(Rgb(0xd0, 0xbc, 0xff)));
        // what the file doesn't say keeps lumen's default
        assert_eq!(config.colors.normal.green, Ansi::default().green);
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_default() {
        assert!(toml::from_str::<Config>("[font]\nfamliy = \"x\"\n").is_err());
        assert!(toml::from_str::<Config>("[colors.primary]\nbackground = \"red\"\n").is_err());
    }
}
