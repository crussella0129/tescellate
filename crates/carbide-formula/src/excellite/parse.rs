//! Pratt parser for Excel-lite. Produces an `Expr` tree.
//!
//! Precedence (low → high) follows Excel:
//!   1. comparison: =  <>  <  >  <=  >=
//!   2. concatenation: &
//!   3. additive: + -
//!   4. multiplicative: * /
//!   5. exponentiation: ^   (right-associative)
//!   6. unary: -  +
//!   7. primary: literal | ref | range | call | (expr)

use super::ast::{BinaryOp, Expr, UnaryOp};
use super::lex::{lex, LexError, Spanned, Token};
use std::fmt;

/// Hard cap on formula nesting depth. A formula nested deeper than this is
/// rejected at parse time — far beyond any hand-written formula (Excel's
/// classic limit was 64), and low enough that the recursive tree-walks
/// over the resulting `Expr` (`eval`, reference collection, the
/// transpiler, even `Expr`'s own recursive `Drop`) cannot overflow the
/// stack. Without this cap a pathologically deep formula crashes the
/// process instead of producing a `#VALUE!`-style parse error.
const MAX_EXPR_DEPTH: u32 = 128;

/// If `name(args)` shapes match a lattice-address syntax, return the
/// canonical address string. Otherwise return `None` and let the parser
/// keep it as a function call.
///
/// Today: `H(int, int)` → `H(q,r)` for hex; `T(int, int)` → `T(col,row)`
/// for triangle. Hooks for parallelogram and other lattices land in
/// their respective phases.
fn recognise_lattice_address(name: &str, args: &[Expr]) -> Option<String> {
    if name == "H" && args.len() == 2 {
        let q = literal_int(&args[0])?;
        let r = literal_int(&args[1])?;
        return Some(format!("H({q},{r})"));
    }
    if name == "T" && args.len() == 2 {
        let col = literal_int(&args[0])?;
        let row = literal_int(&args[1])?;
        return Some(format!("T({col},{row})"));
    }
    if name == "V" && args.len() == 1 {
        // Voronoi seed index `V(n)` — a single non-negative integer.
        let n = literal_int(&args[0])?;
        if n < 0 {
            return None;
        }
        return Some(format!("V({n})"));
    }
    None
}

/// Extract a signed integer literal from an `Expr`, including the unary
/// `-` and `+` wrappings the parser leaves around negative literals.
fn literal_int(e: &Expr) -> Option<i64> {
    match e {
        Expr::Number(n) if n.fract() == 0.0 && n.is_finite() => Some(*n as i64),
        Expr::Unary(UnaryOp::Neg, inner) => literal_int(inner).map(|n| -n),
        Expr::Unary(UnaryOp::Pos, inner) => literal_int(inner),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub pos: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at byte {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            message: e.message,
            pos: e.pos,
        }
    }
}

pub fn parse(src: &str) -> Result<Expr, ParseError> {
    let tokens = lex(src)?;
    let mut p = Parser {
        tokens,
        i: 0,
        depth: 0,
    };
    let expr = p.parse_expr(0)?;
    if p.i < p.tokens.len() {
        let tok = &p.tokens[p.i];
        return Err(ParseError {
            message: format!("unexpected token after expression: {:?}", tok.value),
            pos: tok.start,
        });
    }
    Ok(expr)
}

