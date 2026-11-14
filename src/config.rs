//! `~/.config/lumen/lumen.toml`.
//!
//! The `[colors]` table has the shape of Alacritty's, so a theme written
//! for Alacritty can be pasted in as it is.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui::{Global, Hsla, Rgba, WindowBackgroundAppearance, WindowDecorations};
use serde::Deserialize;
use serde::de::{MapAccess, Visitor};
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
    /// Letters that a font joins into one sign (`->` into an arrow) are
    /// joined.
    pub ligatures: bool,
    /// How heavy plain text is, and bold text: 100 (thin) to 900 (black).
    pub weight: u16,
    pub bold_weight: u16,
    /// Pixels between two characters, beyond what the font has: a cell is wider by it.
    pub letter_spacing: f32,
    /// The family of lumen's own words (the tabs, the notes, the search): the
    /// desktop's if unset.
    pub ui_family: Option<String>,
    /// Their size, in pixels.
    pub ui_size: f32,
}

impl Default for Font {
    fn default() -> Self {
        Font {
            family: "monospace".into(),
            size: 12.,
            line_height: 1.0,
            ligatures: true,
            weight: 400,
            bold_weight: 700,
            letter_spacing: 0.,
            ui_family: None,
            ui_size: 12.5,
        }
    }
}

/// Space between a pane's edge and its text, in pixels: the same on every
/// side (`padding = 10`), or across and down apart (`padding = { x = 10,
/// y = 4 }`), as Alacritty writes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Padding {
    /// Left and right.
    pub x: f32,
    /// Top and bottom.
    pub y: f32,
}

impl Default for Padding {
    fn default() -> Self {
        Padding { x: 10., y: 10. }
    }
}

impl<'de> Deserialize<'de> for Padding {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Either;
        impl<'de> Visitor<'de> for Either {
            type Value = Padding;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("pixels, or a table with x and y")
            }
            fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Padding, E> {
                Ok(Padding { x: n as f32, y: n as f32 })
            }
            fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Padding, E> {
                Ok(Padding { x: n as f32, y: n as f32 })
            }
            fn visit_f64<E: serde::de::Error>(self, n: f64) -> Result<Padding, E> {
                Ok(Padding { x: n as f32, y: n as f32 })
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Padding, M::Error> {
                // a side the table leaves out keeps lumen's default
                let mut padding = Padding::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "x" => padding.x = map.next_value()?,
                        "y" => padding.y = map.next_value()?,
                        _ => return Err(serde::de::Error::unknown_field(&key, &["x", "y"])),
                    }
                }
                Ok(padding)
            }
        }
        d.deserialize_any(Either)
    }
}

/// Who draws the window's frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decorations {
    /// lumen: round corners, a shadow, and the window's buttons in the tab bar.
    #[default]
    Lumen,
    /// The desktop, where it draws frames (KDE, most tiling desktops). Where
    /// it doesn't (GNOME), lumen draws its own.
    Desktop,
    /// Nobody: square corners, no shadow, no border and no buttons. The
    /// window fills all the space the desktop gives it.
    None,
}

/// How a new window opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartupMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Window {
    /// Of the terminal's background: 1 is solid.
    pub opacity: f32,
    /// Blur what is behind a see-through window, where the desktop can.
    pub blur: bool,
    pub padding: Padding,
    /// The space a pane has beyond its last whole cell is spread around the
    /// grid, instead of being left at its right and bottom.
    pub dynamic_padding: bool,
    pub decorations: Decorations,
    pub startup_mode: StartupMode,
    pub columns: usize,
    pub lines: usize,
    /// How round the corners of lumen's frame are, in pixels.
    pub corner_radius: f32,
    /// lumen's frame casts a shadow around the window.
    pub shadow: bool,
    /// A hairline around lumen's frame.
    pub border: bool,
    /// A picture behind the text: a file, `~` for the home folder.
    pub background_image: Option<String>,
    /// How much of the picture shows over the background's own color: 1 is all of it.
    pub background_image_opacity: f32,
    pub background_image_fit: ImageFit,
    /// The background fades from its own color into this one.
    pub gradient: Option<Rgb>,
    /// Which way it fades, in degrees: 180 is from the top down, 90 from the left.
    pub gradient_angle: f32,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            opacity: 1.,
            blur: true,
            padding: Padding::default(),
            dynamic_padding: false,
            decorations: Decorations::default(),
            startup_mode: StartupMode::default(),
            columns: 100,
            lines: 30,
            corner_radius: 12.,
            shadow: true,
            border: true,
            background_image: None,
            background_image_opacity: 0.3,
            background_image_fit: ImageFit::default(),
            gradient: None,
            gradient_angle: 180.,
        }
    }
}

