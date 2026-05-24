//! Triangle lattice — Phase 3's third concrete tiling (after square +
//! hex). Triangles tessellate as alternating up- and down-pointing
//! pairs that share their slanted edges; each row of triangles is one
//! horizontal strip.
//!
//! ## Coordinates
//!
//! `TriCoord { col, row }` with implicit orientation:
//!
//! - Up-pointing `△` if `(col + row) % 2 == 0`.
//! - Down-pointing `▽` otherwise.
//!
//! `col` advances by *half a triangle base*. `(col=0, row=0)` is the
//! left-most up triangle in the top row; `(col=1, row=0)` is the
//! down-pointing triangle immediately to its right.
//!
//! ## Geometry
//!
//! With side length `s` and row height `h = s·√3/2`:
//!
//! - Centroid x: `(col + 1) · s/2`.
//! - Centroid y (up):   `row · h + 2h/3`.
//! - Centroid y (down): `row · h + h/3`.
//!
//! ## Address syntax
//!
//! `T(col,row)` — e.g. `T(0,0)`, `T(3,-1)`. The address parser tolerates
//! whitespace around the comma; the `T` prefix is case-sensitive to
//! match `H(...)`'s convention.
//!
//! Range semantics for `T(c0,r0):T(c1,r1)` is the rectangular block
//! between the two corners, identical in shape to the square lattice
//! but enumerated through triangle coordinates.

use crate::{AddressError, Direction, Lattice, LatticeKind, Point2};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

/// A triangle cell coordinate. `col` advances by half a triangle base
/// in lattice space; `row` advances by one row height. The cell's
/// orientation (up or down) is the parity of `col + row`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TriCoord {
    pub col: i32,
    pub row: i32,
}

impl TriCoord {
    pub const fn new(col: i32, row: i32) -> Self {
        Self { col, row }
    }

    /// `true` when the triangle points upward (`△`), `false` when it
    /// points downward (`▽`).
    #[inline]
    pub fn points_up(self) -> bool {
        (self.col + self.row).rem_euclid(2) == 0
    }
}

/// A regular-triangle lattice. `side` is the triangle's edge length in
/// lattice units.
pub struct TriangleLattice {
    pub side: f32,
}

impl Default for TriangleLattice {
    fn default() -> Self {
        Self { side: 48.0 }
    }
}

impl TriangleLattice {
    pub fn new(side: f32) -> Self {
        Self { side }
    }

    /// Row height — the vertical pitch between rows.
    #[inline]
    fn row_height(&self) -> f32 {
        self.side * 3.0_f32.sqrt() * 0.5
    }
}

impl Lattice for TriangleLattice {
    type Coord = TriCoord;

    fn kind(&self) -> LatticeKind {
        LatticeKind::Triangle
    }

    fn address(&self, c: Self::Coord) -> String {
        format_triangle(c)
    }

    fn parse_address(&self, s: &str) -> Result<Self::Coord, AddressError> {
        parse_triangle(s)
    }

    fn vertices(&self, c: Self::Coord) -> SmallVec<[Point2; 8]> {
        let s = self.side;
        let h = self.row_height();
        let row = c.row as f32;
        let col = c.col as f32;
        let mut out: SmallVec<[Point2; 8]> = SmallVec::new();
        if c.points_up() {
            // Up `△`: base at the bottom of the row, apex at the top.
            out.push(Vec2::new(col * s * 0.5, row * h + h));
            out.push(Vec2::new((col + 2.0) * s * 0.5, row * h + h));
            out.push(Vec2::new((col + 1.0) * s * 0.5, row * h));
        } else {
            // Down `▽`: base at the top of the row, apex at the bottom.
            out.push(Vec2::new(col * s * 0.5, row * h));
            out.push(Vec2::new((col + 2.0) * s * 0.5, row * h));
            out.push(Vec2::new((col + 1.0) * s * 0.5, row * h + h));
        }
        out
    }

    fn centroid(&self, c: Self::Coord) -> Point2 {
        let s = self.side;
        let h = self.row_height();
        let x = (c.col as f32 + 1.0) * s * 0.5;
        let y = if c.points_up() {
            c.row as f32 * h + 2.0 * h / 3.0
        } else {
            c.row as f32 * h + h / 3.0
        };
        Vec2::new(x, y)
    }

