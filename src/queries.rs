//! Questions a program asks the terminal that alacritty_terminal's parser
//! leaves unanswered: which terminal this is (XTVERSION, `CSI > q`), and how
//! large a cell is in pixels (`CSI 16 t`). Tools that draw pictures ask both,
//! over SSH too, where the environment doesn't say which terminal they are in.
//!
//! The output is only looked at here: the parser still gets all of it, and
//! skips these as it did before.

/// A question this module answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// XTVERSION: the terminal's name and version.
    Version,
    /// The size of a cell in pixels.
    CellSize,
}

impl Query {
    /// The answer, for a cell of this size in pixels.
    pub fn answer(self, cell: (f32, f32)) -> String {
        match self {
            Query::Version => format!("\x1bP>|lumen {}\x1b\\", env!("CARGO_PKG_VERSION")),
            // whole pixels, as TIOCGWINSZ and `CSI 14 t` count them
            Query::CellSize => format!("\x1b[6;{};{}t", cell.1.round() as u32, cell.0.round() as u32),
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq)]
enum State {
    #[default]
    Text,
    /// After ESC.
    Esc,
    /// In a control sequence (`ESC [`), which may be one of the questions.
    Csi,
}

/// The longest control sequence that can still be one of the questions.
const LONGEST: usize = 8;

/// Finds the questions in output that arrives in arbitrary pieces, as the
/// parser reads them.
#[derive(Default)]
pub struct Scanner {
    state: State,
    /// The parameters and intermediates of the sequence so far.
    sequence: Vec<u8>,
}

impl Scanner {
    /// Calls `found` for each question that ends in `input`, with how many
    /// bytes of `input` come before and with it.
    pub fn feed(&mut self, input: &[u8], mut found: impl FnMut(usize, Query)) {
        let mut i = 0;
        while i < input.len() {
            match self.state {
                State::Text => match input[i..].iter().position(|&b| b == 0x1b) {
                    Some(at) => {
                        i += at + 1;
                        self.state = State::Esc;
                    },
                    None => i = input.len(),
                },
                State::Esc => {
                    if input[i] == b'[' {
                        i += 1;
                        self.sequence.clear();
                        self.state = State::Csi;
                    } else {
                        // some other escape sequence; this byte is looked at again, it may be the next ESC
                        self.state = State::Text;
                    }
                },
                State::Csi => {
                    let byte = input[i];
                    match byte {
                        // parameters and intermediates
                        0x20..=0x3f => {
                            self.sequence.push(byte);
                            i += 1;
                            if self.sequence.len() > LONGEST {
                                self.state = State::Text;
                            }
                        },
                        // the final byte
                        0x40..=0x7e => {
                            i += 1;
                            self.state = State::Text;
                            match (self.sequence.as_slice(), byte) {
                                (b">" | b">0", b'q') => found(i, Query::Version),
                                (b"16", b't') => found(i, Query::CellSize),
                                _ => {},
                            }
                        },
                        // ESC starts another sequence
                        0x1b => self.state = State::Text,
                        // CAN and SUB cancel it
                        0x18 | 0x1a => {
                            i += 1;
                            self.state = State::Text;
                        },
                        // the parser carries out other control characters, and DEL is
                        // nothing, in the middle of a sequence as anywhere: it goes on
                        0x00..=0x1f | 0x7f => i += 1,
                        _ => {
                            i += 1;
                            self.state = State::Text;
                        },
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(pieces: &[&[u8]]) -> Vec<Query> {
        let mut scanner = Scanner::default();
        let mut queries = Vec::new();
        for piece in pieces {
            scanner.feed(piece, |_, query| queries.push(query));
        }
        queries
    }

    #[test]
    fn where_each_question_ends() {
        let mut ends = Vec::new();
        Scanner::default().feed(b"ab\x1b[c\x1b[16tcd\x1b[>q", |end, query| ends.push((end, query)));
        assert_eq!(ends, [(10, Query::CellSize), (16, Query::Version)]);
    }

    #[test]
    fn control_characters_inside_a_question_are_carried_out_as_the_parser_does() {
        assert_eq!(found(&[b"\x1b[1\x076t"]), [Query::CellSize]);
        assert_eq!(found(&[b"\x1b[>\n\x7fq"]), [Query::Version]);
        // but CAN and SUB end it
        assert!(found(&[b"\x1b[1\x186t", b"\x1b[>\x1aq"]).is_empty());
    }

    #[test]
    fn the_questions_are_found() {
        assert_eq!(found(&[b"\x1b[>q"]), [Query::Version]);
        assert_eq!(found(&[b"\x1b[>0q"]), [Query::Version]);
        assert_eq!(found(&[b"\x1b[16t"]), [Query::CellSize]);
        // as image tools ask them: among other questions, with the device attributes last
        assert_eq!(
            found(&[b"\x1b[>q\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[16t\x1b[c"]),
            [Query::Version, Query::CellSize]
        );
    }

    #[test]
    fn however_the_output_is_cut_up() {
        assert_eq!(found(&[b"text\x1b", b"[", b">", b"q more"]), [Query::Version]);
        assert_eq!(found(&[b"\x1b[1", b"6t"]), [Query::CellSize]);
    }

    #[test]
    fn other_sequences_are_not_questions() {
        assert!(found(&[b"\x1b[14t\x1b[18t\x1b[>4;1m\x1b[2 q\x1b[c\x1b[>c\x1b[116t"]).is_empty());
        // a sequence cut off by another one
        assert_eq!(found(&[b"\x1b[>\x1b[>q"]), [Query::Version]);
        assert!(found(&[b"\x1b[>\x18q"]).is_empty());
    }

    #[test]
    fn the_answers() {
        assert_eq!(Query::CellSize.answer((9.6, 20.)), "\x1b[6;20;10t");
        assert!(Query::Version.answer((0., 0.)).starts_with("\x1bP>|lumen "));
    }
}
