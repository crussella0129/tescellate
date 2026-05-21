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
pub mod voronoi;

use hex::{HexCoord, HexLattice};
use square::{SquareCoord, SquareLattice};
use triangle::{TriCoord, TriangleLattice};
use voronoi::{VoronoiCoord, VoronoiLattice};

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
    Voronoi(VoronoiLattice),
}

/// Lattice-specific parsed coordinate. Returned by `LatticeHandle::parse_coord`
/// for callers (notably `tescellate-core`'s `SheetExtent` bound check) that
/// need to do lattice-aware arithmetic without re-parsing.
#[derive(Debug, Clone, Copy)]
pub enum ParsedCoord {
    Square(SquareCoord),
    Hex(HexCoord),
    Triangle(TriCoord),
    Voronoi(VoronoiCoord),
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
            LatticeKind::Voronoi => Some(LatticeHandle::Voronoi(VoronoiLattice::default())),
        }
    }

    pub fn kind(&self) -> LatticeKind {
        match self {
            LatticeHandle::Square(l) => l.kind(),
            LatticeHandle::Hex(l) => l.kind(),
            LatticeHandle::Triangle(l) => l.kind(),
            LatticeHandle::Voronoi(l) => l.kind(),
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
            LatticeHandle::Voronoi(l) => Ok(ParsedCoord::Voronoi(l.parse_address(addr)?)),
        }
    }

    /// Round-trip a parsed coord through this lattice's address format.
    pub fn format_coord(&self, coord: ParsedCoord) -> String {
        match (self, coord) {
            (LatticeHandle::Square(l), ParsedCoord::Square(c)) => l.address(c),
            (LatticeHandle::Hex(l), ParsedCoord::Hex(c)) => l.address(c),
            (LatticeHandle::Triangle(l), ParsedCoord::Triangle(c)) => l.address(c),
            (LatticeHandle::Voronoi(l), ParsedCoord::Voronoi(c)) => l.address(c),
            // Coord/lattice mismatch — degenerate, just stringify the coord.
            (_, ParsedCoord::Square(c)) => format!("[c{},r{}]", c.col, c.row + 1),
            (_, ParsedCoord::Hex(c)) => format!("H({},{})", c.q, c.r),
            (_, ParsedCoord::Triangle(c)) => format!("T({},{})", c.col, c.row),
            (_, ParsedCoord::Voronoi(c)) => format!("V({})", c.0),
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
            LatticeHandle::Voronoi(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.address(c))
            }
        }
    }

    /// Translate `addr` by `(dcol, drow)` and return the canonical
    /// address of the resulting cell — the geometric primitive behind
    /// the `OFFSET` formula function. The two delta axes map per lattice:
    ///
    /// - **Square / Triangle:** `(col + dcol, row + drow)`.
    /// - **Hex:** axial translation `(q + dcol, r + drow)` — the two
    ///   offset axes are the two axial axes, not screen-space x/y.
    /// - **Voronoi:** 1-D seed index; only `drow` applies — a non-zero
    ///   `dcol` is an error since the seeds have no second axis.
    ///
    /// Out-of-range results (negative square col/row, a Voronoi index
    /// past the seed count) return [`AddressError::OutOfRange`].
    pub fn offset(&self, addr: &str, dcol: i32, drow: i32) -> Result<String, AddressError> {
        match self {
            LatticeHandle::Square(l) => {
                let c = l.parse_address(addr)?;
                let col = c.col + dcol;
                let row = c.row + drow;
                if col < 0 || row < 0 {
                    return Err(AddressError::OutOfRange);
                }
                Ok(l.address(square::SquareCoord { col, row }))
            }
            LatticeHandle::Hex(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.address(hex::HexCoord {
                    q: c.q + dcol,
                    r: c.r + drow,
                }))
            }
            LatticeHandle::Triangle(l) => {
                let c = l.parse_address(addr)?;
                Ok(l.address(triangle::TriCoord {
                    col: c.col + dcol,
                    row: c.row + drow,
                }))
            }
            LatticeHandle::Voronoi(l) => {
                if dcol != 0 {
                    // Seeds are a 1-D sequence — there is no column axis.
                    return Err(AddressError::OutOfRange);
                }
                let c = l.parse_address(addr)?;
                let idx = c.0 as i64 + drow as i64;
                if idx < 0 || idx >= l.len() as i64 {
                    return Err(AddressError::OutOfRange);
                }
                Ok(l.address(voronoi::VoronoiCoord(idx as u32)))
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
            LatticeHandle::Voronoi(l) => {
                // Voronoi has no natural rectangular range — interpret
                // `V(a):V(b)` as the inclusive index span. Useful for
                // SUM(V(0):V(3)) and similar batch references.
                let a = l.parse_address(start)?;
                let b = l.parse_address(end)?;
                let (lo, hi) = (a.0.min(b.0), a.0.max(b.0));
                Ok((lo..=hi).map(|i| l.address(VoronoiCoord(i))).collect())
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
            LatticeHandle::Voronoi(l) => {
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
            LatticeHandle::Voronoi(l) => {
                // Voronoi distance isn't well-defined in cell-step
                // terms — fall back to "everyone within `radius` of the
                // seed in Euclidean distance". For radius == 0 this
                // means only the center cell; positive radii return
                // every seed whose centroid is within `radius` of the
                // anchor seed's centroid. Good enough for the launch
                // demo; a Delaunay-driven cell-step distance lands
                // alongside the v150 follow-up.
                let c = l.parse_address(addr)?;
                let anchor = l.centroid(c);
                let r2 = (radius as f32) * (radius as f32);
                let mut out = Vec::new();
                for i in 0..l.seeds.len() as u32 {
                    let cand = VoronoiCoord(i);
                    let d2 = (l.centroid(cand) - anchor).length_squared();
                    if d2 <= r2 + 1e-3 {
                        out.push(l.address(cand));
                    }
                }
                Ok(out)
            }
        }
    }

    /// Lattice-native distance between two addresses. Each lattice
    /// answers in its own metric:
    ///
    /// - Square: Chebyshev (king-move) distance —
    ///   `max(|Δcol|, |Δrow|)`.
    /// - Hex: hex edge-step distance (cube metric).
    /// - Triangle: same-axis Chebyshev metric as square — two
    ///   triangles a Δ-(col, row) apart sit `max(|Δcol|, |Δrow|)`
    ///   half-bases / row-heights apart. Triangle's true edge-step
    ///   distance is more nuanced (depends on orientations along the
    ///   path) and lands in a follow-up.
    pub fn lattice_distance(&self, a: &str, b: &str) -> Result<i64, AddressError> {
        match self {
            LatticeHandle::Square(l) => {
                let a = l.parse_address(a)?;
                let b = l.parse_address(b)?;
                Ok((a.col - b.col).abs().max((a.row - b.row).abs()) as i64)
            }
            LatticeHandle::Hex(l) => {
                let a = l.parse_address(a)?;
                let b = l.parse_address(b)?;
                Ok(a.distance(b) as i64)
            }
            LatticeHandle::Triangle(l) => {
                let a = l.parse_address(a)?;
                let b = l.parse_address(b)?;
                Ok((a.col - b.col).abs().max((a.row - b.row).abs()) as i64)
            }
            LatticeHandle::Voronoi(l) => {
                // No canonical cell-step distance yet — return the
                // Euclidean distance between seed centroids rounded to
                // an integer. Same caveat as `cells_within_addresses`.
                let a = l.parse_address(a)?;
                let b = l.parse_address(b)?;
                Ok((l.centroid(a) - l.centroid(b)).length().round() as i64)
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
    fn offset_square_translates_col_row() {
        let h = LatticeHandle::for_kind(LatticeKind::Square).unwrap();
        // B2 is (col 1, row 1); +1 col, +2 row → (col 2, row 3) = C4.
        assert_eq!(h.offset("B2", 1, 2).unwrap(), "C4");
    }

    #[test]
    fn offset_square_out_of_range_errors() {
        let h = LatticeHandle::for_kind(LatticeKind::Square).unwrap();
        assert!(h.offset("A1", -1, 0).is_err());
        assert!(h.offset("A1", 0, -1).is_err());
    }

    #[test]
    fn offset_hex_axial() {
        let h = LatticeHandle::for_kind(LatticeKind::HexPointy).unwrap();
        assert_eq!(h.offset("H(0,0)", 1, 2).unwrap(), "H(1,2)");
        assert_eq!(h.offset("H(2,-3)", -1, 1).unwrap(), "H(1,-2)");
    }

    #[test]
    fn offset_triangle_translates() {
        let h = LatticeHandle::for_kind(LatticeKind::Triangle).unwrap();
        assert_eq!(h.offset("T(0,0)", 2, -1).unwrap(), "T(2,-1)");
    }

    #[test]
    fn offset_voronoi_linear() {
        let h = LatticeHandle::for_kind(LatticeKind::Voronoi).unwrap();
        // 1-D index: only the row delta applies.
        assert_eq!(h.offset("V(2)", 0, 3).unwrap(), "V(5)");
    }

    #[test]
    fn offset_voronoi_nonzero_dcol_errors() {
        let h = LatticeHandle::for_kind(LatticeKind::Voronoi).unwrap();
        assert!(h.offset("V(2)", 1, 0).is_err());
    }

    #[test]
    fn offset_voronoi_out_of_range_errors() {
        let h = LatticeHandle::for_kind(LatticeKind::Voronoi).unwrap();
        // Default config has 8 seeds (indices 0..=7).
        assert!(h.offset("V(0)", 0, 999).is_err());
        assert!(h.offset("V(0)", 0, -1).is_err());
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
    fn voronoi_handle_canonicalises() {
        let h = LatticeHandle::for_kind(LatticeKind::Voronoi).unwrap();
        assert_eq!(h.canonicalize("V(0)").unwrap(), "V(0)");
        assert_eq!(h.canonicalize("V(3)").unwrap(), "V(3)");
        // Default config has 8 seeds; V(99) is out of range.
        assert!(h.canonicalize("V(99)").is_err());
    }

    #[test]
    fn voronoi_handle_neighbors_are_delaunay() {
        let h = LatticeHandle::for_kind(LatticeKind::Voronoi).unwrap();
        let n = h.neighbor_addresses("V(0)").unwrap();
        // v157: Delaunay adjacency — a strict subset of "all 7 others"
        // (a seed neighbors only those whose cells share an edge). The
        // exact set is pinned by `voronoi::tests::delaunay_neighbors_on_
        // known_config`; here we assert the dispatch returns a sane,
        // self-excluding, in-range, deduplicated neighbor set.
        assert!(
            !n.is_empty(),
            "a 2-D seed has at least one Delaunay neighbor"
        );
        assert!(
            n.len() < 7,
            "Delaunay adjacency is a strict subset of all others"
        );
        assert!(!n.contains(&"V(0)".to_string()), "no self-adjacency");
        let mut seen = std::collections::HashSet::new();
        for addr in &n {
            assert!(
                seen.insert(addr.clone()),
                "neighbors must be unique: {addr}"
            );
            let idx: u32 = addr
                .trim_start_matches("V(")
                .trim_end_matches(')')
                .parse()
                .unwrap();
            assert!(idx < 8, "neighbor index in range for the 8-seed default");
        }
    }

    #[test]
    fn square_distance_is_chebyshev() {
        let h = LatticeHandle::for_kind(LatticeKind::Square).unwrap();
        assert_eq!(h.lattice_distance("A1", "A1").unwrap(), 0);
        assert_eq!(h.lattice_distance("A1", "B2").unwrap(), 1);
        // A1 → C5: 2 columns + 4 rows → Chebyshev = max(2, 4) = 4.
        assert_eq!(h.lattice_distance("A1", "C5").unwrap(), 4);
    }

    #[test]
    fn hex_distance_uses_cube_metric() {
        let h = LatticeHandle::for_kind(LatticeKind::HexPointy).unwrap();
        assert_eq!(h.lattice_distance("H(0,0)", "H(0,0)").unwrap(), 0);
        assert_eq!(h.lattice_distance("H(0,0)", "H(1,0)").unwrap(), 1);
        // A "knight-jump" hex pair — H(2, -1) is two edge steps away.
        assert_eq!(h.lattice_distance("H(0,0)", "H(2,-1)").unwrap(), 2);
    }

    #[test]
    fn triangle_distance_is_chebyshev_on_axes() {
        let h = LatticeHandle::for_kind(LatticeKind::Triangle).unwrap();
        assert_eq!(h.lattice_distance("T(0,0)", "T(0,0)").unwrap(), 0);
        assert_eq!(h.lattice_distance("T(0,0)", "T(1,0)").unwrap(), 1);
        assert_eq!(h.lattice_distance("T(0,0)", "T(3,5)").unwrap(), 5);
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
    Voronoi,
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
