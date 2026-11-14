//! `~/.config/lumen/lumen.toml`.
//!
//! The `[colors]` table has the shape of Alacritty's, so a theme written
//! for Alacritty can be pasted in as it is.

use std::path::PathBuf;

use gpui::{Global, Hsla, Rgba, WindowBackgroundAppearance};
use serde::Deserialize;
// What an Alacritty theme says that lumen has no use for: taken, so that the
// theme can be pasted in as it is, and left aside.
use serde::de::IgnoredAny as Unused;

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

/// A color, or one of Alacritty's two words for "the cell's own color",
/// which is what lumen does where it is given no color.
fn color_or_cell<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Rgb>, D::Error> {
    let s = String::deserialize(d)?;
    if s.eq_ignore_ascii_case("CellForeground") || s.eq_ignore_ascii_case("CellBackground") {
        return Ok(None);
    }
    Rgb::parse(&s).map(Some).ok_or_else(|| serde::de::Error::custom(format!("not a color: {s:?} (want \"#rrggbb\")")))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorShape {
    #[default]
    Block,
    Beam,
    Underline,
}

/// What `lumen` does when a lumen is already running.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Launch {
    /// A new tab in the window used last.
    #[default]
    Tab,
    /// A new window of the running lumen.
    Window,
    /// A lumen of its own, which starts from nothing.
    Process,
}

/// Where a new tab starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TabFolder {
    /// In the folder of the tab you are in.
    #[default]
    Current,
    /// In your home folder.
    Home,
    /// In this folder, as written: `/`, `~/Projekte`.
    Folder(String),
}

impl TabFolder {
    /// The folder a new tab starts in, if not the current tab's. One that isn't there (a
    /// folder on a drive that isn't plugged in) is none: the tab starts where it would
    /// have without the setting.
    pub fn folder(&self) -> Option<PathBuf> {
        let dir = match self {
            TabFolder::Current => return None,
            TabFolder::Home => dirs::home_dir()?,
            TabFolder::Folder(written) => expand_home(written)?,
        };
        dir.is_dir().then_some(dir)
    }

    /// As the file says it.
    pub fn written(&self) -> &str {
        match self {
            TabFolder::Current => "current",
            TabFolder::Home => "home",
            TabFolder::Folder(written) => written,
        }
    }

    pub fn parse(text: &str) -> Result<TabFolder, String> {
        match text.trim() {
            "current" => Ok(TabFolder::Current),
            "home" => Ok(TabFolder::Home),
            folder if folder.starts_with('/') || folder == "~" || folder.starts_with("~/") => {
                Ok(TabFolder::Folder(folder.to_string()))
            },
            other => Err(format!(
                "new_tab_folder can't be {other:?} (want \"current\", \"home\", or a folder like \"/\" or \"~/Projekte\")"
            )),
        }
    }
}

impl<'de> Deserialize<'de> for TabFolder {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        TabFolder::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// `~` and `~/…` as the home folder and what is in it.
pub fn expand_home(written: &str) -> Option<PathBuf> {
    match written.strip_prefix('~') {
        Some("") => dirs::home_dir(),
        Some(rest) => rest.strip_prefix('/').and_then(|rest| Some(dirs::home_dir()?.join(rest))),
        None => Some(PathBuf::from(written)),
    }
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
    #[serde(rename = "dim_foreground")]
    _dim_foreground: Unused,
    #[serde(rename = "bright_foreground")]
    _bright_foreground: Unused,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CursorColors {
    #[serde(deserialize_with = "color_or_cell")]
    pub cursor: Option<Rgb>,
    #[serde(deserialize_with = "color_or_cell")]
    pub text: Option<Rgb>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SelectionColors {
    #[serde(deserialize_with = "color_or_cell")]
    pub background: Option<Rgb>,
    #[serde(deserialize_with = "color_or_cell")]
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
    // the rest of what Alacritty's [colors] can hold
    #[serde(rename = "dim")]
    _dim: Unused,
    #[serde(rename = "vi_mode_cursor")]
    _vi_mode_cursor: Unused,
    #[serde(rename = "search")]
    _search: Unused,
    #[serde(rename = "hints")]
    _hints: Unused,
    #[serde(rename = "line_indicator")]
    _line_indicator: Unused,
    #[serde(rename = "footer_bar")]
    _footer_bar: Unused,
    #[serde(rename = "indexed_colors")]
    _indexed_colors: Unused,
    #[serde(rename = "transparent_background_colors")]
    _transparent_background_colors: Unused,
}

// lumen's own theme: a night sky with a warm lamp in it.
impl Default for Primary {
    fn default() -> Self {
        Primary {
            background: Rgb(0x12, 0x11, 0x1a),
            foreground: Rgb(0xe4, 0xe1, 0xee),
            _dim_foreground: Unused,
            _bright_foreground: Unused,
        }
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
            _dim: Unused,
            _vi_mode_cursor: Unused,
            _search: Unused,
            _hints: Unused,
            _line_indicator: Unused,
            _footer_bar: Unused,
            _indexed_colors: Unused,
            _transparent_background_colors: Unused,
        }
    }
}

/// The program a line in a file is opened with.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Editor {
    /// The program, or a whole line with `{file}`, `{line}` and `{column}`
    /// in it. What `$VISUAL` or `$EDITOR` name if unset.
    pub command: Option<String>,
    /// Whether it runs in a terminal, and so gets a tab. Unset, lumen goes
    /// by the editor's name.
    pub terminal: Option<bool>,
}

/// The machine's load, as pulse tells it where pulse is installed.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pulse {
    /// Run pulse and show what it says: the light in the title bar, what a tab uses.
    pub enabled: bool,
    /// Seconds between two readings.
    pub every: u32,
}