impl Window {
    /// The background picture's file, if one is set: `~` is the home folder.
    pub fn background_image(&self) -> Option<PathBuf> {
        expand_home(self.background_image.as_deref()?.trim())
    }
}

/// How the background picture fills the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFit {
    /// All of the window, cutting off what sticks out.
    #[default]
    Cover,
    /// All of the picture, with room left beside it.
    Contain,
    /// Stretched to the window's shape.
    Fill,
}

/// Where the tab bar is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabPosition {
    #[default]
    Top,
    Bottom,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Panes {
    /// How far the panes without the keyboard step back: 0 not at all, 1 out of sight.
    pub dim_inactive: f32,
    /// The line between two panes, in pixels: 0 is none.
    pub divider_width: f32,
}

impl Default for Panes {
    fn default() -> Self {
        Panes { dim_inactive: 0.22, divider_width: 1. }
    }
}

/// When the tab bar is there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabBar {
    #[default]
    Always,
    /// While a window has more than one tab.
    Auto,
    Never,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tabs {
    pub bar: TabBar,
    pub position: TabPosition,
}

/// The cursor of a pane that doesn't have the keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unfocused {
    /// An outline of a block.
    #[default]
    Hollow,
    /// Its own shape, without blinking or glowing.
    Same,
    Hidden,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cursor {
    pub style: CursorShape,
    pub blink: bool,
    /// Milliseconds the cursor is lit, and then dimmed.
    pub blink_interval: u64,
    /// The light the cursor gives off.
    pub glow: bool,
    /// The cursor slides to where it goes instead of jumping.
    pub smooth: bool,
    /// Its corners are round.
    pub rounded: bool,
    /// How thick a beam or an underline is, in pixels: a ninth of a row if unset.
    pub thickness: Option<f32>,
    pub unfocused: Unfocused,
    /// How bright the glow is, and how far it reaches: 1 is lumen's own.
    pub glow_strength: f32,
    pub glow_size: f32,
    /// Milliseconds the cursor takes to glide to its next cell.
    pub glide_time: u64,
    /// How bright a blinking cursor is at its dimmest: 0 is out.
    pub blink_dim: f32,
    /// Milliseconds it takes to dim and to come back: 0 is at once.
    pub blink_fade: u64,
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor {
            style: CursorShape::Block,
            blink: true,
            blink_interval: 530,
            glow: true,
            smooth: true,
            rounded: true,
            thickness: None,
            unfocused: Unfocused::default(),
            glow_strength: 1.,
            glow_size: 1.,
            glide_time: 70,
            blink_dim: 0.22,
            blink_fade: 140,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mouse {
    /// How far a turn of the wheel scrolls, as a multiple of what the desktop says.
    pub scroll_multiplier: f32,
    /// The pointer goes out of sight while you type, until the mouse moves.
    pub hide_while_typing: bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Mouse { scroll_multiplier: 1., hide_while_typing: true }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Selection {
    /// The characters a double click stops at: what a word is made of is
    /// everything else.
    pub word_separators: String,
}

impl Default for Selection {
    fn default() -> Self {
        // Alacritty's, which is what lumen took before it could be changed
        Selection { word_separators: ",│`|:\"' ()[]{}<>\t".into() }
    }
}

/// What a program may do with the clipboard through OSC 52.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Clipboard {
    /// Put text on it: a program on a server you are logged into can copy to your desktop.
    #[default]
    Copy,
    /// Read what is on it.
    Paste,
    Both,
    Off,
}

/// What a key in `[keys]` does.
#[derive(Clone, Debug, PartialEq)]
pub enum Binding {
    /// One of lumen's actions, by its name (see [`crate::actions`]).
    Action(&'static str),
    /// This text goes to the program, as if typed.
    Send(String),
    /// Nothing of lumen's: the key goes to the program.
    Unbound,
}

/// The `[keys]` table: keys, as GPUI writes them (`ctrl-shift-t`), and what
/// each does. A key named here no longer does what lumen gave it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keys(pub Vec<(String, Binding)>);

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Name(String),
            Table(SendTable),
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SendTable {
            send: String,
        }
        let table = BTreeMap::<String, toml::Value>::deserialize(d)?;
        let mut keys: Vec<(String, Binding)> = Vec::new();
        let mut pressed = Vec::new();
        for (key, value) in table {
            let strokes = crate::actions::parse(&key).map_err(serde::de::Error::custom)?;
            // the same keys written two ways would leave which one counts to chance
            if let Some(at) = pressed.iter().position(|other| *other == strokes) {
                let first: &str = &keys[at].0;
                return Err(serde::de::Error::custom(format!("keys: {first:?} and {key:?} are the same keys")));
            }
            pressed.push(strokes);
            let value = Value::deserialize(value).map_err(|_| {
                serde::de::Error::custom(format!(
                    "keys: {key:?} is given neither an action's name nor {{ send = \"text\" }}"
                ))
            })?;
            let binding = match value {
                Value::Name(name) if name == "none" => Binding::Unbound,
                Value::Name(name) => match crate::actions::named(&name) {
                    Some(name) => Binding::Action(name),
                    None => {
                        return Err(serde::de::Error::custom(format!(
                            "keys: there is no action {name:?} (see the docs' Keys page for their names)"
                        )));
                    },
                },
                Value::Table(SendTable { send }) => Binding::Send(send),
            };
            keys.push((key, binding));
        }
        Ok(Keys(keys))
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
    /// The color of lumen's own light: the line under the active tab, the search.
    pub accent: Rgb,
    pub draw_bold_text_with_bright_colors: bool,
    /// The colors of lumen's own parts: the tabs, the frame, the lines between panes.
    pub ui: UiColors,
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
    /// Colors 16 to 255 of the 256, in place of the cube and the gray ramp.
    pub indexed_colors: Vec<IndexedColor>,
    #[serde(rename = "transparent_background_colors")]
    _transparent_background_colors: Unused,
}

/// A color that may be see-through: `#rrggbb`, or `#rrggbbaa`, where `aa` is
/// how much of what is behind it it covers (`ff` all of it, `00` nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tint(pub u8, pub u8, pub u8, pub u8);

impl Tint {
    pub fn parse(s: &str) -> Option<Tint> {
        let hex = s.trim().trim_start_matches('#').trim_start_matches("0x");
        match hex.len() {
            6 => Rgb::parse(hex).map(|Rgb(r, g, b)| Tint(r, g, b, 0xff)),
            8 => {
                let n = u32::from_str_radix(hex, 16).ok()?;
                Some(Tint((n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8))
            },
            _ => None,
        }
    }

    pub fn hsla(self) -> Hsla {
        Hsla { a: f32::from(self.3) / 255., ..Rgb(self.0, self.1, self.2).hsla() }
    }
}

impl<'de> Deserialize<'de> for Tint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Tint::parse(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "not a color: {s:?} (want \"#rrggbb\", or \"#rrggbbaa\" to see through it)"
            ))
        })
    }
}

