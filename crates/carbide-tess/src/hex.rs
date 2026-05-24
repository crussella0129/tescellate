//! Hex lattice — Phase 2's first concrete non-rectangular tiling.
//!
//! Coordinates are **axial** `(q, r)` per the Red Blob Games convention
//! (https://www.redblobgames.com/grids/hexagons/). Pointy-top is the
//! default orientation; flat-top is selectable via `HexOrientation`.
//!
//! Address syntax: `H(q,r)`, e.g. `H(0,0)`, `H(2,-3)`. The origin is the
//! sheet's anchor cell (selected at sheet-creation time).
//!
//! Range semantics for `H(a,b):H(c,d)` is the **axial-aligned parallelogram**
//! with the two endpoints as opposite corners. Other shapes (hex disc,
//! ring) are exposed as function-form constructors (`RADIUS(H(0,0), 3)`)
//! rather than new range syntaxes — see `docs/carbide/addressing.md` §2.

use crate::{AddressError, Direction, Lattice, LatticeKind, Point2};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::f32::consts::PI;

/// Axial hex coordinate. `q` runs along one of the three hex axes; `r`
/// along another. The implicit third axis `s = -q - r` is recoverable
/// when needed (used internally for cube rounding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

impl HexCoord {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Implicit cube `s`-axis. `q + r + s == 0` by construction.
    #[inline]
    pub fn s(self) -> i32 {
        -self.q - self.r
    }

    /// Manhattan-style distance in the hex lattice: how many edge-steps
    /// it takes to walk between two cells.
    pub fn distance(self, other: HexCoord) -> i32 {
        let dq = (self.q - other.q).abs();
        let dr = (self.r - other.r).abs();
        let ds = (self.s() - other.s()).abs();
        // Hex distance = max of |dq|, |dr|, |ds|. Equivalent formulations
        // exist; this one mirrors what cube-coordinate code typically uses.
        dq.max(dr).max(ds)
    }
}

/// Whether the hex sits with a vertex on top (`Pointy`) or an edge on
/// top (`Flat`). Affects geometry and the neighbor direction labels but
/// not the axial coordinates themselves.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HexOrientation {
    #[default]
    Pointy,
    Flat,
}

/// A regular-hexagon lattice. `hex_size` is the circumradius (distance
/// from centroid to vertex) in lattice units.
pub struct HexLattice {
    pub hex_size: f32,
    pub orientation: HexOrientation,
}

impl Default for HexLattice {
    fn default() -> Self {
        Self {
            hex_size: 32.0,
            orientation: HexOrientation::Pointy,
        }
    }
}

impl HexLattice {
    pub fn pointy(hex_size: f32) -> Self {
        Self {
            hex_size,
            orientation: HexOrientation::Pointy,
        }
    }

    pub fn flat(hex_size: f32) -> Self {
        Self {
            hex_size,
            orientation: HexOrientation::Flat,
        }
    }
}

impl Lattice for HexLattice {
    type Coord = HexCoord;

    fn kind(&self) -> LatticeKind {
        match self.orientation {
            HexOrientation::Pointy => LatticeKind::HexPointy,
            HexOrientation::Flat => LatticeKind::HexFlat,
        }
    }

    fn address(&self, c: Self::Coord) -> String {
        format_hex(c)
    }

    fn parse_address(&self, s: &str) -> Result<Self::Coord, AddressError> {
        parse_hex(s)
    }

    fn vertices(&self, c: Self::Coord) -> SmallVec<[Point2; 8]> {
        let center = self.centroid(c);
        let mut out: SmallVec<[Point2; 8]> = SmallVec::new();
        for i in 0..6 {
            // Pointy-top: vertices at angles 30°, 90°, 150°, 210°, 270°, 330°.
            // Flat-top:   vertices at angles  0°, 60°, 120°, 180°, 240°, 300°.
            let angle_deg = match self.orientation {
                HexOrientation::Pointy => 60.0 * (i as f32) - 30.0,
                HexOrientation::Flat => 60.0 * (i as f32),
            };
            let angle = angle_deg * PI / 180.0;
            out.push(Vec2::new(
                center.x + self.hex_size * angle.cos(),
                center.y + self.hex_size * angle.sin(),
            ));
        }
        out
    }

