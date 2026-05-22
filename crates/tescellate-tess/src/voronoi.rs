//! Voronoi lattice — Phase 3's first non-uniform tiling.
//!
//! A Voronoi diagram partitions a bounded rectangle into one polygon per
//! seed point, with each polygon containing every point closer to its
//! seed than to any other. Unlike `SquareLattice` / `HexLattice` /
//! `TriangleLattice` — which are uniform tilings parametrised only by a
//! cell size — `VoronoiLattice` carries its **seed configuration** as
//! part of its state. The seeds *are* the lattice.
//!
//! Address syntax: `V(N)` where `N` is the zero-based seed index. The
//! `default()` configuration ships eight seeds inside a 400×400 box
//! centred at the origin — picked to give a clearly-non-rectangular
//! demo at launch.
//!
//! ## Bounded cells
//! A real Voronoi cell can be unbounded (e.g. an edge cell on the
//! convex hull of the seeds). This impl clips every cell against the
//! lattice's `bounds: Rect`, so every cell is a finite convex polygon.
//! This is correct for the "Static Voronoi" demo the launch brief
//! describes; unbounded cells are out of scope.
//!
//! ## Vertex computation — Sutherland-Hodgman half-plane clipping
//! For each seed `S`, start with the four corners of `bounds`; for each
//! other seed `T`, clip the polygon against the half-plane
//! `{p : ‖p − S‖ ≤ ‖p − T‖}` (the half-plane separated by the
//! perpendicular bisector of ST, containing S). The remaining polygon
//! IS the Voronoi cell of S. Brute-force O(N²) per cell — fine for the
//! small N (≤ 15) of the launch demo.

use crate::{AddressError, Direction, Lattice, LatticeKind, Point2, Rect};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Zero-based seed index. Stored as `u32` to keep the address space
/// compact; in practice the launch demo uses N ≤ 15 seeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VoronoiCoord(pub u32);

/// A Voronoi lattice — a fixed set of seed points in a bounded
/// rectangle, plus the geometry to compute the cell polygon around
/// each seed.
#[derive(Debug, Clone)]
pub struct VoronoiLattice {
    pub seeds: Vec<Point2>,
    pub bounds: Rect,
}

impl VoronoiLattice {
    /// Build a Voronoi lattice from `seeds` clipped against `bounds`.
    /// Errors if `seeds` is empty or contains coincident points (the
    /// perpendicular bisector of coincident seeds is degenerate).
    pub fn new(seeds: Vec<Point2>, bounds: Rect) -> Result<Self, AddressError> {
        if seeds.is_empty() {
            return Err(AddressError::Parse("voronoi: empty seed list".into()));
        }
        for i in 0..seeds.len() {
            for j in (i + 1)..seeds.len() {
                if (seeds[i] - seeds[j]).length_squared() < 1e-6 {
                    return Err(AddressError::Parse(format!(
                        "voronoi: seeds {i} and {j} are coincident"
                    )));
                }
            }
        }
        if !bounds.min.x.is_finite()
            || !bounds.min.y.is_finite()
            || !bounds.max.x.is_finite()
            || !bounds.max.y.is_finite()
            || bounds.min.x >= bounds.max.x
            || bounds.min.y >= bounds.max.y
        {
            return Err(AddressError::Parse("voronoi: bounds are degenerate".into()));
        }
        Ok(Self { seeds, bounds })
    }

    /// How many seeds this lattice holds.
    pub fn len(&self) -> usize {
        self.seeds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seeds.is_empty()
    }

