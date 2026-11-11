//! What a key press sends to the program in the terminal.
//!
//! Printable keys are not here: those arrive as text through the input
//! method, which is also what composes dead keys and non-Latin input.
//!
//! There are two encodings. The old one (xterm's) can't tell some keys
//! apart: Tab from Ctrl+I, Escape from the start of a sequence, Enter from
//! Shift+Enter. A program that wants to can switch on the kitty keyboard
//! protocol, where every key has a sequence of its own.

use alacritty_terminal::term::TermMode;
use gpui::{Keystroke, Modifiers};

/// xterm's modifier parameter: 1 + shift (1) + alt (2) + ctrl (4).
fn modifier_param(m: &Modifiers) -> u8 {
    1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.control as u8
}

/// What happened to a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Down,
    /// It is held, and repeats.
    Repeat,
    Up,
}

/// The bytes for `key`, or None when the key is text (or nothing a
/// terminal sends).
pub fn encode(key: &Keystroke, mode: TermMode, press: Press) -> Option<Vec<u8>> {
    if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) {
        return kitty(key, mode, press);
    }
    if press == Press::Up { None } else { legacy(key, mode) }
}

/// How the kitty keyboard protocol writes a key.
enum Form {
    /// `CSI 1 ; modifiers <letter>`: the cursor keys and F1 to F4.
    Letter(char),
    /// `CSI <number> ; modifiers ~`.
    Tilde(u32),
    /// `CSI <number> ; modifiers u`: the rest.
    Unicode(u32),
}

fn kitty(key: &Keystroke, mode: TermMode, press: Press) -> Option<Vec<u8>> {
    let m = &key.modifiers;
    // every key as an escape code, text keys too
    let all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    let events = mode.contains(TermMode::REPORT_EVENT_TYPES);
    let event = match press {
        Press::Down => 1,
        Press::Repeat if events => 2,
        Press::Repeat => 1,
        Press::Up if events => 3,
        Press::Up => return None,
    };
    let modifiers = 1 + m.shift as u32 + 2 * m.alt as u32 + 4 * m.control as u32 + 8 * m.platform as u32;
    let chord = m.alt || m.control || m.platform;

    let name = key.key.as_str();
    let mut text_key = false;
    let form = match name {
        "up" => Form::Letter('A'),
        "down" => Form::Letter('B'),
        "right" => Form::Letter('C'),
        "left" => Form::Letter('D'),
        "home" => Form::Letter('H'),
        "end" => Form::Letter('F'),
        "f1" => Form::Letter('P'),
        "f2" => Form::Letter('Q'),
        "f3" => Form::Tilde(13),
        "f4" => Form::Letter('S'),
        "insert" => Form::Tilde(2),
        "delete" => Form::Tilde(3),
        "pageup" => Form::Tilde(5),
        "pagedown" => Form::Tilde(6),
        "f5" => Form::Tilde(15),
        "f6" => Form::Tilde(17),
        "f7" => Form::Tilde(18),
        "f8" => Form::Tilde(19),
        "f9" => Form::Tilde(20),
        "f10" => Form::Tilde(21),
        "f11" => Form::Tilde(23),
        "f12" => Form::Tilde(24),
        "escape" => Form::Unicode(27),
        "enter" | "tab" | "backspace" => {
            // these three stay as they were unless a modifier is held, so that a
            // shell is still usable after a program left the protocol switched on
            if !all && modifiers == 1 {
                return if press == Press::Up { None } else { legacy(key, mode) };
            }
            Form::Unicode(match name {
                "enter" => 13,
                "tab" => 9,
                _ => 127,
            })
        },
        "space" => {
            text_key = true;
            Form::Unicode(32)
        },
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => {
                    text_key = true;
                    Form::Unicode(c as u32)
                },
                _ => match name.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
                    Some(n @ 13..=35) => Form::Unicode(57376 + n - 13),
                    _ => return None,
                },
            }
        },
    };
    // A key that types text is text, unless every key is wanted as an escape code.
    // Its release is still told to a program that asked for releases, as kitty does.
    if text_key && !all && !chord && press != Press::Up {
        return (name == "space").then(|| b" ".to_vec());
    }

    let typed = key.key_char.as_deref().filter(|t| !t.is_empty());
    let mut out = String::from("\x1b[");
    match form {
        Form::Letter(_) => {
            if modifiers != 1 || event != 1 {
                out.push('1');
            }
        },
        Form::Tilde(n) => out.push_str(&n.to_string()),
        Form::Unicode(code) => {
            out.push_str(&code.to_string());
            // what the key types with Shift held, for programs that ask for it
            if mode.contains(TermMode::REPORT_ALTERNATE_KEYS)
                && m.shift
                && let Some(shifted) = typed.and_then(|t| t.chars().next())
                && shifted as u32 != code
            {
                out.push_str(&format!(":{}", shifted as u32));
            }
        },
    }
    // the text the key types, as code points
    let text = (all && text_key && !chord && press != Press::Up && mode.contains(TermMode::REPORT_ASSOCIATED_TEXT))
        .then_some(typed.or(if name == "space" { Some(" ") } else { None }))
        .flatten()
        .map(|t| t.chars().map(|c| (c as u32).to_string()).collect::<Vec<_>>().join(":"));
    if modifiers != 1 || event != 1 || text.is_some() {
        out.push(';');
        if modifiers != 1 || event != 1 {
            out.push_str(&modifiers.to_string());
        }
        if event != 1 {
            out.push_str(&format!(":{event}"));
        }
    }
    if let Some(text) = text {
        out.push(';');
        out.push_str(&text);
    }
    out.push(match form {
        Form::Letter(letter) => letter,
        Form::Tilde(_) => '~',
        Form::Unicode(_) => 'u',
    });
    Some(out.into_bytes())
}

