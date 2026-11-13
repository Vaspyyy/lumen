//! Words typed at a prompt that are lumen's, not the shell's: `/settings` and
//! Enter opens lumen's settings instead of going to the shell. Only these exact
//! words are taken, so a path like `/usr/bin/htop` still goes to the shell.

/// What a slash word asks lumen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slash {
    /// `lumen settings`, in a tab of its own.
    Settings,
}

/// The slash words lumen knows, with what each does: for the help, and for the list a
/// slash word is looked up in.
pub const WORDS: &[(&str, Slash, &str)] = &[("/settings", Slash::Settings, "change lumen's settings")];

/// What a line typed at a prompt asks lumen for, if it is one of lumen's words.
pub fn parse(typed: &str) -> Option<Slash> {
    let typed = typed.trim();
    WORDS.iter().find(|(word, ..)| *word == typed).map(|(_, slash, _)| *slash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lumens_own_words_are_taken() {
        assert_eq!(parse("/settings"), Some(Slash::Settings));
        assert_eq!(parse("  /settings "), Some(Slash::Settings));
        for line in ["/usr/bin/htop", "/settings now", "/Settings", "settings", "echo /settings", "/", ""] {
            assert_eq!(parse(line), None, "{line:?}");
        }
    }
}