    /// Per-seed Delaunay neighbor sets: `adjacency[i]` is the ascending
    /// list of seed indices whose Voronoi cells share an edge with seed
    /// `i` (the Delaunay graph is the dual of the Voronoi diagram).
    ///
    /// `delaunator` needs ≥3 non-collinear points; for a degenerate
    /// config (<3 seeds, or all-collinear → empty triangulation) we fall
    /// back to every-other-seed, preserving the pre-v157 contract for
    /// those edge cases.
    fn delaunay_adjacency(&self) -> Vec<Vec<u32>> {
        let n = self.seeds.len();
        let mut adj: Vec<std::collections::BTreeSet<u32>> = vec![Default::default(); n];

        if n >= 3 {
            let points: Vec<delaunator::Point> = self
                .seeds
                .iter()
                .map(|s| delaunator::Point {
                    x: s.x as f64,
                    y: s.y as f64,
                })
                .collect();
            let tri = delaunator::triangulate(&points);
            for t in tri.triangles.chunks_exact(3) {
                let (a, b, c) = (t[0] as u32, t[1] as u32, t[2] as u32);
                for (u, v) in [(a, b), (b, c), (c, a)] {
                    adj[u as usize].insert(v);
                    adj[v as usize].insert(u);
                }
            }
        }

        // Degenerate fallback: no triangulation produced any edge.
        let no_edges = adj.iter().all(|s| s.is_empty());
        if no_edges && n > 1 {
            return (0..n)
                .map(|i| (0..n as u32).filter(|&j| j != i as u32).collect())
                .collect();
        }

        adj.into_iter().map(|s| s.into_iter().collect()).collect()
    }
}

impl Default for VoronoiLattice {
    /// An 8-seed configuration spread across a 400×400 box centred at
    /// the origin — the launch-demo defaults. Seeds picked irregularly
    /// so the Voronoi cells are visually varied; nothing magic about
    /// the specific positions.
    fn default() -> Self {
        let seeds = vec![
            Vec2::new(-140.0, -120.0),
            Vec2::new(60.0, -150.0),
            Vec2::new(150.0, -40.0),
            Vec2::new(-80.0, 30.0),
            Vec2::new(20.0, 50.0),
            Vec2::new(130.0, 110.0),
            Vec2::new(-160.0, 90.0),
            Vec2::new(-30.0, 150.0),
        ];
        let bounds = Rect {
            min: Vec2::new(-200.0, -200.0),
            max: Vec2::new(200.0, 200.0),
        };
        Self::new(seeds, bounds).expect("default Voronoi seeds are valid by construction")
    }
}

/// Serde-stable, persistable description of a Voronoi lattice's geometry.
///
/// Plain old data (no `glam` types) so it serializes into `workbook.json`
/// without depending on glam's `serde` feature and stays human-reviewable.
/// `bounds` is `[min_x, min_y, max_x, max_y]`. Stored on the `Sheet`
/// (see `tescellate-core`) so dragged seeds persist and the engine's
/// eval-time lattice matches the UI (ADR-011 / ADR-012).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoronoiConfig {
    pub seeds: Vec<[f32; 2]>,
    pub bounds: [f32; 4],
}

impl VoronoiConfig {
    /// Build a live `VoronoiLattice` from this config. Delegates to
    /// [`VoronoiLattice::new`], so coincident seeds / degenerate bounds
    /// are rejected here exactly as they are at construction time.
    pub fn to_lattice(&self) -> Result<VoronoiLattice, AddressError> {
        let seeds = self.seeds.iter().map(|&[x, y]| Vec2::new(x, y)).collect();
        let bounds = Rect {
            min: Vec2::new(self.bounds[0], self.bounds[1]),
            max: Vec2::new(self.bounds[2], self.bounds[3]),
        };
        VoronoiLattice::new(seeds, bounds)
    }
}

impl From<&VoronoiLattice> for VoronoiConfig {
    fn from(l: &VoronoiLattice) -> Self {
        VoronoiConfig {
            seeds: l.seeds.iter().map(|s| [s.x, s.y]).collect(),
            bounds: [
                l.bounds.min.x,
                l.bounds.min.y,
                l.bounds.max.x,
                l.bounds.max.y,
            ],
        }
    }
}

/// Per-lattice persisted configuration carried on a `Sheet`. Today only
/// Voronoi needs one (the uniform tilings are fully described by their
/// `LatticeKind`); new variants land here if a future lattice grows
/// per-sheet state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatticeConfig {
    Voronoi(VoronoiConfig),
}

/// Parse a `V(N)` address into a `VoronoiCoord`. Whitespace inside the
/// parens is tolerated; the seed count is bounded by `len`.
pub(crate) fn parse_voronoi(addr: &str, len: usize) -> Result<VoronoiCoord, AddressError> {
    let s = addr.trim();
    let body = s
        .strip_prefix("V(")
        .or_else(|| s.strip_prefix("v("))
        .ok_or_else(|| AddressError::Parse(format!("not a Voronoi address: {addr}")))?
        .strip_suffix(')')
        .ok_or_else(|| AddressError::Parse(format!("missing closing paren: {addr}")))?
        .trim();
    let i: u32 = body
        .parse()
        .map_err(|_| AddressError::Parse(format!("bad seed index: {body}")))?;
    if (i as usize) >= len {
        return Err(AddressError::OutOfRange);
    }
    Ok(VoronoiCoord(i))
}