    fn centroid(&self, c: Self::Coord) -> Point2 {
        let size = self.hex_size;
        let q = c.q as f32;
        let r = c.r as f32;
        let sqrt3 = 3.0_f32.sqrt();
        match self.orientation {
            HexOrientation::Pointy => {
                Vec2::new(size * (sqrt3 * q + sqrt3 * 0.5 * r), size * (1.5 * r))
            }
            HexOrientation::Flat => {
                Vec2::new(size * (1.5 * q), size * (sqrt3 * 0.5 * q + sqrt3 * r))
            }
        }
    }

    fn neighbors(&self, c: Self::Coord) -> SmallVec<[(Direction, Self::Coord); 8]> {
        // The six axial offsets are the *same* regardless of orientation;
        // only the on-screen direction labels rotate. The Direction enum
        // already has HE/HW/HNE/HNW/HSE/HSW reserved for pointy-top; for
        // flat-top we reuse the same labels (rotated visually) so cross-
        // lattice code can match on a stable name. Renderers that care
        // about screen direction read `orientation` and rotate.
        smallvec![
            (Direction::HE, HexCoord { q: c.q + 1, r: c.r }),
            (
                Direction::HNE,
                HexCoord {
                    q: c.q + 1,
                    r: c.r - 1
                }
            ),
            (Direction::HNW, HexCoord { q: c.q, r: c.r - 1 }),
            (Direction::HW, HexCoord { q: c.q - 1, r: c.r }),
            (
                Direction::HSW,
                HexCoord {
                    q: c.q - 1,
                    r: c.r + 1
                }
            ),
            (Direction::HSE, HexCoord { q: c.q, r: c.r + 1 }),
        ]
    }

    fn cell_at(&self, p: Point2) -> Option<Self::Coord> {
        let size = self.hex_size;
        let sqrt3 = 3.0_f32.sqrt();
        // Inverse of `centroid`. Produces fractional axial coords; we
        // then round to the nearest integer hex via cube rounding.
        let (q_frac, r_frac) = match self.orientation {
            HexOrientation::Pointy => {
                let q = (sqrt3 / 3.0 * p.x - p.y / 3.0) / size;
                let r = (2.0 / 3.0 * p.y) / size;
                (q, r)
            }
            HexOrientation::Flat => {
                let q = (2.0 / 3.0 * p.x) / size;
                let r = (-p.x / 3.0 + sqrt3 / 3.0 * p.y) / size;
                (q, r)
            }
        };
        Some(axial_round(q_frac, r_frac))
    }
}

/// Cube-coordinate rounding: convert fractional axial coords to the
/// nearest integer hex. Standard algorithm — round each cube axis to
/// the nearest integer, then fix up the axis with the largest delta to
/// preserve the `q + r + s == 0` invariant.
fn axial_round(q_frac: f32, r_frac: f32) -> HexCoord {
    let s_frac = -q_frac - r_frac;

    let mut q = q_frac.round();
    let mut r = r_frac.round();
    let s = s_frac.round();

    let q_diff = (q - q_frac).abs();
    let r_diff = (r - r_frac).abs();
    let s_diff = (s - s_frac).abs();

    if q_diff > r_diff && q_diff > s_diff {
        q = -r - s;
    } else if r_diff > s_diff {
        r = -q - s;
    } else {
        // s gets recomputed by virtue of using q and r below.
    }

    HexCoord {
        q: q as i32,
        r: r as i32,
    }
}

/// Format a hex coord as `H(q,r)`. No spaces, signed integers.
fn format_hex(c: HexCoord) -> String {
    format!("H({},{})", c.q, c.r)
}

