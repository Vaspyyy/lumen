//! Everything a key can be bound to, and the keys that are: lumen's own, and
//! what `[keys]` in the config makes of them.

use std::rc::Rc;

use gpui::{Action, App, KeyBinding, KeyBindingContextPredicate, Keystroke, Modifiers, OperatingSystem, actions};

use crate::config::{Binding, Config};

actions!(
    lumen,
    [
        Copy,
        Paste,
        ScrollPageUp,
        ScrollPageDown,
        ScrollLineUp,
        ScrollLineDown,
        ScrollTop,
        ScrollBottom,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ToggleSearch,
        PreviousCommand,
        NextCommand,
        CopyOutput,
        NewTab,
        CloseTab,
        NextTab,
        PreviousTab,
        MoveTabLeft,
        MoveTabRight,
        SplitRight,
        SplitDown,
        ClosePane,
        FocusLeft,
        FocusRight,
        FocusUp,
        FocusDown,
        ToggleZoomPane,
        NewWindow,
        Quit,
    ]
);

/// Go to a tab by its place: 1 is the first, 9 the last.
#[derive(Clone, PartialEq, Eq, Debug, gpui::Action)]
#[action(namespace = lumen, no_json)]
pub struct GoToTab(pub usize);

/// Text for the program in the pane, as if it was typed: what `{ send = "..." }`
/// in `[keys]` binds a key to.
#[derive(Clone, PartialEq, Eq, Debug, gpui::Action)]
#[action(namespace = lumen, no_json)]
pub struct SendText(pub String);

/// Where a key bound to an action works.
#[derive(Clone, Copy)]
enum Scope {
    /// Anywhere in a window.
    Window,
    /// In a pane.
    Pane,
    /// In a pane whose shell marks its commands, outside a full-screen program:
    /// a full-screen program has no commands on its screen, and these keys are its own there.
    Shell,
}

impl Scope {
    fn context(self) -> Option<&'static str> {
        match self {
            Scope::Window => None,
            Scope::Pane => Some("Terminal"),
            Scope::Shell => Some("Terminal && marks && !alt_screen"),
        }
    }
}

type Make = fn() -> Box<dyn Action>;

/// The actions, by the names `[keys]` knows them by.
static NAMED: &[(&str, Make, Scope)] = &[
    ("copy", || Box::new(Copy), Scope::Pane),
    ("paste", || Box::new(Paste), Scope::Pane),
    ("scroll_page_up", || Box::new(ScrollPageUp), Scope::Pane),
    ("scroll_page_down", || Box::new(ScrollPageDown), Scope::Pane),
    ("scroll_line_up", || Box::new(ScrollLineUp), Scope::Pane),
    ("scroll_line_down", || Box::new(ScrollLineDown), Scope::Pane),
    ("scroll_top", || Box::new(ScrollTop), Scope::Pane),
    ("scroll_bottom", || Box::new(ScrollBottom), Scope::Pane),
    ("zoom_in", || Box::new(ZoomIn), Scope::Pane),
    ("zoom_out", || Box::new(ZoomOut), Scope::Pane),
    ("zoom_reset", || Box::new(ZoomReset), Scope::Pane),
    ("search", || Box::new(ToggleSearch), Scope::Pane),
    ("previous_command", || Box::new(PreviousCommand), Scope::Shell),
    ("next_command", || Box::new(NextCommand), Scope::Shell),
    ("copy_output", || Box::new(CopyOutput), Scope::Shell),
    ("new_tab", || Box::new(NewTab), Scope::Window),
    ("close_tab", || Box::new(CloseTab), Scope::Window),
    ("next_tab", || Box::new(NextTab), Scope::Window),
    ("previous_tab", || Box::new(PreviousTab), Scope::Window),
    ("move_tab_left", || Box::new(MoveTabLeft), Scope::Window),
    ("move_tab_right", || Box::new(MoveTabRight), Scope::Window),
    ("tab_1", || Box::new(GoToTab(1)), Scope::Window),
    ("tab_2", || Box::new(GoToTab(2)), Scope::Window),
    ("tab_3", || Box::new(GoToTab(3)), Scope::Window),
    ("tab_4", || Box::new(GoToTab(4)), Scope::Window),
    ("tab_5", || Box::new(GoToTab(5)), Scope::Window),
    ("tab_6", || Box::new(GoToTab(6)), Scope::Window),
    ("tab_7", || Box::new(GoToTab(7)), Scope::Window),
    ("tab_8", || Box::new(GoToTab(8)), Scope::Window),
    ("last_tab", || Box::new(GoToTab(9)), Scope::Window),
    ("split_right", || Box::new(SplitRight), Scope::Window),
    ("split_down", || Box::new(SplitDown), Scope::Window),
    ("close_pane", || Box::new(ClosePane), Scope::Window),
    ("focus_left", || Box::new(FocusLeft), Scope::Window),
    ("focus_right", || Box::new(FocusRight), Scope::Window),
    ("focus_up", || Box::new(FocusUp), Scope::Window),
    ("focus_down", || Box::new(FocusDown), Scope::Window),
    ("zoom_pane", || Box::new(ToggleZoomPane), Scope::Window),
    ("new_window", || Box::new(NewWindow), Scope::Window),
    ("quit", || Box::new(Quit), Scope::Window),
];