struct Parser {
    tokens: Vec<Spanned<Token>>,
    i: usize,
    /// Current `parse_expr` nesting. Bounded by `MAX_EXPR_DEPTH` so a
    /// pathologically deep formula is rejected here rather than crashing a
    /// later recursive walk over the `Expr` tree.
    depth: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Assoc {
    Left,
    Right,
}

fn binding_power(tok: &Token) -> Option<(u8, Assoc, BinaryOp)> {
    use BinaryOp::*;
    Some(match tok {
        Token::Eq => (1, Assoc::Left, Eq),
        Token::NotEq => (1, Assoc::Left, NotEq),
        Token::Lt => (1, Assoc::Left, Lt),
        Token::Gt => (1, Assoc::Left, Gt),
        Token::LtEq => (1, Assoc::Left, LtEq),
        Token::GtEq => (1, Assoc::Left, GtEq),
        Token::Amp => (2, Assoc::Left, Concat),
        Token::Plus => (3, Assoc::Left, Add),
        Token::Minus => (3, Assoc::Left, Sub),
        Token::Star => (4, Assoc::Left, Mul),
        Token::Slash => (4, Assoc::Left, Div),
        Token::Caret => (5, Assoc::Right, Pow),
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> Option<&Spanned<Token>> {
        self.tokens.get(self.i)
    }

    fn bump(&mut self) -> &Spanned<Token> {
        let t = &self.tokens[self.i];
        self.i += 1;
        t
    }

    /// Reject a formula that has nested past `MAX_EXPR_DEPTH`.
    fn check_depth(&self) -> Result<(), ParseError> {
        if self.depth > MAX_EXPR_DEPTH {
            Err(ParseError {
                message: format!("formula nesting is too deep (limit is {MAX_EXPR_DEPTH} levels)"),
                pos: self.peek().map(|t| t.start).unwrap_or(0),
            })
        } else {
            Ok(())
        }
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        // Track nesting depth so an absurdly deep formula is rejected here
        // rather than overflowing the stack in a later recursive walk
        // (`eval`, reference collection, the transpiler, `Expr`'s drop).
        // The operator and apply-suffix loops grow a left-deep chain, so
        // each iteration counts as a level too. `entry_depth` is restored
        // on the way out, so a sibling subtree starts from a clean slate.
        let entry_depth = self.depth;
        self.depth += 1;
        self.check_depth()?;

        let mut lhs = self.parse_prefix()?;

        // Range parsing: `<cellref>:<cellref>`. Works for any primary that
        // resolved to `Expr::CellRef` — square (`A1`), hex (`H(q,r)`),
        // and future address forms. Has to run before the apply-suffix
        // loop so that `H(0,0):H(3,3)` doesn't get mis-parsed as a call.
        if matches!(&lhs, Expr::CellRef(_))
            && matches!(self.peek().map(|t| &t.value), Some(Token::Colon))
        {
            self.bump(); // :
            let rhs = self.parse_prefix()?;
            let (start, end) = match (lhs, rhs) {
                (Expr::CellRef(a), Expr::CellRef(b)) => (a, b),
                (a, b) => {
                    return Err(ParseError {
                        message: format!("expected cell refs around `:`, got {a:?} : {b:?}"),
                        pos: 0,
                    });
                }
            };
            lhs = Expr::Range(start, end);
        }

        // Apply-suffix loop: any expression followed by `(...)` is a call.
        // This is what lets `Y(Fact)(5)` and `f(7)` (with `f` LET-bound)
        // parse — the postfix is left-associative, so each `(args)` wraps
        // the running lhs in another `Apply`.
        while matches!(self.peek().map(|t| &t.value), Some(Token::LParen)) {
            self.depth += 1;
            self.check_depth()?;
            self.bump(); // (
            let args = self.parse_call_args()?;
            self.expect(Token::RParen, "expected `)` to close call")?;
            lhs = Expr::Apply(Box::new(lhs), args);
        }

        while let Some(tok) = self.peek() {
            let Some((bp, assoc, op)) = binding_power(&tok.value) else {
                break;
            };
            if bp < min_bp {
                break;
            }
            // Each operator extends the left-deep chain by one level.
            self.depth += 1;
            self.check_depth()?;
            self.bump();
            let next_min = match assoc {
                Assoc::Left => bp + 1,
                Assoc::Right => bp,
            };
            let rhs = self.parse_expr(next_min)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        self.depth = entry_depth;
        Ok(lhs)
    }

    /// Parse a comma-separated argument list. Consumes commas; the caller
    /// is responsible for the surrounding `(` and `)`. Used by both the
    /// `Ident(...)` Call path and the apply-suffix path so both forms
    /// allow the same argument syntax.
    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if matches!(self.peek().map(|t| &t.value), Some(Token::RParen)) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr(0)?);
            match self.peek().map(|t| t.value.clone()) {
                Some(Token::Comma) => {
                    self.bump();
                }
                Some(Token::RParen) => break,
                other => {
                    return Err(ParseError {
                        message: format!("expected `,` or `)` in argument list, got {other:?}"),
                        pos: self.peek().map(|t| t.start).unwrap_or(0),
                    });
                }
            }
        }
        Ok(args)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        let tok = self.peek().cloned().ok_or_else(|| ParseError {
            message: "unexpected end of input".into(),
            pos: 0,
        })?;
        match tok.value {
            Token::Number(n) => {
                self.bump();
                Ok(Expr::Number(n))
            }
            Token::Str(s) => {
                self.bump();
                Ok(Expr::Str(s))
            }
            Token::Plus => {
                self.bump();
                let rhs = self.parse_expr(6)?;
                Ok(Expr::Unary(UnaryOp::Pos, Box::new(rhs)))
            }
            Token::Minus => {
                self.bump();
                let rhs = self.parse_expr(6)?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(rhs)))
            }
            Token::LParen => {
                self.bump();
                let inner = self.parse_expr(0)?;
                self.expect(Token::RParen, "expected `)`")?;
                Ok(inner)
            }
            Token::LBracket => {
                self.bump();
                self.parse_array_literal(tok.start)
            }
            Token::Ident(ref name) => {
                if name == "TRUE" {
                    self.bump();
                    return Ok(Expr::Bool(true));
                }
                if name == "FALSE" {
                    self.bump();
                    return Ok(Expr::Bool(false));
                }
                let name = name.clone();
                self.bump();
                // If followed by `(`, this is a function call (or a user
                // lambda apply via the eval-time Call→Var fallback). If
                // not, it's a bare identifier — emit `Var` and let the
                // evaluator resolve it through the lexical environment.
                if matches!(self.peek().map(|t| &t.value), Some(Token::LParen)) {
                    self.bump(); // (
                    let args = self.parse_call_args()?;
                    self.expect(Token::RParen, "expected `)` to close call")?;
                    // Lattice-address recognition: certain `Ident(args)`
                    // shapes are addresses, not calls. Today that's just
                    // `H(q, r)` (axial hex). Future lattices (T(x,y,▲),
                    // O(i,j), V<hash>) extend this seam. See
                    // `docs/carbide/addressing.md`.
                    if let Some(canonical) = recognise_lattice_address(&name, &args) {
                        Ok(Expr::CellRef(canonical))
                    } else {
                        Ok(Expr::Call(name, args))
                    }
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Token::CellRef(addr) => {
                self.bump();
                Ok(Expr::CellRef(addr))
            }
            other => Err(ParseError {
                message: format!("unexpected token {other:?}"),
                pos: tok.start,
            }),
        }
    }

    /// Parse the body of an array literal: contents between `[` and `]`.
    /// `[` is already consumed by the caller; `start_pos` is its byte offset
    /// for error reporting. Decides 1D vs 2D by inspecting the first element:
    /// if it is itself a 1-row `Expr::Array`, the outer is 2D and all
    /// siblings must also be 1-row arrays of the same width.
    fn parse_array_literal(&mut self, start_pos: usize) -> Result<Expr, ParseError> {
        // Empty `[]` — 0×0 array literal.
        if matches!(self.peek().map(|t| &t.value), Some(Token::RBracket)) {
            self.bump();
            return Ok(Expr::Array(vec![]));
        }
        let first = self.parse_expr(0)?;
        // 2D mode if the first element is a 1-row array literal.
        let two_dim = matches!(&first, Expr::Array(rows) if rows.len() == 1);
        if two_dim {
            let first_row = match first {
                Expr::Array(rows) => rows.into_iter().next().unwrap(),
                _ => unreachable!(),
            };
            let cols = first_row.len();
            let mut rows = vec![first_row];
            while matches!(self.peek().map(|t| &t.value), Some(Token::Comma)) {
                self.bump();
                let next = self.parse_expr(0)?;
                let row = match next {
                    Expr::Array(r) if r.len() == 1 => r.into_iter().next().unwrap(),
                    other => {
                        return Err(ParseError {
                            message: format!("2D array: expected another `[…]` row, got {other:?}"),
                            pos: start_pos,
                        });
                    }
                };
                if row.len() != cols {
                    return Err(ParseError {
                        message: format!(
                            "ragged 2D array: row has {} elements, expected {cols}",
                            row.len()
                        ),
                        pos: start_pos,
                    });
                }
                rows.push(row);
            }
            self.expect(Token::RBracket, "expected `]` to close array")?;
            Ok(Expr::Array(rows))
        } else {
            let mut row = vec![first];
            while matches!(self.peek().map(|t| &t.value), Some(Token::Comma)) {
                self.bump();
                row.push(self.parse_expr(0)?);
            }
            self.expect(Token::RBracket, "expected `]` to close array")?;
            Ok(Expr::Array(vec![row]))
        }
    }

    fn expect(&mut self, want: Token, msg: &str) -> Result<(), ParseError> {
        let tok = self.peek().ok_or_else(|| ParseError {
            message: format!("{msg}: end of input"),
            pos: self.tokens.last().map(|t| t.end).unwrap_or(0),
        })?;
        if std::mem::discriminant(&tok.value) == std::mem::discriminant(&want) {
            self.bump();
            Ok(())
        } else {
            Err(ParseError {
                message: format!("{msg}: got {:?}", tok.value),
                pos: tok.start,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ast::{BinaryOp, Expr, UnaryOp};
    use super::parse;

    fn p(src: &str) -> Expr {
        parse(src).expect("parse")
    }

    #[test]
    fn precedence_plus_times() {
        let e = p("1 + 2 * 3");
        // 1 + (2 * 3)
        assert_eq!(
            e,
            Expr::Binary(
                BinaryOp::Add,
                Box::new(Expr::Number(1.0)),
                Box::new(Expr::Binary(
                    BinaryOp::Mul,
                    Box::new(Expr::Number(2.0)),
                    Box::new(Expr::Number(3.0)),
                )),
            )
        );
    }

    #[test]
    fn pow_right_associative() {
        let e = p("2 ^ 3 ^ 2");
        // 2 ^ (3 ^ 2)
        let inner = Expr::Binary(
            BinaryOp::Pow,
            Box::new(Expr::Number(3.0)),
            Box::new(Expr::Number(2.0)),
        );
        assert_eq!(
            e,
            Expr::Binary(BinaryOp::Pow, Box::new(Expr::Number(2.0)), Box::new(inner))
        );
    }

    #[test]
    fn unary_neg_binds_tight() {
        let e = p("-2^2");
        // Note: Excel actually parses this as (-2)^2 = 4 because unary `-`
        // is HIGHER precedence than `^`. We honour that — see the precedence
        // doc comment at the top of this file.
        assert_eq!(
            e,
            Expr::Binary(
                BinaryOp::Pow,
                Box::new(Expr::Unary(UnaryOp::Neg, Box::new(Expr::Number(2.0)))),
                Box::new(Expr::Number(2.0)),
            )
        );
    }

    #[test]
    fn function_call_with_range() {
        let e = p("SUM(A1:B5, 7)");
        assert_eq!(
            e,
            Expr::Call(
                "SUM".into(),
                vec![Expr::Range("A1".into(), "B5".into()), Expr::Number(7.0),]
            )
        );
    }

    #[test]
    fn comparison_chains_left() {
        let e = p("1 < 2 = 3");
        // (1 < 2) = 3 — same-precedence left-associative.
        assert_eq!(
            e,
            Expr::Binary(
                BinaryOp::Eq,
                Box::new(Expr::Binary(
                    BinaryOp::Lt,
                    Box::new(Expr::Number(1.0)),
                    Box::new(Expr::Number(2.0)),
                )),
                Box::new(Expr::Number(3.0)),
            )
        );
    }

    #[test]
    fn nested_if() {
        let e = p(r#"IF(A1>0, "pos", IF(A1=0, "zero", "neg"))"#);
        if let Expr::Call(name, args) = e {
            assert_eq!(name, "IF");
            assert_eq!(args.len(), 3);
        } else {
            panic!("expected outer IF call");
        }
    }

    #[test]
    fn bool_literals() {
        assert_eq!(p("TRUE"), Expr::Bool(true));
        assert_eq!(p("FALSE"), Expr::Bool(false));
    }

    #[test]
    fn trailing_garbage_is_error() {
        assert!(parse("1+2 3").is_err());
    }

    #[test]
    fn array_literal_1d() {
        let e = p("[1, 2, 3]");
        assert_eq!(
            e,
            Expr::Array(vec![vec![
                Expr::Number(1.0),
                Expr::Number(2.0),
                Expr::Number(3.0)
            ]])
        );
    }

    #[test]
    fn array_literal_2d() {
        let e = p("[[1, 2], [3, 4]]");
        assert_eq!(
            e,
            Expr::Array(vec![
                vec![Expr::Number(1.0), Expr::Number(2.0)],
                vec![Expr::Number(3.0), Expr::Number(4.0)],
            ])
        );
    }

    #[test]
    fn array_of_cell_refs() {
        let e = p("[A1, B3, C5]");
        assert_eq!(
            e,
            Expr::Array(vec![vec![
                Expr::CellRef("A1".into()),
                Expr::CellRef("B3".into()),
                Expr::CellRef("C5".into()),
            ]])
        );
    }

    #[test]
    fn array_can_hold_expressions() {
        let e = p("[1+2, SUM(A1:A3), \"hi\"]");
        if let Expr::Array(rows) = e {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].len(), 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn ragged_2d_is_error() {
        assert!(parse("[[1, 2], [3, 4, 5]]").is_err());
    }

    #[test]
    fn empty_array_ok() {
        assert_eq!(p("[]"), Expr::Array(vec![]));
    }

    #[test]
    fn deep_nesting_is_rejected_not_crashed() {
        // A formula nested far past `MAX_EXPR_DEPTH` is a parse error — the
        // engine would otherwise overflow the stack walking the tree.
        let mut deep = String::from("1");
        for _ in 0..5000 {
            deep.push_str("+1");
        }
        assert!(
            parse(&deep).is_err(),
            "a 5000-deep formula must be rejected"
        );

        // A formula comfortably within the limit still parses.
        let mut shallow = String::from("1");
        for _ in 0..50 {
            shallow.push_str("+1");
        }
        assert!(
            parse(&shallow).is_ok(),
            "a 50-deep formula must still parse"
        );
    }
}
