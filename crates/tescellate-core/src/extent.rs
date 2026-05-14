//! Sheet extent — bounded vs unbounded. See PLAN.md §4.
//!
//! Storage is sparse either way (only populated cells live in
//! `Sheet.cells`). Extent governs validation: `Bounded` rejects writes
//! outside its region; `Unbounded` accepts any syntactically valid address.

use serde::{Deserialize, Serialize};

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
            // Future lattices: their own contains_* helpers.
            SheetExtent::Bounded(_) => true,
        }
    }
}