/// lumen's own keys, and the actions they are bound to.
const DEFAULTS: &[(&str, &str)] = &[
    ("ctrl-shift-c", "copy"),
    ("ctrl-shift-v", "paste"),
    ("shift-insert", "paste"),
    ("shift-pageup", "scroll_page_up"),
    ("shift-pagedown", "scroll_page_down"),
    ("shift-home", "scroll_top"),
    ("shift-end", "scroll_bottom"),
    ("ctrl-=", "zoom_in"),
    ("ctrl-+", "zoom_in"),
    ("ctrl--", "zoom_out"),
    ("ctrl-0", "zoom_reset"),
    ("ctrl-shift-f", "search"),
    ("ctrl-shift-up", "previous_command"),
    ("ctrl-shift-down", "next_command"),
    ("ctrl-shift-o", "copy_output"),
    ("ctrl-shift-t", "new_tab"),
    ("ctrl-shift-w", "close_pane"),
    ("ctrl-shift-q", "close_tab"),
    ("ctrl-tab", "next_tab"),
    ("ctrl-shift-tab", "previous_tab"),
    ("ctrl-pagedown", "next_tab"),
    ("ctrl-pageup", "previous_tab"),
    ("ctrl-shift-pageup", "move_tab_left"),
    ("ctrl-shift-pagedown", "move_tab_right"),
    ("ctrl-shift-d", "split_right"),
    ("ctrl-shift-e", "split_down"),
    ("ctrl-alt-left", "focus_left"),
    ("ctrl-alt-right", "focus_right"),
    ("ctrl-alt-up", "focus_up"),
    ("ctrl-alt-down", "focus_down"),
    ("ctrl-shift-enter", "zoom_pane"),
    ("ctrl-shift-n", "new_window"),
    ("alt-1", "tab_1"),
    ("alt-2", "tab_2"),
    ("alt-3", "tab_3"),
    ("alt-4", "tab_4"),
    ("alt-5", "tab_5"),
    ("alt-6", "tab_6"),
    ("alt-7", "tab_7"),
    ("alt-8", "tab_8"),
    ("alt-9", "last_tab"),
];

/// The keys GPUI names by a word, as it names them. Any other key is the
/// one character it types.
const KEY_NAMES: &[&str] = &[
    "enter",
    "tab",
    "space",
    "backspace",
    "delete",
    "escape",
    "insert",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
    "menu",
    "print",
    "pause",
    "back",
    "forward",
];

/// The action of this name, as the config can keep it: its name for as long
/// as lumen runs.
pub fn named(name: &str) -> Option<&'static str> {
    NAMED.iter().find(|(known, ..)| *known == name).map(|(known, ..)| *known)
}

