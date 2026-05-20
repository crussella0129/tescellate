//! Tessellation lattices for Tescellate.
//!
//! See `PLAN.md` §3 for the full design. This crate currently exposes only
//! the `Lattice` trait and the `LatticeKind` enum; concrete implementations
//! land in Phase 1 (square) and beyond.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;

pub mod hex;
pub mod square;
pub mod triangle;

use hex::{HexCoord, HexLattice};
use square::{SquareCoord, SquareLattice};
use triangle::{TriCoord, TriangleLattice};

/// String-keyed lattice dispatch. Wraps `SquareLattice` / `HexLattice` so
/// upstream code (the workbook engine, the formula stdlib) can talk to a
/// lattice without knowing its associated `Coord` type. Every method goes
/// through the address string at the seam — the same convention the
/// Carbide AST already uses for cell refs (`Expr::CellRef(String)`).
///
/// New variants get added here as Phase 3+ lattices land.
pub enum LatticeHandle {
    Square(SquareLattice),
    Hex(HexLattice),
    Triangle(TriangleLattice),
}

/// Lattice-specific parsed coordinate. Returned by `LatticeHandle::parse_coord`
/// for callers (notably `tescellate-core`'s `SheetExtent` bound check) that
/// need to do lattice-aware arithmetic without re-parsing.
#[derive(Debug, Clone, Copy)]
pub enum ParsedCoord {
    Square(SquareCoord),
    Hex(HexCoord),
    Triangle(TriCoord),
}

impl LatticeHandle {
    /// Construct the canonical handle for a `LatticeKind` with default
    /// geometry (cell size). The renderer overrides geometry separately;
    /// the engine layer only cares about address arithmetic.
    pub fn for_kind(kind: LatticeKind) -> Option<Self> {
        match kind {
            LatticeKind::Square => Some(LatticeHandle::Square(SquareLattice::default())),
            LatticeKind::HexPointy => Some(LatticeHandle::Hex(HexLattice::pointy(32.0))),
            LatticeKind::HexFlat => Some(LatticeHandle::Hex(HexLattice::flat(32.0))),
            LatticeKind::Triangle => Some(LatticeHandle::Triangle(TriangleLattice::default())),
            LatticeKind::Parallelogram => None,
        }
    }

    pub fn kind(&self) -> LatticeKind {
        match self {
            LatticeHandle::Square(l) => l.kind(),
            LatticeHandle::Hex(l) => l.kind(),
            LatticeHandle::Triangle(l) => l.kind(),
        }
    }

    /// Parse `addr` into a lattice-specific coord. Useful for callers
    /// that need to do their own arithmetic (extent bounds checks,
    /// spill-target enumeration) without giving up canonical form.
    pub fn parse_coord(&self, addr: &str) -> Result<ParsedCoord, AddressError> {
        match self {
            LatticeHandle::Square(l) => Ok(ParsedCoord::Square(l.parse_address(addr)?)),
            LatticeHandle::Hex(l) => Ok(ParsedCoord::Hex(l.parse_address(addr)?)),
            LatticeHandle::Triangle(l) => Ok(ParsedCoord::Triangle(l.parse_address(addr)?)),
        }
    }

    /// Round-trip a parsed coord through this lattice's address format.
    pub fn format_coord(&self, coord: ParsedCoord) -> String {
        match (self, coord) {
            (LatticeHandle::Square(l), ParsedCoord::Square(c)) => l.address(c),
            (LatticeHandle::Hex(l), ParsedCoord::Hex(c)) => l.address(c),
            (LatticeHandle::Triangle(l), ParsedCoord::Triangle(c)) => l.address(c),
            // Coord/lattice mismatch — degenerate, just stringify the coord.
            (_, ParsedCoord::Square(c)) => format!("[c{},r{}]", c.col, c.row + 1),
            (_, ParsedCoord::Hex(c)) => format!("H({},{})", c.q, c.r),
            (_, ParsedCoord::Triangle(c)) => format!("T({},{})", c.col, c.row),
        }
    }