pub(crate) fn format_voronoi(c: VoronoiCoord) -> String {
    format!("V({})", c.0)
}

impl Lattice for VoronoiLattice {
    type Coord = VoronoiCoord;

    fn kind(&self) -> LatticeKind {
        LatticeKind::Voronoi
    }

    fn address(&self, c: Self::Coord) -> String {
        format_voronoi(c)
    }

    fn parse_address(&self, s: &str) -> Result<Self::Coord, AddressError> {
        parse_voronoi(s, self.seeds.len())
    }

    fn vertices(&self, c: Self::Coord) -> SmallVec<[Point2; 8]> {
        let i = c.0 as usize;
        if i >= self.seeds.len() {
            return SmallVec::new();
        }
        let s = self.seeds[i];

        // Start with the bounding rectangle as a CCW polygon.
        let mut poly: Vec<Point2> = vec![
            Vec2::new(self.bounds.min.x, self.bounds.min.y),
            Vec2::new(self.bounds.max.x, self.bounds.min.y),
            Vec2::new(self.bounds.max.x, self.bounds.max.y),
            Vec2::new(self.bounds.min.x, self.bounds.max.y),
        ];

        // Clip against each other seed's bisector.
        for (j, &t) in self.seeds.iter().enumerate() {
            if j == i {
                continue;
            }
            poly = clip_halfplane(&poly, s, t);
            if poly.is_empty() {
                break;
            }
        }

        poly.into_iter().collect()
    }

    fn centroid(&self, c: Self::Coord) -> Point2 {
        let i = c.0 as usize;
        if i < self.seeds.len() {
            self.seeds[i]
        } else {
            // Degenerate fall-through; callers shouldn't pass out-of-range
            // coords, but returning the origin is safer than panicking.
            Vec2::ZERO
        }
    }

    fn neighbors(&self, c: Self::Coord) -> SmallVec<[(Direction, Self::Coord); 8]> {
        // Delaunay adjacency (v157): two seeds neighbor iff their Voronoi
        // cells share an edge, i.e. they share a Delaunay-triangulation
        // edge (the Delaunay graph is the dual of the Voronoi diagram).
        // Direction labels have no natural meaning on a Voronoi diagram;
        // `Direction::N` is a placeholder. Neighbors come back in
        // ascending seed-index order for determinism.
        let i = c.0 as usize;
        let mut out: SmallVec<[(Direction, VoronoiCoord); 8]> = SmallVec::new();
        if i >= self.seeds.len() {
            return out;
        }
        let adjacency = self.delaunay_adjacency();
        for &j in &adjacency[i] {
            out.push((Direction::N, VoronoiCoord(j)));
        }
        out
    }

    fn cell_at(&self, p: Point2) -> Option<Self::Coord> {
        if p.x < self.bounds.min.x
            || p.x > self.bounds.max.x
            || p.y < self.bounds.min.y
            || p.y > self.bounds.max.y
        {
            return None;
        }
        let (mut best, mut best_d2) = (0u32, f32::INFINITY);
        for (i, seed) in self.seeds.iter().enumerate() {
            let d2 = (p - *seed).length_squared();
            if d2 < best_d2 {
                best_d2 = d2;
                best = i as u32;
            }
        }
        Some(VoronoiCoord(best))
    }
}