/// The keystrokes a binding is pressed with: one, or several one after
/// another (`ctrl-k ctrl-c`).
fn parse(keys: &str) -> Result<Vec<(Modifiers, String)>, String> {
    let mut strokes = Vec::new();
    for part in keys.split_whitespace() {
        let stroke = Keystroke::parse_with_os(part, OperatingSystem::Linux)
            .map_err(|_| format!("keys: {keys:?} is not a key (write them as \"ctrl-shift-t\")"))?;
        let key = stroke.key.as_str();
        let function = key.strip_prefix('f').is_some_and(|n| n.parse::<u8>().is_ok_and(|n| (1..=35).contains(&n)));
        if key.chars().count() != 1 && !function && !KEY_NAMES.contains(&key) {
            return Err(format!("keys: lumen doesn't know the key {key:?} in {keys:?}"));
        }
        strokes.push((stroke.modifiers, stroke.key));
    }
    if strokes.is_empty() {
        return Err("keys: a key with no name".into());
    }
    Ok(strokes)
}

/// Whether a key in `[keys]` is one lumen can bind.
pub fn check_keys(keys: &str) -> Result<(), String> {
    parse(keys).map(|_| ())
}

/// Puts the keys in place: lumen's own, without those the config names, and
/// the config's. Called again when the config file is saved.
pub fn bind(cx: &mut App) {
    let user = cx.global::<Config>().keys.0.clone();
    let user_keys: Vec<Vec<(Modifiers, String)>> = user.iter().filter_map(|(keys, _)| parse(keys).ok()).collect();
    let action = |name: &str| NAMED.iter().find(|(known, ..)| *known == name).map(|&(_, make, scope)| (make(), scope));
    let mut bindings = Vec::new();
    let mut add = |keys: &str, action: Box<dyn Action>, scope: Scope, cx: &App| {
        let context = scope.context().and_then(|context| KeyBindingContextPredicate::parse(context).ok()).map(Rc::new);
        if let Ok(binding) = KeyBinding::load(keys, action, context, false, None, cx) {
            bindings.push(binding);
        }
    };
    for &(keys, name) in DEFAULTS {
        let taken = parse(keys).is_ok_and(|strokes| user_keys.contains(&strokes));
        if let (false, Some((action, scope))) = (taken, action(name)) {
            add(keys, action, scope, cx);
        }
    }
    for (keys, binding) in &user {
        match binding {
            Binding::Action(name) => {
                if let Some((action, scope)) = action(name) {
                    add(keys, action, scope, cx);
                }
            },
            Binding::Send(text) => add(keys, Box::new(SendText(text.clone())), Scope::Pane, cx),
            Binding::Unbound => {},
        }
    }
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_key_is_a_key_and_its_action_has_a_name() {
        for &(keys, name) in DEFAULTS {
            assert!(check_keys(keys).is_ok(), "{keys}");
            assert!(named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn keys_are_the_same_however_they_are_written() {
        assert_eq!(parse("ctrl-shift-t"), parse("shift-ctrl-t"));
        assert_eq!(parse("ctrl-shift-t"), parse("ctrl-T"));
        assert_ne!(parse("ctrl-t"), parse("ctrl-shift-t"));
        assert_eq!(parse("ctrl-k ctrl-c").unwrap().len(), 2);
    }

    #[test]
    fn a_key_lumen_doesnt_know_is_an_error() {
        assert!(check_keys("ctrl-shift-entr").is_err());
        assert!(check_keys("ctrl-shift-").is_err());
        // the minus key is a second dash
        assert!(check_keys("ctrl--").is_ok());
        assert!(check_keys("").is_err());
        assert!(check_keys("ctrl-shift-enter").is_ok());
        assert!(check_keys("f12").is_ok());
        assert!(check_keys("alt-[").is_ok());
        assert!(check_keys("super-t").is_ok());
    }
}
