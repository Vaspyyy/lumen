//! A terminal on screen: its keyboard, mouse, selection and clipboard.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Render, ScrollDelta, ScrollEvent, Styled, Subscription, Task, UTF16Selection, Window, div,
};

use crate::actions::{Copy, Paste, ScrollBottom, ScrollPageDown, ScrollPageUp, ScrollTop, ZoomIn, ZoomOut, ZoomReset};
use crate::config::Config;
use crate::element::{Geometry, TerminalElement};
use crate::terminal::{Started, Terminal, TerminalEvent};
use crate::{keys, mouse};

pub enum ViewEvent {
    /// The program ended: the pane can go.
    Closed,
    /// The title changed.
    Title,
    /// The pane was clicked or otherwise took the keyboard.
    Focused,
}

pub struct TerminalView {
    pub terminal: Entity<Terminal>,
    pub focus: FocusHandle,
    geometry: Rc<Cell<Option<Geometry>>>,
    zoom: f32,
    /// A selection is being dragged out.
    selecting: bool,
    /// The cell the last mouse report was for, so a move within a cell says nothing.
    reported_cell: Option<(usize, usize)>,
    /// Wheel movement that hasn't added up to a line yet.
    scroll_rest: f32,
    last_input: Instant,
    blink_on: bool,
    /// Text an input method is still composing.
    composing: Option<String>,
    _blink: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ViewEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TerminalView {
    pub fn new(started: Started, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let terminal = cx.new(|cx| Terminal::new(started, cx));
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.observe(&terminal, |_, _, cx| cx.notify()),
            cx.subscribe(&terminal, |_, _, event: &TerminalEvent, cx| match event {
                TerminalEvent::Title => cx.emit(ViewEvent::Title),
                TerminalEvent::Exited => cx.emit(ViewEvent::Closed),
                TerminalEvent::Bell => {},
            }),
            cx.on_focus(&focus, window, |this, _, cx| {
                this.touch();
                this.report_focus(true, cx);
                cx.emit(ViewEvent::Focused);
                cx.notify();
            }),
            cx.on_blur(&focus, window, |this, _, cx| {
                this.report_focus(false, cx);
                cx.notify();
            }),
        ];
        let blink = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(530)).await;
                let alive = this.update(cx, |this, cx| {
                    if !cx.global::<Config>().cursor.blink {
                        return;
                    }
                    // typing holds the cursor lit
                    let on = !this.blink_on || this.last_input.elapsed() < Duration::from_millis(500);
                    if on != this.blink_on {
                        this.blink_on = on;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        TerminalView {
            terminal,
            focus,
            geometry: Rc::new(Cell::new(None)),
            zoom: 1.,
            selecting: false,
            reported_cell: None,
            scroll_rest: 0.,
            last_input: Instant::now(),
            blink_on: true,
            composing: None,
            _blink: blink,
            _subscriptions: subscriptions,
        }
    }

    fn touch(&mut self) {
        self.last_input = Instant::now();
        self.blink_on = true;
    }

    fn mode(&self, cx: &App) -> TermMode {
        self.terminal.read(cx).mode()
    }

    fn report_focus(&self, focused: bool, cx: &App) {
        if self.mode(cx).contains(TermMode::FOCUS_IN_OUT) {
            self.terminal.read(cx).write(if focused { &b"\x1b[I"[..] } else { &b"\x1b[O"[..] });
        }
    }

    /// Sends what was typed, and shows the prompt it went to.
    fn send(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let terminal = self.terminal.read(cx);
        terminal.clear_selection();
        terminal.scroll(Scroll::Bottom);
        terminal.write(bytes);
        self.touch();
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.composing.is_some() {
            return;
        }
        if let Some(bytes) = keys::encode(&event.keystroke, self.mode(cx)) {
            self.send(bytes, cx);
            cx.stop_propagation();
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.terminal.read(cx).selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.paste_text(&text, cx);
        }
    }

    fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let bytes = if self.mode(cx).contains(TermMode::BRACKETED_PASTE) {
            // the end marker inside the text would end the paste early and run the rest
            let text = text.replace('\x1b', "");
            format!("\x1b[200~{text}\x1b[201~").into_bytes()
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
        };
        self.send(bytes, cx);
    }

    fn scroll_by(&mut self, scroll: Scroll, cx: &mut Context<Self>) {
        self.terminal.read(cx).scroll(scroll);
        cx.notify();
    }

    fn zoom(&mut self, factor: Option<f32>, cx: &mut Context<Self>) {
        self.zoom = match factor {
            Some(f) => (self.zoom * f).clamp(0.4, 4.),
            None => 1.,
        };
        cx.notify();
    }

    pub fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let (col, row, right_half) = geometry.cell_at(event.position);
        let mode = self.mode(cx);
        if mode.intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            if let Some(button) = mouse::button_code(event.button) {
                self.reported_cell = Some((col, row));
                self.terminal.read(cx).write(mouse::report(button, true, col, row, event.modifiers, mode));
            }
            return;
        }
        match event.button {
            MouseButton::Left => {
                let ty = match event.click_count {
                    0 | 1 => SelectionType::Simple,
                    2 => SelectionType::Semantic,
                    _ => SelectionType::Lines,
                };
                let ty = if event.modifiers.control && event.click_count <= 1 { SelectionType::Block } else { ty };
                let side = if right_half { Side::Right } else { Side::Left };
                let terminal = self.terminal.read(cx);
                terminal.start_selection(ty, terminal.grid_point(col, row), side);
                self.selecting = true;
                cx.notify();
            },
            MouseButton::Middle => {
                if let Some(text) = cx.read_from_primary().and_then(|item| item.text()) {
                    self.paste_text(&text, cx);
                }
            },
            _ => {},
        }
    }

    pub fn mouse_move(&mut self, event: &MouseMoveEvent, hovered: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let (col, row, right_half) = geometry.cell_at(event.position);
        if self.selecting {
            let side = if right_half { Side::Right } else { Side::Left };
            let terminal = self.terminal.read(cx);
            terminal.update_selection(terminal.grid_point(col, row), side);
            cx.notify();
            return;
        }
        let mode = self.mode(cx);
        let wanted = mode.contains(TermMode::MOUSE_MOTION)
            || (mode.contains(TermMode::MOUSE_DRAG) && event.pressed_button.is_some());
        if hovered && wanted && !event.modifiers.shift && self.reported_cell != Some((col, row)) {
            self.reported_cell = Some((col, row));
            let button = event.pressed_button.and_then(mouse::button_code).unwrap_or(mouse::NO_BUTTON);
            self.terminal.read(cx).write(mouse::report(button + mouse::MOTION, true, col, row, event.modifiers, mode));
        }
    }

    pub fn mouse_up(&mut self, event: &MouseUpEvent, hovered: bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            if event.button == MouseButton::Left {
                self.selecting = false;
                if let Some(text) = self.terminal.read(cx).selection_text() {
                    cx.write_to_primary(ClipboardItem::new_string(text.clone()));
                    if cx.global::<Config>().copy_on_select {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                } else {
                    // a click that selected nothing takes the old selection away
                    self.terminal.read(cx).clear_selection();
                }
                cx.notify();
            }
            return;
        }
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let mode = self.mode(cx);
        if hovered && mode.intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            if let Some(button) = mouse::button_code(event.button) {
                let (col, row, _) = geometry.cell_at(event.position);
                self.terminal.read(cx).write(mouse::report(button, false, col, row, event.modifiers, mode));
            }
        }
    }

    pub fn scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(geometry) = self.geometry.get() else {
            return;
        };
        let lines = match event.delta {
            ScrollDelta::Pixels(delta) => delta.y / geometry.cell.height,
            ScrollDelta::Lines(delta) => delta.y * 3.,
        };
        self.scroll_rest += lines;
        let whole = self.scroll_rest.trunc();
        self.scroll_rest -= whole;
        let steps = whole as i32;
        if steps == 0 {
            return;
        }
        let mode = self.mode(cx);
        let terminal = self.terminal.read(cx);
        if mode.intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            let (col, row, _) = geometry.cell_at(event.position);
            let button = if steps > 0 { mouse::WHEEL_UP } else { mouse::WHEEL_DOWN };
            for _ in 0..steps.unsigned_abs() {
                terminal.write(mouse::report(button, true, col, row, event.modifiers, mode));
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !event.modifiers.shift {
            // a full-screen program without mouse support (less, man) scrolls by arrow keys
            let key: &[u8] = match (steps > 0, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            terminal.write(key.repeat(steps.unsigned_abs() as usize));
        } else {
            terminal.scroll(Scroll::Delta(steps));
        }
        cx.notify();
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window) && window.is_window_active();
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(|this, _: &ScrollPageUp, _, cx| this.scroll_by(Scroll::PageUp, cx)))
            .on_action(cx.listener(|this, _: &ScrollPageDown, _, cx| this.scroll_by(Scroll::PageDown, cx)))
            .on_action(cx.listener(|this, _: &ScrollTop, _, cx| this.scroll_by(Scroll::Top, cx)))
            .on_action(cx.listener(|this, _: &ScrollBottom, _, cx| this.scroll_by(Scroll::Bottom, cx)))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom(Some(1.1), cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom(Some(1. / 1.1), cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.zoom(None, cx)))
            .child(TerminalElement {
                terminal: self.terminal.clone(),
                view: cx.entity(),
                focus: self.focus.clone(),
                geometry: self.geometry.clone(),
                focused,
                cursor_on: self.blink_on,
                zoom: self.zoom,
            })
    }
}

/// Text input. The terminal has no text of its own to edit: what the input
/// method commits is sent to the program, and what it is still composing
/// is held back.
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.composing.as_ref().map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.composing = None;
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.composing = None;
        if !text.is_empty() {
            self.send(text.as_bytes().to_vec(), cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        new_text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composing = (!new_text.is_empty()).then(|| new_text.to_string());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // where the input method's popup goes: the top left of the grid, until
        // the cursor's place is tracked here
        self.geometry.get().map(|g| Bounds::new(g.origin, g.cell))
    }

    fn character_index_for_point(
        &mut self,
        _: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
