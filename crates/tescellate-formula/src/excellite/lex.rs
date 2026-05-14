//! Lexer for the Excel-lite engine.
//!
//! Cell references are recognised as `[A-Z]+[0-9]+` (square-lattice only for
//! Phase 1). The parser is responsible for distinguishing a bare identifier
//! used as a function name from one used as a reference.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Str(String),
    /// Uppercase identifier — could be a function name (`SUM`), a boolean
    /// literal (`TRUE`/`FALSE`), or — when followed by digits at the same
    /// position — the letter half of a cell address. The lexer emits the
    /// full `A1` form as `CellRef` directly; bare `SUM` stays as `Ident`.
    Ident(String),
    CellRef(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Amp,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub start: usize,
    pub end: usize,
}

impl<T> Spanned<T> {
    pub fn new(value: T, start: usize, end: usize) -> Self {
        Self { value, start, end }
    }
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub pos: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lex error at byte {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for LexError {}

pub fn lex(src: &str) -> Result<Vec<Spanned<Token>>, LexError> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let (tok, next) = match b {
            b'(' => (Token::LParen, i + 1),
            b')' => (Token::RParen, i + 1),
            b'[' => (Token::LBracket, i + 1),
            b']' => (Token::RBracket, i + 1),
            b',' => (Token::Comma, i + 1),
            b':' => (Token::Colon, i + 1),
            b'+' => (Token::Plus, i + 1),
            b'-' => (Token::Minus, i + 1),
            b'*' => (Token::Star, i + 1),
            b'/' => (Token::Slash, i + 1),
            b'^' => (Token::Caret, i + 1),
            b'&' => (Token::Amp, i + 1),
            b'=' => (Token::Eq, i + 1),
            b'<' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    (Token::LtEq, i + 2)
                } else if bytes.get(i + 1) == Some(&b'>') {
                    (Token::NotEq, i + 2)
                } else {
                    (Token::Lt, i + 1)
                }
            }
            b'>' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    (Token::GtEq, i + 2)
                } else {
                    (Token::Gt, i + 1)
                }
            }
            b'"' => lex_string(bytes, i)?,
            b'0'..=b'9' | b'.' => lex_number(bytes, i)?,
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => lex_ident_or_ref(bytes, i),
            _ => {
                return Err(LexError {
                    message: format!("unexpected byte {b:?}"),
                    pos: i,
                });
            }
        };
        i = next;
        out.push(Spanned::new(tok, start, i));
    }
    Ok(out)
}

fn lex_string(bytes: &[u8], start: usize) -> Result<(Token, usize), LexError> {
    debug_assert_eq!(bytes[start], b'"');
    let mut s = String::new();
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            // Excel-style escaped quote: "" inside a string is a literal ".
            if bytes.get(i + 1) == Some(&b'"') {
                s.push('"');
                i += 2;
                continue;
            }
            return Ok((Token::Str(s), i + 1));
        }
        s.push(bytes[i] as char);
        i += 1;
    }
    Err(LexError {
        message: "unterminated string".into(),
        pos: start,
    })
}

fn lex_number(bytes: &[u8], start: usize) -> Result<(Token, usize), LexError> {
    let mut i = start;
    let mut saw_dot = false;
    let mut saw_exp = false;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'0'..=b'9' => i += 1,
            b'.' if !saw_dot && !saw_exp => {
                saw_dot = true;
                i += 1;
            }
            b'e' | b'E' if !saw_exp => {
                saw_exp = true;
                i += 1;
                if matches!(bytes.get(i), Some(b'+') | Some(b'-')) {
                    i += 1;
                }
            }
            _ => break,
        }
    }
    let text = std::str::from_utf8(&bytes[start..i]).unwrap();
    let value: f64 = text.parse().map_err(|_| LexError {
        message: format!("bad number `{text}`"),
        pos: start,
    })?;
    Ok((Token::Number(value), i))
}

fn lex_ident_or_ref(bytes: &[u8], start: usize) -> (Token, usize) {
    // Consume the letter run (case-insensitive).
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    let letters_end = i;
    // If followed immediately by digits, it's a cell ref (square A1 form).
    if letters_end > start
        && i < bytes.len()
        && bytes[i].is_ascii_digit()
        && bytes[start..letters_end]
            .iter()
            .all(u8::is_ascii_alphabetic)
    {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let raw = std::str::from_utf8(&bytes[start..i]).unwrap();
        // Cell refs are uppercased to keep address resolution stable.
        return (Token::CellRef(raw.to_ascii_uppercase()), i);
    }
    // Bare identifier — function name or boolean literal. Allow trailing
    // underscore/digit characters to support names like `LOG10`.
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    let raw = std::str::from_utf8(&bytes[start..i]).unwrap();
    (Token::Ident(raw.to_ascii_uppercase()), i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Token> {
        lex(src).unwrap().into_iter().map(|s| s.value).collect()
    }

    #[test]
    fn basic_arithmetic() {
        assert_eq!(
            toks("1 + 2 * 3"),
            vec![
                Token::Number(1.0),
                Token::Plus,
                Token::Number(2.0),
                Token::Star,
                Token::Number(3.0)
            ]
        );
    }

    #[test]
    fn cell_refs_and_ranges() {
        assert_eq!(
            toks("SUM(A1:B10)"),
            vec![
                Token::Ident("SUM".into()),
                Token::LParen,
                Token::CellRef("A1".into()),
                Token::Colon,
                Token::CellRef("B10".into()),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn comparison_operators() {
        assert_eq!(
            toks("A1<=B1<>C1>=D1"),
            vec![
                Token::CellRef("A1".into()),
                Token::LtEq,
                Token::CellRef("B1".into()),
                Token::NotEq,
                Token::CellRef("C1".into()),
                Token::GtEq,
                Token::CellRef("D1".into()),
            ]
        );
    }

    #[test]
    fn strings_with_escaped_quotes() {
        assert_eq!(
            toks(r#""he said ""hi""""#),
            vec![Token::Str(r#"he said "hi""#.into())]
        );
    }

    #[test]
    fn lowercase_refs_normalized() {
        assert_eq!(toks("a1"), vec![Token::CellRef("A1".into())]);
    }

    #[test]
    fn scientific_number() {
        assert_eq!(toks("1.5e-3"), vec![Token::Number(0.0015)]);
    }
}