impl Default for Pulse {
    fn default() -> Self {
        Pulse { enabled: true, every: 3 }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub font: Font,
    pub window: Window,
    pub cursor: Cursor,
    pub colors: Colors,
    pub editor: Editor,
    pub pulse: Pulse,
    /// A program may post a notification on the desktop, for a tab that isn't looked at.
    pub notifications: bool,
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
    /// What starting lumen again does while one runs: see [`Launch`].
    pub launch: Launch,
    /// Where a new tab starts: see [`TabFolder`].
    pub new_tab_folder: TabFolder,
    /// What was wrong with the file when it was last read, while lumen is
    /// running: the settings from before stay, and the window says why.
    #[serde(skip)]
    pub problem: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            font: Font::default(),
            window: Window::default(),
            cursor: Cursor::default(),
            colors: Colors::default(),
            editor: Editor::default(),
            pulse: Pulse::default(),
            notifications: true,
            scrollback: 10_000,
            copy_on_select: false,
            shell: None,
            prewarm: false,
            launch: Launch::default(),
            new_tab_folder: TabFolder::default(),
            problem: None,
        }
    }
}

impl Global for Config {}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("lumen").join("lumen.toml")
    }

    /// The config file, or the defaults where there is none. A file that
    /// doesn't parse is reported (what is wrong with it is the second
    /// value) and the defaults are used: a terminal that won't start is
    /// how one gets locked out of fixing it.
    pub fn load() -> (Config, Option<String>) {
        match std::fs::read_to_string(Self::path()) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => (config.checked(), None),
                Err(e) => (Config::default().checked(), Some(e.message().to_string())),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default().checked(), None),
            Err(e) => (Config::default().checked(), Some(e.to_string())),
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
        self.pulse.every = self.pulse.every.clamp(1, 60);
        self
    }

    /// When the file was last written, which is how a change to it is noticed.
    pub fn modified() -> Option<std::time::SystemTime> {
        std::fs::metadata(Self::path()).and_then(|meta| meta.modified()).ok()
    }

    /// The file again, for a lumen that is running with `self`: a file that
    /// doesn't parse changes nothing, and is reported.
    pub fn reloaded(&self) -> Config {
        match Self::load() {
            (config, None) => config,
            (_, Some(problem)) => Config { problem: Some(format!("lumen.toml: {problem}")), ..self.clone() },
        }
    }

    /// What the window lets through of what is behind it.
    pub fn appearance(&self) -> WindowBackgroundAppearance {
        if self.window.opacity < 1. && self.window.blur {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Transparent
        }
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
    fn a_theme_with_everything_alacritty_has() {
        // the shape of the themes people share: tables lumen has no use for, and
        // "the cell's own color" where a color could be
        let config: Config = toml::from_str(
            r##"
            [colors.primary]
            background = "#1e1e2e"
            foreground = "#cdd6f4"
            dim_foreground = "#7f849c"
            bright_foreground = "#cdd6f4"

            [colors.cursor]
            text = "CellBackground"
            cursor = "#f5e0dc"

            [colors.vi_mode_cursor]
            text = "#1e1e2e"
            cursor = "#b4befe"

            [colors.search.matches]
            foreground = "#1e1e2e"
            background = "#a6adc8"

            [colors.search.focused_match]
            foreground = "#1e1e2e"
            background = "#a6e3a1"

            [colors.footer_bar]
            foreground = "#1e1e2e"
            background = "#a6adc8"

            [colors.hints.start]
            foreground = "#1e1e2e"
            background = "#f9e2af"

            [colors.hints.end]
            foreground = "#1e1e2e"
            background = "#a6adc8"

            [colors.line_indicator]
            foreground = "None"
            background = "None"

            [colors.selection]
            text = "CellForeground"
            background = "#f5e0dc"

            [colors.normal]
            black = "#45475a"
            red = "#f38ba8"

            [colors.bright]
            black = "#585b70"

            [colors.dim]
            black = "#45475a"

            [[colors.indexed_colors]]
            index = 16
            color = "#fab387"

            [[colors.indexed_colors]]
            index = 17
            color = "#f5e0dc"
            "##,
        )
        .unwrap();
        assert_eq!(config.colors.primary.background, Rgb(0x1e, 0x1e, 0x2e));
        assert_eq!(config.colors.cursor.cursor, Some(Rgb(0xf5, 0xe0, 0xdc)));
        // the cell's own color is what lumen uses where it has none
        assert_eq!(config.colors.cursor.text, None);
        assert_eq!(config.colors.selection.text, None);
        assert_eq!(config.colors.selection.background, Some(Rgb(0xf5, 0xe0, 0xdc)));
        assert_eq!(config.colors.bright.black, Rgb(0x58, 0x5b, 0x70));

        let flags: Config = toml::from_str(
            "[colors]\ntransparent_background_colors = true\ndraw_bold_text_with_bright_colors = true\n",
        )
        .unwrap();
        assert!(flags.colors.draw_bold_text_with_bright_colors);
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_default() {
        assert!(toml::from_str::<Config>("[font]\nfamliy = \"x\"\n").is_err());
        assert!(toml::from_str::<Config>("[colors.primary]\nbackground = \"red\"\n").is_err());
        assert!(toml::from_str::<Config>("[colors.primary]\nbackgruond = \"#000000\"\n").is_err());
        assert!(toml::from_str::<Config>("[colors.cursor]\ncursor = \"red\"\n").is_err());
        assert!(toml::from_str::<Config>("[colors.serach]\nx = 1\n").is_err());
    }
}