    fn neighbors(&self, c: Self::Coord) -> SmallVec<[(Direction, Self::Coord); 8]> {
        // Three edge-neighbors per triangle: left, right, and the
        // opposite-orientation triangle across the horizontal base —
        // below for `△`, above for `▽`. Edge labels are reused from
        // the square set; renderers that need a more triangle-specific
        // tag can derive it from `points_up()`.
        let left = TriCoord {
            col: c.col - 1,
            row: c.row,
        };
        let right = TriCoord {
            col: c.col + 1,
            row: c.row,
        };
        if c.points_up() {
            // △: base neighbour is below.
            let base = TriCoord {
                col: c.col,
                row: c.row + 1,
            };
            smallvec![
                (Direction::TLeft, left),
                (Direction::TRight, right),
                (Direction::TBase, base),
            ]
        } else {
            // ▽: base neighbour is above.
            let base = TriCoord {
                col: c.col,
                row: c.row - 1,
            };
            smallvec![
                (Direction::TLeft, left),
                (Direction::TRight, right),
                (Direction::TBase, base),
            ]
        }
    }

    fn cell_at(&self, p: Point2) -> Option<Self::Coord> {
        let s = self.side;
        let h = self.row_height();
        if s <= 0.0 || h <= 0.0 {
            return None;
        }
        // The row strip is unambiguous — y maps to exactly one row.
        let r_float = p.y / h;
        let row = r_float.floor() as i32;
        // Within the strip, the candidate columns straddle the
        // half-triangle x = (col+1) * s/2. Probe a small range to find
        // the triangle whose interior contains `p`.
        let approx_col = (p.x / (s * 0.5) - 1.0).floor() as i32;
        for d in -1..=2 {
            let candidate = TriCoord {
                col: approx_col + d,
                row,
            };
            if point_in_triangle(p, self.vertices(candidate).as_slice()) {
                return Some(candidate);
            }
        }
        None
    }
}

/// Format a triangle coord as `T(col,row)` — no spaces, signed ints.
fn format_triangle(c: TriCoord) -> String {
    format!("T({},{})", c.col, c.row)
}