    /// Canonicalize an address string (`"a1"` → `"A1"`, etc.). Returns
    /// `Err` if the string isn't a valid address for this lattice.
    pub fn canonicalize(&self, addr: &str) -> Result<String, AddressError> {
        match self {
            LatticeHandle::Square(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.address(c))
            }
            LatticeHandle::Hex(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.address(c))
            }
            LatticeHandle::Triangle(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.address(c))
            }
        }
    }

    /// Enumerate the cells in `start:end`. Square = rectangle; hex =
    /// axial-aligned parallelogram. Both endpoints are inclusive.
    pub fn enumerate_range(&self, start: &str, end: &str) -> Result<Vec<String>, AddressError> {
        match self {
            LatticeHandle::Square(l) => {
                let a = l.parse_address(start)?;
                let b = l.parse_address(end)?;
                let (c0, c1) = (a.col.min(b.col), a.col.max(b.col));
                let (r0, r1) = (a.row.min(b.row), a.row.max(b.row));
                let mut out = Vec::with_capacity(((c1 - c0 + 1) * (r1 - r0 + 1)) as usize);
                for r in r0..=r1 {
                    for c in c0..=c1 {
                        out.push(l.address(SquareCoord { col: c, row: r }));
                    }
                }
                Ok(out)
            }
            LatticeHandle::Hex(l) => {
                let a = l.parse_address(start)?;
                let b = l.parse_address(end)?;
                Ok(hex::axial_parallelogram(a, b)
                    .into_iter()
                    .map(|c| l.address(c))
                    .collect())
            }
            LatticeHandle::Triangle(l) => {
                let a = l.parse_address(start)?;
                let b = l.parse_address(end)?;
                Ok(triangle::triangle_rect(a, b)
                    .into_iter()
                    .map(|c| l.address(c))
                    .collect())
            }
        }
    }

    /// Edge-neighbors of `addr` in canonical neighbor order. 4 cells on
    /// square, 6 on hex, 3 on triangle. Each returned address is the
    /// canonical form.
    pub fn neighbor_addresses(&self, addr: &str) -> Result<Vec<String>, AddressError> {
        match self {
            LatticeHandle::Square(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.neighbors(c)
                    .into_iter()
                    .map(|(_, c)| l.address(c))
                    .collect())
            }
            LatticeHandle::Hex(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.neighbors(c)
                    .into_iter()
                    .map(|(_, c)| l.address(c))
                    .collect())
            }
            LatticeHandle::Triangle(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.neighbors(c)
                    .into_iter()
                    .map(|(_, c)| l.address(c))
                    .collect())
            }
        }
    }

    /// Every cell within `radius` edge-steps of `addr` (inclusive of the
    /// center). Returns canonical addresses in a deterministic order:
    /// row-major for square (top-left to bottom-right), q-then-r for hex.
    pub fn cells_within_addresses(
        &self,
        addr: &str,
        radius: i64,
    ) -> Result<Vec<String>, AddressError> {
        match self {
            LatticeHandle::Square(l) => {
                let c = l.parse_address(addr)?;
                let r = radius.max(0) as i32;
                let mut out = Vec::new();
                for dr in -r..=r {
                    for dc in -r..=r {
                        out.push(l.address(SquareCoord {
                            col: c.col + dc,
                            row: c.row + dr,
                        }));
                    }
                }
                Ok(out)
            }
            LatticeHandle::Hex(l) => {
                let c = l.parse_address(addr)?;
                let r = radius.max(0) as i32;
                Ok(hex::hex_disc(c, r)
                    .into_iter()
                    .map(|c| l.address(c))
                    .collect())
            }
            LatticeHandle::Triangle(l) => {
                let c = l.parse_address(addr)?;
                let r = radius.max(0) as i32;
                // For triangles use the (col±r, row±r) rectangle in
                // triangle coords — same convention as the square
                // lattice, since triangles step similarly along their
                // half-base/row axes.
                let mut out = Vec::new();
                for dr in -r..=r {
                    for dc in -r..=r {
                        out.push(l.address(TriCoord {
                            col: c.col + dc,
                            row: c.row + dr,
                        }));
                    }
                }
                Ok(out)
            }
        }
    }
}

#[cfg(test)]
mod handle_tests {
    use super::*;

