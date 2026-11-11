//! What a key press sends to the program in the terminal.
//!
//! Printable keys are not here: those arrive as text through the input
//! method, which is also what composes dead keys and non-Latin input.

use alacritty_terminal::term::TermMode;
use gpui::{Keystroke, Modifiers};

/// xterm's modifier parameter: 1 + shift (1) + alt (2) + ctrl (4).
fn modifier_param(m: &Modifiers) -> u8 {
    1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.control as u8
}

/// The bytes for `key`, or None when the key is text (or nothing a
/// terminal sends).
pub fn encode(key: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let m = &key.modifiers;
    if m.platform {
        return None;
    }
    let plain = !m.shift && !m.alt && !m.control;
    let param = modifier_param(m);
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let esc = |s: &str| Some(s.as_bytes().to_vec());

    // CSI 1 ; <mod> <letter>, or SS3/CSI <letter> without modifiers
    let cursor_key = |letter: char| {
        Some(if !plain {
            format!("\x1b[1;{param}{letter}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{letter}").into_bytes()
        } else {
            format!("\x1b[{letter}").into_bytes()
        })
    };
    // CSI <n> ~, or CSI <n> ; <mod> ~
    let tilde_key = |n: u8| Some(if plain { format!("\x1b[{n}~") } else { format!("\x1b[{n};{param}~") }.into_bytes());

    match key.key.as_str() {
        "enter" => match (m.shift, m.alt, m.control) {
            (false, false, false) | (false, false, true) => esc("\r"),
            // a line break that doesn't submit, where the program knows one
            _ => esc("\x1b\r"),
        },
        "tab" => match (m.shift, m.alt) {
            (true, _) => esc("\x1b[Z"),
            (false, true) => esc("\x1b\t"),
            _ => esc("\t"),
        },
        "backspace" => match (m.control, m.alt) {
            (true, false) => esc("\x08"),
            (false, true) => esc("\x1b\x7f"),
            (true, true) => esc("\x1b\x08"),
            _ => esc("\x7f"),
        },
        "escape" => esc(if m.alt { "\x1b\x1b" } else { "\x1b" }),
        "up" => cursor_key('A'),
        "down" => cursor_key('B'),
        "right" => cursor_key('C'),
        "left" => cursor_key('D'),
        "home" => cursor_key('H'),
        "end" => cursor_key('F'),
        "insert" => tilde_key(2),
        "delete" => tilde_key(3),
        "pageup" => tilde_key(5),
        "pagedown" => tilde_key(6),
        "f1" | "f2" | "f3" | "f4" => {
            let letter = (b'P' + key.key.as_bytes()[1] - b'1') as char;
            Some(if plain { format!("\x1bO{letter}") } else { format!("\x1b[1;{param}{letter}") }.into_bytes())
        },
        "f5" => tilde_key(15),
        "f6" => tilde_key(17),
        "f7" => tilde_key(18),
        "f8" => tilde_key(19),
        "f9" => tilde_key(20),
        "f10" => tilde_key(21),
        "f11" => tilde_key(23),
        "f12" => tilde_key(24),
        "space" if m.control => Some(if m.alt { vec![0x1b, 0] } else { vec![0] }),
        "space" if m.alt => esc("\x1b "),
        // the one printable key that arrives by name and not as text
        "space" => esc(" "),
        name => {
            let mut chars = name.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            if m.control {
                // ctrl turns a letter (and a few symbols) into a control code
                let code = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    '@' | '2' => 0,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '^' | '6' => 0x1e,
                    '_' | '-' | '7' | '/' => 0x1f,
                    '?' | '8' => 0x7f,
                    _ => return None,
                };
                return Some(if m.alt { vec![0x1b, code] } else { vec![code] });
            }
            if m.alt {
                // alt sends the key behind an escape: what shells bind as meta
                let text = key.key_char.as_deref().filter(|t| !t.is_empty()).unwrap_or(name);
                let text = if m.shift && text == name { name.to_uppercase() } else { text.to_string() };
                let mut bytes = vec![0x1b];
                bytes.extend_from_slice(text.as_bytes());
                return Some(bytes);
            }
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &'static str) -> Keystroke {
        Keystroke::parse(s).unwrap()
    }

    fn sent(s: &'static str, mode: TermMode) -> Option<String> {
        encode(&key(s), mode).map(|b| String::from_utf8(b).unwrap())
    }

    #[test]
    fn text_is_left_to_the_input_method() {
        assert_eq!(sent("a", TermMode::NONE), None);
        assert_eq!(sent("shift-a", TermMode::NONE), None);
        assert_eq!(sent("space", TermMode::NONE).as_deref(), Some(" "));
        assert_eq!(sent("shift-space", TermMode::NONE).as_deref(), Some(" "));
    }

    #[test]
    fn control_and_meta() {
        assert_eq!(sent("ctrl-c", TermMode::NONE).as_deref(), Some("\x03"));
        assert_eq!(sent("ctrl-space", TermMode::NONE).as_deref(), Some("\0"));
        assert_eq!(sent("alt-b", TermMode::NONE).as_deref(), Some("\x1bb"));
        assert_eq!(sent("alt-shift-b", TermMode::NONE).as_deref(), Some("\x1bB"));
        assert_eq!(sent("ctrl-alt-d", TermMode::NONE).as_deref(), Some("\x1b\x04"));
        assert_eq!(sent("alt-backspace", TermMode::NONE).as_deref(), Some("\x1b\x7f"));
    }

    #[test]
    fn cursor_keys_follow_the_mode_and_carry_modifiers() {
        assert_eq!(sent("up", TermMode::NONE).as_deref(), Some("\x1b[A"));
        assert_eq!(sent("up", TermMode::APP_CURSOR).as_deref(), Some("\x1bOA"));
        assert_eq!(sent("ctrl-right", TermMode::APP_CURSOR).as_deref(), Some("\x1b[1;5C"));
        assert_eq!(sent("shift-alt-left", TermMode::NONE).as_deref(), Some("\x1b[1;4D"));
        assert_eq!(sent("home", TermMode::NONE).as_deref(), Some("\x1b[H"));
    }

    #[test]
    fn editing_and_function_keys() {
        assert_eq!(sent("delete", TermMode::NONE).as_deref(), Some("\x1b[3~"));
        assert_eq!(sent("ctrl-delete", TermMode::NONE).as_deref(), Some("\x1b[3;5~"));
        assert_eq!(sent("pageup", TermMode::NONE).as_deref(), Some("\x1b[5~"));
        assert_eq!(sent("f1", TermMode::NONE).as_deref(), Some("\x1bOP"));
        assert_eq!(sent("f4", TermMode::NONE).as_deref(), Some("\x1bOS"));
        assert_eq!(sent("shift-f2", TermMode::NONE).as_deref(), Some("\x1b[1;2Q"));
        assert_eq!(sent("f5", TermMode::NONE).as_deref(), Some("\x1b[15~"));
        assert_eq!(sent("f12", TermMode::NONE).as_deref(), Some("\x1b[24~"));
        assert_eq!(sent("shift-tab", TermMode::NONE).as_deref(), Some("\x1b[Z"));
        assert_eq!(sent("shift-enter", TermMode::NONE).as_deref(), Some("\x1b\r"));
    }
}
