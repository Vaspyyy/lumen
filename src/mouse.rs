//! Mouse reports for programs that ask for the mouse (vim, htop, tmux).

use alacritty_terminal::term::TermMode;
use gpui::{Modifiers, MouseButton};

pub const WHEEL_UP: u8 = 64;
pub const WHEEL_DOWN: u8 = 65;
/// Added to the button of a report that is a move, not a press.
pub const MOTION: u8 = 32;
/// The "button" of a move with no button held.
pub const NO_BUTTON: u8 = 3;

pub fn button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

/// The report for a button (or wheel step, or move) at a cell, both
/// counted from 0.
pub fn report(button: u8, pressed: bool, col: usize, row: usize, modifiers: Modifiers, mode: TermMode) -> Vec<u8> {
    let mods = 4 * modifiers.shift as u8 + 8 * modifiers.alt as u8 + 16 * modifiers.control as u8;
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if pressed { 'M' } else { 'm' };
        return format!("\x1b[<{};{};{}{}", button + mods, col + 1, row + 1, end).into_bytes();
    }
    // the original encoding: one byte per number, and no way to say which button was released
    let button = if pressed { button } else { NO_BUTTON };
    let byte = |n: usize| (32 + n).min(255) as u8;
    vec![0x1b, b'[', b'M', 32 + button + mods, byte(col + 1), byte(row + 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_and_legacy() {
        let none = Modifiers::default();
        let sgr = TermMode::SGR_MOUSE;
        assert_eq!(report(0, true, 0, 0, none, sgr), b"\x1b[<0;1;1M");
        assert_eq!(report(2, false, 9, 4, none, sgr), b"\x1b[<2;10;5m");
        assert_eq!(report(WHEEL_UP, true, 2, 3, Modifiers { control: true, ..none }, sgr), b"\x1b[<80;3;4M");
        assert_eq!(report(0, true, 0, 0, none, TermMode::NONE), b"\x1b[M !!");
        assert_eq!(report(0, false, 0, 0, none, TermMode::NONE), b"\x1b[M#!!");
    }
}