    #[test]
    fn square_range_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::Square).unwrap();
        let cells = h.enumerate_range("A1", "B2").unwrap();
        assert_eq!(cells, vec!["A1", "B1", "A2", "B2"]);
    }

    #[test]
    fn hex_range_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::HexPointy).unwrap();
        let cells = h.enumerate_range("H(0,0)", "H(1,1)").unwrap();
        assert_eq!(cells.len(), 4);
        assert!(cells.contains(&"H(0,0)".to_string()));
        assert!(cells.contains(&"H(1,1)".to_string()));
    }

    #[test]
    fn triangle_range_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::Triangle).unwrap();
        let cells = h.enumerate_range("T(0,0)", "T(2,1)").unwrap();
        // 3 columns × 2 rows = 6 triangles.
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&"T(0,0)".to_string()));
        assert!(cells.contains(&"T(2,1)".to_string()));
    }

    #[test]
    fn triangle_neighbors_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::Triangle).unwrap();
        // Up △ at T(0,0) has neighbours T(-1,0), T(1,0), T(0,1).
        let n = h.neighbor_addresses("T(0,0)").unwrap();
        assert_eq!(n.len(), 3);
        assert!(n.contains(&"T(-1,0)".to_string()));
        assert!(n.contains(&"T(1,0)".to_string()));
        assert!(n.contains(&"T(0,1)".to_string()));
    }

    #[test]
    fn triangle_canonicalize_round_trips() {
        let h = LatticeHandle::for_kind(LatticeKind::Triangle).unwrap();
        assert_eq!(h.canonicalize("T(0,0)").unwrap(), "T(0,0)");
        assert_eq!(h.canonicalize("T(-3,5)").unwrap(), "T(-3,5)");
    }

    #[test]
    fn square_neighbors_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::Square).unwrap();
        let n = h.neighbor_addresses("B2").unwrap();
        assert_eq!(n.len(), 4);
        // SquareLattice order: N, E, S, W.
        assert!(n.contains(&"B1".to_string()));
        assert!(n.contains(&"C2".to_string()));
        assert!(n.contains(&"B3".to_string()));
        assert!(n.contains(&"A2".to_string()));
    }

    #[test]
    fn hex_neighbors_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::HexPointy).unwrap();
        let n = h.neighbor_addresses("H(0,0)").unwrap();
        assert_eq!(n.len(), 6);
        assert!(n.contains(&"H(1,0)".to_string()));
        assert!(n.contains(&"H(-1,0)".to_string()));
        assert!(n.contains(&"H(0,1)".to_string()));
        assert!(n.contains(&"H(0,-1)".to_string()));
    }

    #[test]
    fn square_radius_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::Square).unwrap();
        let cells = h.cells_within_addresses("B2", 1).unwrap();
        assert_eq!(cells.len(), 9); // 3×3 around B2
    }

    #[test]
    fn hex_radius_via_handle() {
        let h = LatticeHandle::for_kind(LatticeKind::HexPointy).unwrap();
        let cells = h.cells_within_addresses("H(0,0)", 2).unwrap();
        assert_eq!(cells.len(), 19); // 1 + 3*2*3
    }

    #[test]
    fn canonicalize_round_trips() {
        let h = LatticeHandle::for_kind(LatticeKind::Square).unwrap();
        assert_eq!(h.canonicalize("A1").unwrap(), "A1");
        let h = LatticeHandle::for_kind(LatticeKind::HexPointy).unwrap();
        assert_eq!(h.canonicalize("H(2,-3)").unwrap(), "H(2,-3)");
    }
}

/// Identifies which lattice a sheet uses. Stored in workbook files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatticeKind {
    Square,
    HexPointy,
    HexFlat,
    Triangle,
    Parallelogram,
}

/// A 2D point in lattice space (pre-zoom). Renderer applies camera/zoom.
pub type Point2 = Vec2;

/// Axis-aligned bounding rectangle in lattice space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Point2,
    pub max: Point2,
}

#[derive(Debug, Error)]
pub enum AddressError {
    #[error("could not parse address: {0}")]
    Parse(String),
    #[error("address out of range")]
    OutOfRange,
}

/// Edge-adjacency direction. Lattice-specific; each impl maps neighbors to
/// its own variants. Kept as a single enum so cross-lattice code can refer
/// to neighbor labels without generic juggling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    // Square / parallelogram
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
    // Hex pointy-top
    HE,
    HW,
    HNE,
    HNW,
    HSE,
    HSW,
    // Triangle (depends on orientation of the source triangle)
    TLeft,
    TRight,
    TBase,
}

/// The core abstraction of `tescellate-tess`. See PLAN.md §3.1.
///
/// Implementations are dyn-compatible via `LatticeKind`-dispatched wrappers
/// at the workbook layer; this trait itself uses associated types so each
/// impl can keep its natural coordinate representation.
pub trait Lattice {
    type Coord: Copy + Eq + std::hash::Hash + Serialize + for<'de> Deserialize<'de>;

    fn kind(&self) -> LatticeKind;

    fn address(&self, c: Self::Coord) -> String;
    fn parse_address(&self, s: &str) -> Result<Self::Coord, AddressError>;

    fn vertices(&self, c: Self::Coord) -> SmallVec<[Point2; 8]>;
    fn centroid(&self, c: Self::Coord) -> Point2;

    fn neighbors(&self, c: Self::Coord) -> SmallVec<[(Direction, Self::Coord); 8]>;

    fn cell_at(&self, p: Point2) -> Option<Self::Coord>;
}