/// Parse a `T(col,row)` address. Whitespace tolerated around the comma;
/// case-sensitive `T` prefix.
fn parse_triangle(s: &str) -> Result<TriCoord, AddressError> {
    let s_trim = s.trim();
    let inner = s_trim
        .strip_prefix("T(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| AddressError::Parse(s.into()))?;
    let (cs, rs) = inner
        .split_once(',')
        .ok_or_else(|| AddressError::Parse(s.into()))?;
    let col: i32 = cs
        .trim()
        .parse()
        .map_err(|_| AddressError::Parse(s.into()))?;
    let row: i32 = rs
        .trim()
        .parse()
        .map_err(|_| AddressError::Parse(s.into()))?;
    Ok(TriCoord { col, row })
}

/// Whether `p` is inside the triangle whose three vertices are `verts`,
/// inclusive of the edges. Uses the cross-product sign test — robust
/// against winding order.
fn point_in_triangle(p: Point2, verts: &[Point2]) -> bool {
    if verts.len() != 3 {
        return false;
    }
    let (a, b, c) = (verts[0], verts[1], verts[2]);
    let s1 = cross(a, b, p);
    let s2 = cross(b, c, p);
    let s3 = cross(c, a, p);
    let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0;
    let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0;
    !(has_neg && has_pos)
}

#[inline]
fn cross(a: Point2, b: Point2, p: Point2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

/// Enumerate the cells in the inclusive rectangular block whose
/// opposite corners are `a` and `b`. Used for triangle range semantics
/// `T(c0,r0):T(c1,r1)`.
pub fn triangle_rect(a: TriCoord, b: TriCoord) -> Vec<TriCoord> {
    let c_lo = a.col.min(b.col);
    let c_hi = a.col.max(b.col);
    let r_lo = a.row.min(b.row);
    let r_hi = a.row.max(b.row);
    let mut out = Vec::with_capacity(((c_hi - c_lo + 1) * (r_hi - r_lo + 1)) as usize);
    for row in r_lo..=r_hi {
        for col in c_lo..=c_hi {
            out.push(TriCoord { col, row });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_decides_orientation() {
        assert!(TriCoord::new(0, 0).points_up());
        assert!(!TriCoord::new(1, 0).points_up());
        assert!(!TriCoord::new(0, 1).points_up());
        assert!(TriCoord::new(2, 0).points_up());
        // Negative coords use rem_euclid so the parity is still
        // well-defined.
        assert!(!TriCoord::new(-1, 0).points_up());
        assert!(TriCoord::new(-2, 0).points_up());
    }

    #[test]
    fn address_round_trips() {
        let lat = TriangleLattice::default();
        for c in [
            TriCoord::new(0, 0),
            TriCoord::new(1, 0),
            TriCoord::new(3, 5),
            TriCoord::new(-2, -7),
        ] {
            let s = lat.address(c);
            assert_eq!(lat.parse_address(&s).unwrap(), c);
        }
    }

    #[test]
    fn parse_address_rejects_garbage() {
        let lat = TriangleLattice::default();
        assert!(lat.parse_address("T 0,0)").is_err());
        assert!(lat.parse_address("T(0)").is_err());
        assert!(lat.parse_address("(0,0)").is_err());
        assert!(lat.parse_address("T(a,0)").is_err());
    }

    #[test]
    fn vertices_have_three_points() {
        let lat = TriangleLattice::new(10.0);
        assert_eq!(lat.vertices(TriCoord::new(0, 0)).len(), 3);
        assert_eq!(lat.vertices(TriCoord::new(1, 0)).len(), 3);
    }

    #[test]
    fn up_and_down_share_a_slanted_edge() {
        // Up △ at (0,0) and Down ▽ at (1,0) share the edge
        // {(s, h), (s/2, 0)}.
        let lat = TriangleLattice::new(10.0);
        let up = lat.vertices(TriCoord::new(0, 0));
        let down = lat.vertices(TriCoord::new(1, 0));
        let shared = up.iter().filter(|v| down.contains(v)).count();
        assert_eq!(shared, 2);
    }

    #[test]
    fn centroid_is_average_of_vertices() {
        let lat = TriangleLattice::new(12.0);
        for c in [
            TriCoord::new(0, 0),
            TriCoord::new(1, 0),
            TriCoord::new(2, 1),
            TriCoord::new(-1, 3),
        ] {
            let verts = lat.vertices(c);
            let avg = verts.iter().fold(Vec2::ZERO, |a, v| a + *v) / verts.len() as f32;
            let centroid = lat.centroid(c);
            assert!(
                (avg - centroid).length() < 1e-4,
                "centroid mismatch at {c:?}: avg={avg:?}, centroid={centroid:?}",
            );
        }
    }

    #[test]
    fn cell_at_finds_the_triangle_containing_its_centroid() {
        let lat = TriangleLattice::new(20.0);
        for c in [
            TriCoord::new(0, 0),
            TriCoord::new(1, 0),
            TriCoord::new(2, 1),
            TriCoord::new(3, 2),
            TriCoord::new(-1, 1),
        ] {
            let p = lat.centroid(c);
            assert_eq!(lat.cell_at(p), Some(c), "cell_at failed for {c:?}");
        }
    }

    #[test]
    fn neighbours_are_three_with_opposite_orientation() {
        let lat = TriangleLattice::new(10.0);
        let cell = TriCoord::new(0, 0);
        let neighbours = lat.neighbors(cell);
        assert_eq!(neighbours.len(), 3);
        // Every neighbour points the opposite way.
        for &(_, n) in neighbours.iter() {
            assert_ne!(n.points_up(), cell.points_up());
        }
    }

    #[test]
    fn up_has_base_neighbour_below_and_down_above() {
        let lat = TriangleLattice::new(10.0);
        let up = TriCoord::new(0, 0);
        let up_base = lat
            .neighbors(up)
            .into_iter()
            .find(|(d, _)| matches!(d, Direction::TBase))
            .unwrap()
            .1;
        assert_eq!(up_base, TriCoord::new(0, 1));

        let down = TriCoord::new(1, 0);
        let down_base = lat
            .neighbors(down)
            .into_iter()
            .find(|(d, _)| matches!(d, Direction::TBase))
            .unwrap()
            .1;
        assert_eq!(down_base, TriCoord::new(1, -1));
    }

    #[test]
    fn triangle_rect_enumerates_the_inclusive_block() {
        let cells = triangle_rect(TriCoord::new(0, 0), TriCoord::new(2, 1));
        // 3 columns × 2 rows = 6 cells.
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&TriCoord::new(0, 0)));
        assert!(cells.contains(&TriCoord::new(2, 1)));
    }
}
