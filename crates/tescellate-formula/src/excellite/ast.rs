//! Excel-lite AST. Lattice-agnostic: cell references and ranges carry the
//! address as a string and are resolved against the active sheet's lattice
//! at evaluation time. This keeps the parser independent of which tiling
//! the sheet uses.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Number(f64),
    Str(String),
    Bool(bool),
    /// A cell reference token as the user typed it (e.g. `"A1"`). The
    /// evaluator resolves it via the sheet's `Lattice::parse_address`.
    CellRef(String),
    /// A range from `start` to `end`, both lattice addresses.
    Range(String, String),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
}