/// Clip `poly` against the half-plane `{p : ‖p − s‖ ≤ ‖p − t‖}` — i.e.
/// the side containing `s`. Sutherland-Hodgman: walk the polygon edges,
/// emitting endpoints that are inside and intersection points where the
/// edge crosses the bisector. Returns the new polygon (empty when the
/// half-plane misses `poly` entirely).
fn clip_halfplane(poly: &[Point2], s: Point2, t: Point2) -> Vec<Point2> {
    // The bisector is `(p - mid) · n = 0` where `mid = (s + t) / 2` and
    // `n = t - s`. A point is "inside" (closer to s) when
    // `(p - mid) · n <= 0`.
    let mid = (s + t) * 0.5;
    let n = t - s;
    let signed = |p: Point2| (p - mid).dot(n);

    let mut out: Vec<Point2> = Vec::with_capacity(poly.len() + 1);
    if poly.is_empty() {
        return out;
    }
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = signed(prev) <= 0.0;
    for &curr in poly {
        let curr_in = signed(curr) <= 0.0;
        match (prev_in, curr_in) {
            (true, true) => out.push(curr),
            (true, false) => {
                // Going from inside to outside — emit intersection.
                if let Some(x) = intersect(prev, curr, mid, n) {
                    out.push(x);
                }
            }
            (false, true) => {
                // Going from outside to inside — emit intersection + endpoint.
                if let Some(x) = intersect(prev, curr, mid, n) {
                    out.push(x);
                }
                out.push(curr);
            }
            (false, false) => {} // both out — emit nothing
        }
        prev = curr;
        prev_in = curr_in;
    }
    out
}

