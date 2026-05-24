//! Sheet extent — bounded vs unbounded. See PLAN.md §4.
//!
//! Storage is sparse either way (only populated cells live in
//! `Sheet.cells`). Extent governs validation: `Bounded` rejects writes
//! outside its region; `Unbounded` accepts any syntactically valid address.

use serde::{Deserialize, Serialize};
use carbide_tess::ParsedCoord;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
pub enum SheetExtent {
    #[default]
    Unbounded,
    Bounded(BoundedExtent),
}

/// Per-lattice bounded-extent specification. Each variant says "cells exist
/// in this region, nothing outside". Future lattices add their own variants;
/// the validation logic is owned by the lattice implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "lattice", content = "params", rename_all = "snake_case")]
pub enum BoundedExtent {
    /// Square grid: cells span (0..cols) × (0..rows).
    Square { cols: u32, rows: u32 },
    /// Hex axial-aligned parallelogram region (Phase 2+).
    HexAxial {
        q_min: i32,
        q_max: i32,
        r_min: i32,
        r_max: i32,
    },
    /// Hex disc of radius N around (center_q, center_r) (Phase 2+).
    HexRadius {
        center_q: i32,
        center_r: i32,
        radius: u32,
    },
    /// Triangle region of side N (Phase 3+).
    Triangle { side: u32 },
    /// Parallelogram region (Phase 3+).
    Parallelogram { u: u32, v: u32 },
}

impl SheetExtent {
    /// True if this address falls within the sheet. For Unbounded any address
    /// the lattice accepts is in-bounds; the bound check is up to the lattice.
    pub fn contains_square(&self, col: i32, row: i32) -> bool {
        match self {
            SheetExtent::Unbounded => true,
            SheetExtent::Bounded(BoundedExtent::Square { cols, rows }) => {
                col >= 0 && row >= 0 && (col as u32) < *cols && (row as u32) < *rows
            }
            // A square address against a non-square bounded extent is meaningless;
            // accept (the lattice will have already rejected the address).
            SheetExtent::Bounded(_) => true,
        }
    }

    /// True if a hex address `(q, r)` falls within the sheet. Mirrors
    /// `contains_square` but for `BoundedExtent::HexAxial` / `HexRadius`.
    pub fn contains_hex(&self, q: i32, r: i32) -> bool {
        match self {
            SheetExtent::Unbounded => true,
            SheetExtent::Bounded(BoundedExtent::HexAxial {
                q_min,
                q_max,
                r_min,
                r_max,
            }) => q >= *q_min && q <= *q_max && r >= *r_min && r <= *r_max,
            SheetExtent::Bounded(BoundedExtent::HexRadius {
                center_q,
                center_r,
                radius,
            }) => {
                // Cube-distance: max of the three axis deltas, where s = -q - r.
                let dq = (q - *center_q).abs();
                let dr = (r - *center_r).abs();
                let ds = ((-q - r) - (-*center_q - *center_r)).abs();
                dq.max(dr).max(ds) as u32 <= *radius
            }
            // Hex address against a non-hex bound: accept (analogous to above).
            SheetExtent::Bounded(_) => true,
        }
    }

    /// True if a triangle address `(col, row)` falls within the sheet.
    /// Bounded extent uses a `side` square-shaped region centred on
    /// `(0, 0)` — half-base columns × row rows — same shape as the hex
    /// disc but on triangle coords. Other bounded variants accept (the
    /// lattice has already validated the address).
    pub fn contains_triangle(&self, col: i32, row: i32) -> bool {
        match self {
            SheetExtent::Unbounded => true,
            SheetExtent::Bounded(BoundedExtent::Triangle { side }) => {
                let s = *side as i32;
                col >= -s && col <= s && row >= -s && row <= s
            }
            SheetExtent::Bounded(_) => true,
        }
    }

    /// Lattice-dispatched bounds check. Engine.rs uses this so it doesn't
    /// have to match on lattice kind at every call site.
    pub fn contains(&self, coord: ParsedCoord) -> bool {
        match coord {
            ParsedCoord::Square(c) => self.contains_square(c.col, c.row),
            ParsedCoord::Hex(c) => self.contains_hex(c.q, c.r),
            ParsedCoord::Triangle(c) => self.contains_triangle(c.col, c.row),
            // Voronoi has no row/col bounds — the lattice carries its
            // own seed list as the source of truth. Any V(N) that
            // parsed cleanly is by construction in range, so accept.
            ParsedCoord::Voronoi(_) => true,
        }
    }
}