/// `[colors.ui]`: the colors of what lumen draws itself. Each may be
/// see-through, and each that isn't set follows `text`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiColors {
    /// lumen's own words and signs. Unset, white on a dark background and black on a light one.
    pub text: Option<Tint>,
    /// The hairline around lumen's frame.
    pub border: Option<Tint>,
    /// The line between two panes.
    pub divider: Option<Tint>,
    /// Behind the tab bar: the window's background if unset.
    pub tab_bar: Option<Tint>,
    /// Behind the tab you are in.
    pub active_tab: Option<Tint>,
    /// The frame's shadow.
    pub shadow: Option<Tint>,
}

/// The colors lumen's own parts are drawn in, as they come out of
/// `[colors.ui]` and the theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ui {
    pub text: Hsla,
    pub border: Hsla,
    pub divider: Hsla,
    pub tab_bar: Option<Hsla>,
    pub active_tab: Hsla,
    pub shadow: Hsla,
}

/// One of the 256 colors, as Alacritty's `[[colors.indexed_colors]]` names it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedColor {
    pub index: u8,
    pub color: Rgb,
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
            ui: UiColors::default(),
            _dim: Unused,
            _vi_mode_cursor: Unused,
            _search: Unused,
            _hints: Unused,
            _line_indicator: Unused,
            _footer_bar: Unused,
            indexed_colors: Vec::new(),
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
    pub tabs: Tabs,
    pub panes: Panes,
    pub cursor: Cursor,
    pub mouse: Mouse,
    pub selection: Selection,
    pub colors: Colors,
    pub editor: Editor,
    pub pulse: Pulse,
    /// Variables every new terminal gets in its environment, over lumen's own
    /// (`TERM` among them).
    pub env: BTreeMap<String, String>,
    pub keys: Keys,
    pub clipboard: Clipboard,
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
            tabs: Tabs::default(),
            panes: Panes::default(),
            cursor: Cursor::default(),
            mouse: Mouse::default(),
            selection: Selection::default(),
            colors: Colors::default(),
            editor: Editor::default(),
            pulse: Pulse::default(),
            env: BTreeMap::new(),
            keys: Keys::default(),
            clipboard: Clipboard::default(),
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
        // TOML can write a number that is none (nan), which no range holds: it is the default
        let within = |value: f32, least: f32, most: f32, default: f32| {
            if value.is_nan() { default } else { value.clamp(least, most) }
        };
        let (font, window, mouse) = (Font::default(), Window::default(), Mouse::default());
        let (cursor, panes) = (Cursor::default(), Panes::default());
        self.font.family = installed_family(&self.font.family);
        self.font.size = within(self.font.size, 4., 96., font.size);
        self.font.line_height = within(self.font.line_height, 0.8, 3., font.line_height);
        self.font.weight = self.font.weight.clamp(100, 900);
        self.font.bold_weight = self.font.bold_weight.clamp(100, 900);
        self.font.letter_spacing = within(self.font.letter_spacing, -5., 20., font.letter_spacing);
        self.font.ui_size = within(self.font.ui_size, 8., 18., font.ui_size);
        self.window.corner_radius = within(self.window.corner_radius, 0., 40., window.corner_radius);
        self.window.background_image_opacity =
            within(self.window.background_image_opacity, 0., 1., window.background_image_opacity);
        self.window.gradient_angle = if self.window.gradient_angle.is_finite() {
            self.window.gradient_angle.rem_euclid(360.)
        } else {
            window.gradient_angle
        };
        self.panes.dim_inactive = within(self.panes.dim_inactive, 0., 1., panes.dim_inactive);
        self.panes.divider_width = within(self.panes.divider_width, 0., 8., panes.divider_width);
        self.cursor.glow_strength = within(self.cursor.glow_strength, 0., 3., cursor.glow_strength);
        self.cursor.glow_size = within(self.cursor.glow_size, 0.2, 4., cursor.glow_size);
        self.cursor.glide_time = self.cursor.glide_time.clamp(10, 1000);
        self.cursor.blink_dim = within(self.cursor.blink_dim, 0., 1., cursor.blink_dim);
        self.cursor.blink_fade = self.cursor.blink_fade.min(1000);
        self.window.opacity = within(self.window.opacity, 0., 1., window.opacity);
        self.window.padding.x = within(self.window.padding.x, 0., 200., window.padding.x);
        self.window.padding.y = within(self.window.padding.y, 0., 200., window.padding.y);
        self.window.columns = self.window.columns.clamp(20, 1000);
        self.window.lines = self.window.lines.clamp(5, 500);
        self.cursor.blink_interval = self.cursor.blink_interval.clamp(100, 5000);
        self.cursor.thickness = self.cursor.thickness.filter(|t| !t.is_nan()).map(|t| t.clamp(1., 20.));
        self.mouse.scroll_multiplier = within(self.mouse.scroll_multiplier, 0.1, 20., mouse.scroll_multiplier);
        self.pulse.every = self.pulse.every.clamp(1, 60);
        self
    }

    /// Whether a window with this many tabs shows the tab bar.
    pub fn tab_bar(&self, tabs: usize) -> bool {
        match self.tabs.bar {
            TabBar::Always => true,
            TabBar::Auto => tabs > 1,
            TabBar::Never => false,
        }
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

    /// The colors of lumen's own parts: what `[colors.ui]` sets, and for the
    /// rest, shades of its text color, which is white or black, whichever
    /// stands out more on the theme's background.
    pub fn ui(&self) -> Ui {
        let set = &self.colors.ui;
        let background = self.colors.primary.background;
        let white = Rgb(0xff, 0xff, 0xff);
        let black = Rgb(0, 0, 0);
        let auto = if crate::palette::contrast(white, background) >= crate::palette::contrast(black, background) {
            white
        } else {
            black
        };
        let text = set.text.map(Tint::hsla).unwrap_or_else(|| auto.hsla());
        let or = |tint: Option<Tint>, alpha: f32| tint.map(Tint::hsla).unwrap_or_else(|| text.opacity(alpha));
        Ui {
            text,
            border: or(set.border, 0.1),
            divider: or(set.divider, 0.08),
            tab_bar: set.tab_bar.map(Tint::hsla),
            active_tab: or(set.active_tab, 0.09),
            shadow: set.shadow.map(Tint::hsla).unwrap_or_else(|| gpui::black().opacity(0.35)),
        }
    }

    /// What a window asks the desktop for: a frame of the desktop's, or none
    /// (and lumen draws its own, or none either).
    pub fn decorations(&self) -> WindowDecorations {
        match self.window.decorations {
            Decorations::Desktop => WindowDecorations::Server,
            Decorations::Lumen | Decorations::None => WindowDecorations::Client,
        }
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

    #[test]
    fn padding_is_one_number_or_two() {
        let same: Config = toml::from_str("[window]\npadding = 4\n").unwrap();
        assert_eq!(same.window.padding, Padding { x: 4., y: 4. });
        let apart: Config = toml::from_str("[window]\npadding = { x = 2, y = 0.5 }\n").unwrap();
        assert_eq!(apart.window.padding, Padding { x: 2., y: 0.5 });
        // a side left out keeps lumen's
        let one: Config = toml::from_str("[window.padding]\ny = 0\n").unwrap();
        assert_eq!(one.window.padding, Padding { x: 10., y: 0. });
        assert!(toml::from_str::<Config>("[window]\npadding = { x = 1, z = 2 }\n").is_err());
        assert!(toml::from_str::<Config>("[window]\npadding = \"wide\"\n").is_err());
        let huge = Config {
            window: Window { padding: Padding { x: -3., y: 900. }, ..Window::default() },
            ..Config::default()
        };
        assert_eq!(huge.checked().window.padding, Padding { x: 0., y: 200. });
        // a number that is none is the default, and infinity the largest allowed
        let none: Config = toml::from_str("[window]\npadding = nan\n[mouse]\nscroll_multiplier = nan\n").unwrap();
        let none = none.checked();
        assert_eq!(none.window.padding, Padding::default());
        assert_eq!(none.mouse.scroll_multiplier, 1.);
        let endless: Config = toml::from_str("[mouse]\nscroll_multiplier = inf\n[cursor]\nthickness = nan\n").unwrap();
        let endless = endless.checked();
        assert_eq!(endless.mouse.scroll_multiplier, 20.);
        assert_eq!(endless.cursor.thickness, None);
    }

    #[test]
    fn the_frame_the_tab_bar_and_the_cursor() {
        let config: Config = toml::from_str(
            r#"
            [window]
            decorations = "none"
            startup_mode = "maximized"
            dynamic_padding = true

            [tabs]
            bar = "auto"

            [cursor]
            rounded = false
            blink_interval = 800
            thickness = 3
            unfocused = "hidden"

            [font]
            ligatures = false
            weight = 300
            bold_weight = 600

            [mouse]
            scroll_multiplier = 3
            hide_while_typing = false

            [selection]
            word_separators = " "
            "#,
        )
        .unwrap();
        assert_eq!(config.window.decorations, Decorations::None);
        assert_eq!(config.decorations(), WindowDecorations::Client);
        assert_eq!(config.window.startup_mode, StartupMode::Maximized);
        assert!(config.window.dynamic_padding);
        assert!(!config.tab_bar(1) && config.tab_bar(2));
        assert!(!config.cursor.rounded);
        assert_eq!(config.cursor.blink_interval, 800);
        assert_eq!(config.cursor.thickness, Some(3.));
        assert_eq!(config.cursor.unfocused, Unfocused::Hidden);
        assert!(!config.font.ligatures);
        assert_eq!((config.font.weight, config.font.bold_weight), (300, 600));
        assert_eq!(config.mouse.scroll_multiplier, 3.);
        assert!(!config.mouse.hide_while_typing);
        assert_eq!(config.selection.word_separators, " ");

        let desktop: Config = toml::from_str("[window]\ndecorations = \"desktop\"\n[tabs]\nbar = \"never\"\n").unwrap();
        assert_eq!(desktop.decorations(), WindowDecorations::Server);
        assert!(!desktop.tab_bar(5));
        assert!(Config::default().tab_bar(1));
        assert!(toml::from_str::<Config>("[window]\ndecorations = \"kde\"\n").is_err());
        assert!(toml::from_str::<Config>("[tabs]\nbar = \"sometimes\"\n").is_err());
        assert!(toml::from_str::<Config>("[cursor]\nunfocused = \"gone\"\n").is_err());
    }

    #[test]
    fn keys_the_environment_and_the_clipboard() {
        let config: Config = toml::from_str(
            r#"
            clipboard = "both"

            [env]
            TERM = "xterm"
            EDITOR = "hx"

            [keys]
            "ctrl-shift-t" = "none"
            "alt-t" = "new_tab"
            "shift-enter" = { send = "\u001b\r" }
            "#,
        )
        .unwrap();
        assert_eq!(config.clipboard, Clipboard::Both);
        assert_eq!(config.env.get("TERM").map(String::as_str), Some("xterm"));
        assert_eq!(config.env.len(), 2);
        let bound = |key: &str| config.keys.0.iter().find(|(k, _)| k == key).map(|(_, b)| b.clone());
        assert_eq!(bound("ctrl-shift-t"), Some(Binding::Unbound));
        assert_eq!(bound("alt-t"), Some(Binding::Action("new_tab")));
        assert_eq!(bound("shift-enter"), Some(Binding::Send("\x1b\r".into())));

        // a mistake in a key, an action or the shape of a binding is said
        assert!(toml::from_str::<Config>("[keys]\n\"ctrl-shift-entr\" = \"copy\"\n").is_err());
        assert!(toml::from_str::<Config>("[keys]\n\"ctrl-c\" = \"cpoy\"\n").is_err());
        assert!(toml::from_str::<Config>("[keys]\n\"ctrl-c\" = { sned = \"x\" }\n").is_err());
        assert!(toml::from_str::<Config>("[keys]\n\"ctrl-c\" = 3\n").is_err());
        let twice = toml::from_str::<Config>("[keys]\n\"ctrl-shift-t\" = \"none\"\n\"shift-ctrl-t\" = \"copy\"\n");
        assert!(twice.unwrap_err().message().contains("are the same keys"));
        assert!(toml::from_str::<Config>("clipboard = \"always\"\n").is_err());
        assert!(toml::from_str::<Config>("[env]\nN = 1\n").is_err());
    }

    #[test]
    fn indexed_colors_are_the_themes() {
        let config: Config = toml::from_str(
            r##"
            [[colors.indexed_colors]]
            index = 16
            color = "#fab387"

            [[colors.indexed_colors]]
            index = 255
            color = "#010203"
            "##,
        )
        .unwrap();
        assert_eq!(crate::palette::indexed(16, &config.colors), Rgb(0xfa, 0xb3, 0x87));
        assert_eq!(crate::palette::indexed(255, &config.colors), Rgb(1, 2, 3));
        // the ones it doesn't name are the usual ones
        assert_eq!(crate::palette::indexed(17, &config.colors), crate::palette::indexed(17, &Colors::default()));
        assert!(toml::from_str::<Config>("[[colors.indexed_colors]]\nindex = 300\ncolor = \"#000000\"\n").is_err());
    }

    #[test]
    fn a_tint_is_a_color_that_may_be_see_through() {
        assert_eq!(Tint::parse("#ff8000"), Some(Tint(0xff, 0x80, 0x00, 0xff)));
        assert_eq!(Tint::parse("#ff800080"), Some(Tint(0xff, 0x80, 0x00, 0x80)));
        assert_eq!(Tint::parse("0x00000000"), Some(Tint(0, 0, 0, 0)));
        assert_eq!(Tint::parse("#ff80"), None);
        assert_eq!(Tint::parse("red"), None);
        assert!((Tint(0, 0, 0, 0x80).hsla().a - 0.502).abs() < 0.01);
        assert!(toml::from_str::<Config>("[colors.ui]\ntext = \"#12345\"\n").is_err());
        assert!(toml::from_str::<Config>("[colors.ui]\ntxet = \"#123456\"\n").is_err());
    }

    #[test]
    fn lumens_own_colors_follow_the_theme() {
        // white on lumen's dark theme, at the shades lumen always had
        let dark = Config::default().ui();
        assert_eq!(dark.text, gpui::white());
        assert_eq!(dark.border, gpui::white().opacity(0.1));
        assert_eq!(dark.divider, gpui::white().opacity(0.08));
        assert_eq!(dark.active_tab, gpui::white().opacity(0.09));
        assert_eq!(dark.tab_bar, None);
        // black on a light one, where white would not be seen
        let light: Config = toml::from_str("[colors.primary]\nbackground = \"#fafafa\"\n").unwrap();
        assert_eq!(light.ui().text, gpui::black());
        assert_eq!(light.ui().border, gpui::black().opacity(0.1));
        // what is set wins, and what isn't follows the text that is set
        let set: Config =
            toml::from_str("[colors.ui]\ntext = \"#ff0000\"\ntab_bar = \"#00000080\"\ndivider = \"#00ff00\"\n")
                .unwrap();
        let ui = set.ui();
        assert_eq!(ui.text, Rgb(0xff, 0, 0).hsla());
        assert_eq!(ui.border, Rgb(0xff, 0, 0).hsla().opacity(0.1));
        assert_eq!(ui.divider, Rgb(0, 0xff, 0).hsla());
        assert!(ui.tab_bar.is_some_and(|bar| (bar.a - 0.502).abs() < 0.01));
    }

    #[test]
    fn the_looks_are_read_and_kept_in_range() {
        let config: Config = toml::from_str(
            r##"
            [window]
            corner_radius = 99
            shadow = false
            border = false
            background_image = "~/wall.png"
            background_image_opacity = 2
            background_image_fit = "contain"
            gradient = "#000000"
            gradient_angle = -90

            [tabs]
            position = "bottom"

            [panes]
            dim_inactive = nan
            divider_width = 3

            [cursor]
            glow_strength = 9
            glow_size = 0
            glide_time = 1
            blink_dim = 0
            blink_fade = 5000

            [font]
            letter_spacing = 1.5
            ui_family = "Inter"
            ui_size = 30
            "##,
        )
        .unwrap();
        let config = config.checked();
        assert_eq!(config.window.corner_radius, 40.);
        assert!(!config.window.shadow && !config.window.border);
        assert_eq!(config.window.background_image(), dirs::home_dir().map(|home| home.join("wall.png")));
        assert_eq!(config.window.background_image_opacity, 1.);
        assert_eq!(config.window.background_image_fit, ImageFit::Contain);
        assert_eq!(config.window.gradient, Some(Rgb(0, 0, 0)));
        assert_eq!(config.window.gradient_angle, 270.);
        assert_eq!(config.tabs.position, TabPosition::Bottom);
        assert_eq!(config.panes.dim_inactive, Panes::default().dim_inactive);
        assert_eq!(config.panes.divider_width, 3.);
        assert_eq!(config.cursor.glow_strength, 3.);
        assert_eq!(config.cursor.glow_size, 0.2);
        assert_eq!(config.cursor.glide_time, 10);
        assert_eq!(config.cursor.blink_dim, 0.);
        assert_eq!(config.cursor.blink_fade, 1000);
        assert_eq!(config.font.letter_spacing, 1.5);
        assert_eq!(config.font.ui_family.as_deref(), Some("Inter"));
        assert_eq!(config.font.ui_size, 18.);
        assert!(toml::from_str::<Config>("[window]\nbackground_image_fit = \"tile\"\n").is_err());
        assert!(toml::from_str::<Config>("[tabs]\nposition = \"left\"\n").is_err());
        assert!(toml::from_str::<Config>("[panes]\ndim = 0.5\n").is_err());
    }
}