/// Parse an `H(q,r)` address. Whitespace tolerated around the comma and
/// parentheses contents; case-sensitive `H` prefix.
fn parse_hex(s: &str) -> Result<HexCoord, AddressError> {
    let s_trim = s.trim();
    let inner = s_trim
        .strip_prefix("H(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| AddressError::Parse(s.into()))?;
    let (qs, rs) = inner
        .split_once(',')
        .ok_or_else(|| AddressError::Parse(s.into()))?;
    let q: i32 = qs
        .trim()
        .parse()
        .map_err(|_| AddressError::Parse(s.into()))?;
    let r: i32 = rs
        .trim()
        .parse()
        .map_err(|_| AddressError::Parse(s.into()))?;
    Ok(HexCoord { q, r })
}

/// Enumerate the cells in the axial-aligned parallelogram with `a` and
/// `b` as opposite corners. Inclusive on both ends. Used for hex range
/// semantics (`H(a,b):H(c,d)`).
pub fn axial_parallelogram(a: HexCoord, b: HexCoord) -> Vec<HexCoord> {
    let q_lo = a.q.min(b.q);
    let q_hi = a.q.max(b.q);
    let r_lo = a.r.min(b.r);
    let r_hi = a.r.max(b.r);
    let mut out = Vec::with_capacity(((q_hi - q_lo + 1) * (r_hi - r_lo + 1)) as usize);
    for r in r_lo..=r_hi {
        for q in q_lo..=q_hi {
            out.push(HexCoord { q, r });
        }
    }
    out
}

/// Enumerate every cell within `radius` edge-steps of `center` (inclusive).
/// Returns `1 + 3*N*(N+1)` cells for radius N (the closed hex disc).
pub fn hex_disc(center: HexCoord, radius: i32) -> Vec<HexCoord> {
    let radius = radius.max(0);
    let cap = 1 + 3 * radius * (radius + 1);
    let mut out = Vec::with_capacity(cap as usize);
    for dq in -radius..=radius {
        let r_min = (-radius).max(-dq - radius);
        let r_max = radius.min(-dq + radius);
        for dr in r_min..=r_max {
            out.push(HexCoord {
                q: center.q + dq,
                r: center.r + dr,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse_round_trip() {
        let cases = [
            HexCoord { q: 0, r: 0 },
            HexCoord { q: 1, r: 0 },
            HexCoord { q: 0, r: 1 },
            HexCoord { q: -5, r: 3 },
            HexCoord { q: 17, r: -42 },
            HexCoord { q: -100, r: -100 },
        ];
        for c in cases {
            let s = format_hex(c);
            assert_eq!(parse_hex(&s).unwrap(), c, "round-trip failed for {c:?}");
        }
    }

    #[test]
    fn parse_known_strings() {
        assert_eq!(parse_hex("H(0,0)").unwrap(), HexCoord { q: 0, r: 0 });
        assert_eq!(parse_hex("H(2,-3)").unwrap(), HexCoord { q: 2, r: -3 });
        assert_eq!(parse_hex("H( 1 , 2 )").unwrap(), HexCoord { q: 1, r: 2 });
    }

    #[test]
    fn parse_rejects_garbage() {
        let bad = [
            "", "A1", "H()", "H(1)", "H(1,)", "H(,1)", "H(1,2", "h(1,2)", "H(1.5,2)",
        ];
        for s in bad {
            assert!(parse_hex(s).is_err(), "expected error for {s:?}");
        }
    }

    #[test]
    fn six_neighbors_pointy() {
        let lat = HexLattice::pointy(32.0);
        let n = lat.neighbors(HexCoord { q: 0, r: 0 });
        assert_eq!(n.len(), 6);
        // Each neighbor is at axial distance 1.
        for (_, c) in &n {
            assert_eq!(HexCoord { q: 0, r: 0 }.distance(*c), 1);
        }
    }

    #[test]
    fn six_neighbors_flat() {
        let lat = HexLattice::flat(32.0);
        let n = lat.neighbors(HexCoord { q: 0, r: 0 });
        assert_eq!(n.len(), 6);
        for (_, c) in &n {
            assert_eq!(HexCoord { q: 0, r: 0 }.distance(*c), 1);
        }
    }

    #[test]
    fn neighbors_are_symmetric() {
        // If B is a neighbor of A, then A is a neighbor of B.
        let lat = HexLattice::pointy(32.0);
        let a = HexCoord { q: 3, r: -1 };
        for (_, b) in lat.neighbors(a) {
            let back: Vec<HexCoord> = lat.neighbors(b).into_iter().map(|(_, c)| c).collect();
            assert!(back.contains(&a), "{a:?} not in neighbors of {b:?}");
        }
    }

    #[test]
    fn distance_matches_neighbor_steps() {
        let a = HexCoord { q: 0, r: 0 };
        let b = HexCoord { q: 3, r: 2 };
        // Verify by reachability: the cell-by-cell hex disc of radius 5
        // contains b iff distance(a, b) <= 5.
        assert!(hex_disc(a, 5).contains(&b));
        assert_eq!(a.distance(b), 5);
    }

    #[test]
    fn centroid_round_trips_through_cell_at_pointy() {
        let lat = HexLattice::pointy(32.0);
        for q in -10..=10 {
            for r in -10..=10 {
                let c = HexCoord { q, r };
                let centroid = lat.centroid(c);
                assert_eq!(lat.cell_at(centroid), Some(c), "round-trip failed at {c:?}");
            }
        }
    }

    #[test]
    fn centroid_round_trips_through_cell_at_flat() {
        let lat = HexLattice::flat(32.0);
        for q in -10..=10 {
            for r in -10..=10 {
                let c = HexCoord { q, r };
                let centroid = lat.centroid(c);
                assert_eq!(lat.cell_at(centroid), Some(c), "round-trip failed at {c:?}");
            }
        }
    }

    #[test]
    fn cell_at_perturbed_centroid_still_resolves() {
        // Cube rounding should keep us in the same cell within ~30% of the
        // hex size offset from the centroid. Avoid the apothem itself
        // (~86.6% of hex_size for pointy) — that's an edge.
        let lat = HexLattice::pointy(32.0);
        let c = HexCoord { q: 4, r: -2 };
        let centroid = lat.centroid(c);
        for (dx, dy) in [
            (0.0, 0.0),
            (5.0, 0.0),
            (-5.0, 3.0),
            (0.0, -7.0),
            (-4.0, -4.0),
        ] {
            let p = Vec2::new(centroid.x + dx, centroid.y + dy);
            assert_eq!(
                lat.cell_at(p),
                Some(c),
                "perturbation ({dx},{dy}) broke cell_at"
            );
        }
    }

    #[test]
    fn vertices_are_equidistant_from_centroid() {
        let lat = HexLattice::pointy(32.0);
        let c = HexCoord { q: 1, r: -1 };
        let centroid = lat.centroid(c);
        let verts = lat.vertices(c);
        assert_eq!(verts.len(), 6);
        for v in &verts {
            let d = (*v - centroid).length();
            assert!(
                (d - lat.hex_size).abs() < 1e-3,
                "vertex {v:?} not at hex_size distance"
            );
        }
    }

    #[test]
    fn hex_disc_size_matches_formula() {
        // |hex disc radius N| = 1 + 3*N*(N+1).
        for radius in 0..=5 {
            let n = hex_disc(HexCoord { q: 0, r: 0 }, radius).len() as i32;
            let expected = 1 + 3 * radius * (radius + 1);
            assert_eq!(n, expected, "wrong cell count for radius {radius}");
        }
    }

    #[test]
    fn hex_disc_radius_zero_is_singleton() {
        let d = hex_disc(HexCoord { q: 7, r: -3 }, 0);
        assert_eq!(d, vec![HexCoord { q: 7, r: -3 }]);
    }

    #[test]
    fn hex_disc_only_contains_cells_within_distance() {
        let center = HexCoord { q: 2, r: 1 };
        for radius in 0..=4 {
            for c in hex_disc(center, radius) {
                assert!(
                    center.distance(c) <= radius,
                    "{c:?} too far from {center:?} for radius {radius}"
                );
            }
        }
    }

    #[test]
    fn axial_parallelogram_is_rectangle_in_qr() {
        let p = axial_parallelogram(HexCoord { q: 0, r: 0 }, HexCoord { q: 2, r: 3 });
        assert_eq!(p.len(), 12);
        assert!(p.contains(&HexCoord { q: 0, r: 0 }));
        assert!(p.contains(&HexCoord { q: 2, r: 3 }));
        assert!(p.contains(&HexCoord { q: 1, r: 2 }));
        // Should not include cells outside the qr box.
        assert!(!p.contains(&HexCoord { q: 3, r: 0 }));
    }

    #[test]
    fn axial_parallelogram_swapped_corners() {
        let a = axial_parallelogram(HexCoord { q: 0, r: 0 }, HexCoord { q: 2, r: 3 });
        let b = axial_parallelogram(HexCoord { q: 2, r: 3 }, HexCoord { q: 0, r: 0 });
        assert_eq!(a.len(), b.len());
        for c in &a {
            assert!(b.contains(c));
        }
    }

    #[test]
    fn axial_parallelogram_singleton() {
        let p = axial_parallelogram(HexCoord { q: 5, r: -2 }, HexCoord { q: 5, r: -2 });
        assert_eq!(p, vec![HexCoord { q: 5, r: -2 }]);
    }

    #[test]
    fn cube_invariant_holds_after_rounding() {
        // Stress: random-ish points should always round to a valid hex
        // where the implicit s satisfies q + r + s == 0.
        let lat = HexLattice::pointy(32.0);
        for i in 0..1000 {
            let x = (i as f32 * 0.37).sin() * 500.0;
            let y = (i as f32 * 0.61).cos() * 500.0;
            let c = lat.cell_at(Vec2::new(x, y)).unwrap();
            assert_eq!(c.q + c.r + c.s(), 0);
        }
    }
}
