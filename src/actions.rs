//! Everything a key can be bound to.

use gpui::{App, KeyBinding, actions};

actions!(
    lumen,
    [
        Copy,
        Paste,
        ScrollPageUp,
        ScrollPageDown,
        ScrollTop,
        ScrollBottom,
        ZoomIn,
        ZoomOut,
        ZoomReset,
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

pub fn bind(cx: &mut App) {
    let terminal = Some("Terminal");
    let mut keys = vec![
        KeyBinding::new("ctrl-shift-c", Copy, terminal),
        KeyBinding::new("ctrl-shift-v", Paste, terminal),
        KeyBinding::new("shift-insert", Paste, terminal),
        KeyBinding::new("shift-pageup", ScrollPageUp, terminal),
        KeyBinding::new("shift-pagedown", ScrollPageDown, terminal),
        KeyBinding::new("shift-home", ScrollTop, terminal),
        KeyBinding::new("shift-end", ScrollBottom, terminal),
        KeyBinding::new("ctrl-=", ZoomIn, terminal),
        KeyBinding::new("ctrl-+", ZoomIn, terminal),
        KeyBinding::new("ctrl--", ZoomOut, terminal),
        KeyBinding::new("ctrl-0", ZoomReset, terminal),
        KeyBinding::new("ctrl-shift-t", NewTab, None),
        KeyBinding::new("ctrl-shift-w", ClosePane, None),
        KeyBinding::new("ctrl-shift-q", CloseTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("ctrl-pagedown", NextTab, None),
        KeyBinding::new("ctrl-pageup", PreviousTab, None),
        KeyBinding::new("ctrl-shift-pageup", MoveTabLeft, None),
        KeyBinding::new("ctrl-shift-pagedown", MoveTabRight, None),
        KeyBinding::new("ctrl-shift-d", SplitRight, None),
        KeyBinding::new("ctrl-shift-e", SplitDown, None),
        KeyBinding::new("ctrl-alt-left", FocusLeft, None),
        KeyBinding::new("ctrl-alt-right", FocusRight, None),
        KeyBinding::new("ctrl-alt-up", FocusUp, None),
        KeyBinding::new("ctrl-alt-down", FocusDown, None),
        KeyBinding::new("ctrl-shift-enter", ToggleZoomPane, None),
        KeyBinding::new("ctrl-shift-n", NewWindow, None),
    ];
    for (n, key) in
        ["alt-1", "alt-2", "alt-3", "alt-4", "alt-5", "alt-6", "alt-7", "alt-8", "alt-9"].into_iter().enumerate()
    {
        keys.push(KeyBinding::new(key, GoToTab(n + 1), None));
    }
    cx.bind_keys(keys);
}