fn legacy(key: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
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
        encode(&key(s), mode, Press::Down).map(|b| String::from_utf8(b).unwrap())
    }

    /// What the kitty protocol sends with these flags on.
    fn kitty_sent(s: &'static str, flags: TermMode, press: Press) -> Option<String> {
        encode(&key(s), flags, press).map(|b| String::from_utf8(b).unwrap())
    }

    #[test]
    fn kitty_tells_keys_apart() {
        let on = TermMode::DISAMBIGUATE_ESC_CODES;
        let down = Press::Down;
        // text is still text, and the shell's three keys are as they were
        assert_eq!(kitty_sent("a", on, down), None);
        assert_eq!(kitty_sent("shift-a", on, down), None);
        assert_eq!(kitty_sent("space", on, down).as_deref(), Some(" "));
        assert_eq!(kitty_sent("enter", on, down).as_deref(), Some("\r"));
        assert_eq!(kitty_sent("tab", on, down).as_deref(), Some("\t"));
        assert_eq!(kitty_sent("backspace", on, down).as_deref(), Some("\x7f"));
        // what the old encoding can't say
        assert_eq!(kitty_sent("escape", on, down).as_deref(), Some("\x1b[27u"));
        assert_eq!(kitty_sent("shift-enter", on, down).as_deref(), Some("\x1b[13;2u"));
        assert_eq!(kitty_sent("ctrl-i", on, down).as_deref(), Some("\x1b[105;5u"));
        assert_eq!(kitty_sent("shift-tab", on, down).as_deref(), Some("\x1b[9;2u"));
        assert_eq!(kitty_sent("ctrl-c", on, down).as_deref(), Some("\x1b[99;5u"));
        assert_eq!(kitty_sent("ctrl-shift-c", on, down).as_deref(), Some("\x1b[99;6u"));
        assert_eq!(kitty_sent("alt-space", on, down).as_deref(), Some("\x1b[32;3u"));
        // the keys that had sequences keep their shape
        assert_eq!(kitty_sent("up", on, down).as_deref(), Some("\x1b[A"));
        assert_eq!(kitty_sent("ctrl-up", on, down).as_deref(), Some("\x1b[1;5A"));
        assert_eq!(kitty_sent("f1", on, down).as_deref(), Some("\x1b[P"));
        assert_eq!(kitty_sent("f3", on, down).as_deref(), Some("\x1b[13~"));
        assert_eq!(kitty_sent("shift-f5", on, down).as_deref(), Some("\x1b[15;2~"));
        assert_eq!(kitty_sent("delete", on, down).as_deref(), Some("\x1b[3~"));
        // a release is nothing a program asked for here
        assert_eq!(kitty_sent("escape", on, Press::Up), None);
    }

    #[test]
    fn kitty_reports_repeats_and_releases() {
        let on = TermMode::DISAMBIGUATE_ESC_CODES | TermMode::REPORT_EVENT_TYPES;
        assert_eq!(kitty_sent("escape", on, Press::Repeat).as_deref(), Some("\x1b[27;1:2u"));
        assert_eq!(kitty_sent("escape", on, Press::Up).as_deref(), Some("\x1b[27;1:3u"));
        assert_eq!(kitty_sent("ctrl-left", on, Press::Up).as_deref(), Some("\x1b[1;5:3D"));
        assert_eq!(kitty_sent("up", on, Press::Up).as_deref(), Some("\x1b[1;1:3A"));
        // a key that types text is typed as text, and its release is reported
        assert_eq!(kitty_sent("a", on, Press::Repeat), None);
        assert_eq!(kitty_sent("a", on, Press::Up).as_deref(), Some("\x1b[97;1:3u"));
        assert_eq!(kitty_sent("shift-a", on, Press::Up).as_deref(), Some("\x1b[97;2:3u"));
        assert_eq!(kitty_sent("space", on, Press::Up).as_deref(), Some("\x1b[32;1:3u"));
        // the shell's three don't report theirs, so that a shell stays usable
        assert_eq!(kitty_sent("enter", on, Press::Up), None);
        assert_eq!(kitty_sent("tab", on, Press::Up), None);
        assert_eq!(kitty_sent("backspace", on, Press::Up), None);
    }

    #[test]
    fn kitty_can_report_every_key() {
        let all = TermMode::DISAMBIGUATE_ESC_CODES | TermMode::REPORT_ALL_KEYS_AS_ESC;
        assert_eq!(kitty_sent("a", all, Press::Down).as_deref(), Some("\x1b[97u"));
        assert_eq!(kitty_sent("enter", all, Press::Down).as_deref(), Some("\x1b[13u"));
        assert_eq!(kitty_sent("space", all, Press::Down).as_deref(), Some("\x1b[32u"));
        let mut shifted = key("shift-a");
        shifted.key_char = Some("A".into());
        let text = all | TermMode::REPORT_ASSOCIATED_TEXT | TermMode::REPORT_ALTERNATE_KEYS;
        let sent = encode(&shifted, text, Press::Down).map(|b| String::from_utf8(b).unwrap());
        assert_eq!(sent.as_deref(), Some("\x1b[97:65;2;65u"));
        let mut plain = key("a");
        plain.key_char = Some("a".into());
        let sent = encode(&plain, text, Press::Down).map(|b| String::from_utf8(b).unwrap());
        assert_eq!(sent.as_deref(), Some("\x1b[97;;97u"));
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