/// Intersection of edge `a→b` with the line `(p - mid) · n = 0`.
/// Returns `None` when the edge is parallel to the line.
fn intersect(a: Point2, b: Point2, mid: Point2, n: Point2) -> Option<Point2> {
    let denom = (b - a).dot(n);
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = (mid - a).dot(n) / denom;
    Some(a + (b - a) * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> VoronoiLattice {
        let seeds = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(10.0, 10.0),
        ];
        let bounds = Rect {
            min: Vec2::new(-10.0, -10.0),
            max: Vec2::new(20.0, 20.0),
        };
        VoronoiLattice::new(seeds, bounds).unwrap()
    }

    fn bounds() -> Rect {
        Rect {
            min: Vec2::new(-10.0, -10.0),
            max: Vec2::new(20.0, 20.0),
        }
    }

    fn neighbor_indices(v: &VoronoiLattice, i: u32) -> Vec<u32> {
        let mut out: Vec<u32> = v
            .neighbors(VoronoiCoord(i))
            .into_iter()
            .map(|(_, c)| c.0)
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn delaunay_neighbors_on_known_config() {
        // 4 square corners (0..3) + a centre seed (4). The centre breaks
        // the cocircular corners into 4 triangles, giving an unambiguous
        // Delaunay graph: centre ↔ all corners; each corner ↔ centre + its
        // two edge-adjacent corners (NOT the diagonal one).
        let seeds = vec![
            Vec2::new(0.0, 0.0),   // 0
            Vec2::new(10.0, 0.0),  // 1
            Vec2::new(0.0, 10.0),  // 2
            Vec2::new(10.0, 10.0), // 3
            Vec2::new(5.0, 5.0),   // 4 centre
        ];
        let v = VoronoiLattice::new(seeds, bounds()).unwrap();
        assert_eq!(neighbor_indices(&v, 4), vec![0, 1, 2, 3]);
        assert_eq!(neighbor_indices(&v, 0), vec![1, 2, 4]);
        assert_eq!(neighbor_indices(&v, 1), vec![0, 3, 4]);
        assert_eq!(neighbor_indices(&v, 2), vec![0, 3, 4]);
        assert_eq!(neighbor_indices(&v, 3), vec![1, 2, 4]);
    }

    #[test]
    fn delaunay_neighbors_two_seeds_fallback() {
        // <3 seeds → no triangulation → every-other-seed fallback.
        let seeds = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let v = VoronoiLattice::new(seeds, bounds()).unwrap();
        assert_eq!(neighbor_indices(&v, 0), vec![1]);
        assert_eq!(neighbor_indices(&v, 1), vec![0]);
    }

    #[test]
    fn delaunay_neighbors_collinear_fallback() {
        // 3 collinear seeds → empty triangulation → fallback.
        let seeds = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(10.0, 0.0),
        ];
        let v = VoronoiLattice::new(seeds, bounds()).unwrap();
        assert_eq!(neighbor_indices(&v, 0), vec![1, 2]);
        assert_eq!(neighbor_indices(&v, 1), vec![0, 2]);
        assert_eq!(neighbor_indices(&v, 2), vec![0, 1]);
    }

    #[test]
    fn voronoi_cell_at_returns_nearest_seed() {
        let v = small();
        // (1, 1) is closer to seed 0 (0,0) than to the others.
        assert_eq!(v.cell_at(Vec2::new(1.0, 1.0)), Some(VoronoiCoord(0)));
        // (9, 1) is closer to seed 1 (10,0).
        assert_eq!(v.cell_at(Vec2::new(9.0, 1.0)), Some(VoronoiCoord(1)));
        // (5, 9) is closer to seed 2 (0,10) than seed 3 (10,10)? distance
        // 25+1 vs 25+1 — tie at (5, 10); use (3, 9) to disambiguate
        // toward seed 2.
        assert_eq!(v.cell_at(Vec2::new(3.0, 9.0)), Some(VoronoiCoord(2)));
    }

    #[test]
    fn voronoi_cell_at_returns_none_outside_bounds() {
        let v = small();
        assert_eq!(v.cell_at(Vec2::new(100.0, 0.0)), None);
        assert_eq!(v.cell_at(Vec2::new(-100.0, 0.0)), None);
    }

    #[test]
    fn voronoi_vertices_are_inside_bounds() {
        let v = small();
        for i in 0..v.len() as u32 {
            let verts = v.vertices(VoronoiCoord(i));
            assert!(!verts.is_empty(), "seed {i} produced an empty cell");
            for vert in &verts {
                assert!(
                    vert.x >= v.bounds.min.x - 1e-3 && vert.x <= v.bounds.max.x + 1e-3,
                    "vertex {vert:?} of seed {i} is outside bounds x"
                );
                assert!(
                    vert.y >= v.bounds.min.y - 1e-3 && vert.y <= v.bounds.max.y + 1e-3,
                    "vertex {vert:?} of seed {i} is outside bounds y"
                );
            }
        }
    }

    #[test]
    fn voronoi_centroid_is_seed_point() {
        let v = small();
        for i in 0..v.len() as u32 {
            assert_eq!(v.centroid(VoronoiCoord(i)), v.seeds[i as usize]);
        }
    }

    #[test]
    fn voronoi_address_round_trips() {
        let v = small();
        for i in 0..v.len() as u32 {
            let c = VoronoiCoord(i);
            let s = v.address(c);
            assert_eq!(v.parse_address(&s).unwrap(), c);
        }
    }

    #[test]
    fn voronoi_address_out_of_range_errors() {
        let v = small();
        assert!(matches!(
            v.parse_address("V(99)"),
            Err(AddressError::OutOfRange)
        ));
    }

    #[test]
    fn voronoi_rejects_coincident_seeds() {
        let seeds = vec![Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)];
        let bounds = Rect {
            min: Vec2::new(-1.0, -1.0),
            max: Vec2::new(1.0, 1.0),
        };
        assert!(VoronoiLattice::new(seeds, bounds).is_err());
    }

    #[test]
    fn default_has_eight_seeds() {
        let v = VoronoiLattice::default();
        assert_eq!(v.len(), 8);
    }

    // --- T-001: VoronoiConfig / LatticeConfig ---

    #[test]
    fn config_to_lattice_happy() {
        let cfg = VoronoiConfig {
            seeds: vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]],
            bounds: [-10.0, -10.0, 20.0, 20.0],
        };
        let lat = cfg.to_lattice().expect("valid config builds a lattice");
        assert_eq!(lat.len(), 4);
        assert_eq!(lat.seeds[1], Vec2::new(10.0, 0.0));
        assert_eq!(lat.bounds.min, Vec2::new(-10.0, -10.0));
        assert_eq!(lat.bounds.max, Vec2::new(20.0, 20.0));
    }

    #[test]
    fn config_to_lattice_rejects_coincident() {
        let cfg = VoronoiConfig {
            seeds: vec![[1.0, 1.0], [1.0, 1.0]],
            bounds: [-5.0, -5.0, 5.0, 5.0],
        };
        assert!(cfg.to_lattice().is_err());
    }

    #[test]
    fn config_lattice_round_trip() {
        let lat = small();
        let cfg = VoronoiConfig::from(&lat);
        let back = cfg.to_lattice().unwrap();
        assert_eq!(back.seeds, lat.seeds);
        assert_eq!(back.bounds.min, lat.bounds.min);
        assert_eq!(back.bounds.max, lat.bounds.max);
    }

    #[test]
    fn lattice_config_json_round_trip() {
        let cfg = VoronoiConfig::from(&small());
        let lc = LatticeConfig::Voronoi(cfg);
        let json = serde_json::to_string(&lc).unwrap();
        let back: LatticeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lc);
    }
}
