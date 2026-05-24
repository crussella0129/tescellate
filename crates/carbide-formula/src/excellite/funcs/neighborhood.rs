//! Lattice-aware neighborhood functions: NEIGHBORS, RADIUS, DISK,
//! DISTANCE.
//!
//! These functions are special-form in the sense that their first argument
//! is treated as a *cell address*, not an evaluated value. The Carbide
//! parser already produces `Expr::CellRef("A1")` for unquoted addresses;
//! we pull the address out before delegating to `EvalCtx::neighbors_of` /
//! `cells_within`. Pass a quoted string (`NEIGHBORS("A1")`) to bypass the
//! parser's address-recognition rules — useful when constructing the
//! address dynamically.
//!
//! Returns are 1D `Array`s of cell values in the lattice's canonical
//! neighbor order. Square: 4 cells (N, E, S, W). Hex: 6 cells. Aggregate
//! over the result with SUM / COUNT / MAP / etc.

use super::coerce::{arity_n, arity_range, to_int};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use carbide_core::{Array, CellValue};

/// Resolve the first argument of NEIGHBORS / RADIUS to an address string.
/// Accepts `Expr::CellRef("A1")` directly, or any expression that
/// evaluates to a Text value containing an address.
fn resolve_address(arg: &Expr, ctx: &dyn EvalCtx) -> Result<String, EvalError> {
    match arg {
        Expr::CellRef(addr) => Ok(addr.clone()),
        other => match eval(other, ctx)? {
            CellValue::Text(s) => Ok(s),
            v => Err(EvalError::Value(format!(
                "expected an address or text, got {v:?}"
            ))),
        },
    }
}

/// NEIGHBORS(cell) — edge-neighbors of `cell` as a 1D array.
/// Square: 4 cells. Hex: 6 cells. Order is lattice-specific but
/// deterministic across calls.
pub fn neighbors(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("NEIGHBORS", args, 1)?;
    let addr = resolve_address(&args[0], ctx)?;
    let vals = ctx.neighbors_of(&addr)?;
    Ok(CellValue::Array(Box::new(Array::col(vals))))
}

/// RADIUS(cell, n) — every cell within `n` edge-steps of `cell`,
/// inclusive of `cell` itself. Returns a 1D array.
///
/// Square: an (2n+1)×(2n+1) square box. Hex: a closed hex disc of
/// `1 + 3n(n+1)` cells.
pub fn radius(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("RADIUS", args, 1, 2)?;
    let addr = resolve_address(&args[0], ctx)?;
    let n = if let Some(arg) = args.get(1) {
        to_int(&eval(arg, ctx)?)?
    } else {
        1
    };
    if n < 0 {
        return Err(EvalError::Value("RADIUS: radius must be >= 0".into()));
    }
    let vals = ctx.cells_within(&addr, n)?;
    Ok(CellValue::Array(Box::new(Array::col(vals))))
}

/// DISTANCE(cell_a, cell_b) — lattice-native distance between two
/// addresses. Square: Chebyshev (king-move). Hex: hex edge-step.
/// Triangle: half-base / row-step Chebyshev (a first cut; the true
/// triangle edge-step metric depends on orientations along the path
/// and lands in a follow-up). Returns an `Integer`.
pub fn distance(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("DISTANCE", args, 2)?;
    let a = resolve_address(&args[0], ctx)?;
    let b = resolve_address(&args[1], ctx)?;
    Ok(CellValue::Integer(ctx.lattice_distance(&a, &b)?))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("NEIGHBORS", neighbors);
    r.add("RADIUS", radius);
    // DISK is the launch-brief's preferred name for RADIUS — same
    // function, both names registered so docs / muscle memory agree.
    r.add("DISK", radius);
    r.add("DISTANCE", distance);
}
