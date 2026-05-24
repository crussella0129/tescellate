//! Property-based tests for the tessellation lattices — v13.
//!
//! Carbide's defining feature is non-square tessellating cells: a sheet
//! can be a square grid or a hex grid (`carbide-tess`). The lattice math
//! — addressing, neighbours, ranges, radius discs — is geometry, and
//! geometry has invariants that hold for *every* cell, which is exactly
//! what property testing is for.
//!
//! `carbide-tess` has good unit coverage of fixed cases; this file adds
//! a generative gate over both lattices, targeting invariants the fixed
//! tests leave thin: address↔coordinate round-trips across wide coordinate
//! ranges, neighbour symmetry, range membership (not just length), and
//! radius monotonicity.
//!
//! Determinism is fixed-seed so a CI failure always reproduces. The lattice
//! API is exercised through `LatticeHandle`, the public string-level seam
//! that both the engine and the file format already go through.

use std::collections::HashSet;

use carbide_tess::hex::HexCoord;
use carbide_tess::square::SquareCoord;
use carbide_tess::{LatticeHandle, LatticeKind, ParsedCoord};

/// Fixed seed — a generative CI gate must reproduce byte-for-byte.
const SEED: u64 = 0x1A77_1CE0_F00D_5EED;
/// Random cells generated per property.
const ITERS: usize = 1000;

/// xorshift64 — a tiny, dependency-free, deterministic PRNG.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(if seed == 0 { 0xDEAD_BEEF } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }

    /// A value in `lo..=hi` (signed).
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1) as u32) as i32
    }
}

/// A lattice handle for `kind` — the public string-level API.
fn handle(kind: LatticeKind) -> LatticeHandle {
    LatticeHandle::for_kind(kind).expect("square and hex are always available")
}

/// A random square coordinate, well inside the non-negative addressable
/// region — far enough from the origin that a generated cell's neighbours
/// and radius discs stay on-grid. Negative square coordinates are an
/// internal off-grid representation, not part of the A1 addressing
/// contract — unlike hex, whose axial space is genuinely unbounded.
fn gen_square(rng: &mut Rng) -> SquareCoord {
    SquareCoord {
        col: rng.range(30, 300),
        row: rng.range(30, 300),
    }
}

/// A random axial hex coordinate, wide range including negatives.
fn gen_hex(rng: &mut Rng) -> HexCoord {
    HexCoord::new(rng.range(-80, 80), rng.range(-80, 80))
}

fn square_addr(rng: &mut Rng, h: &LatticeHandle) -> String {
    h.format_coord(ParsedCoord::Square(gen_square(rng)))
}

fn hex_addr(rng: &mut Rng, h: &LatticeHandle) -> String {
    h.format_coord(ParsedCoord::Hex(gen_hex(rng)))
}

#[test]
fn square_addresses_round_trip() {
    let h = handle(LatticeKind::Square);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let c = gen_square(&mut rng);
        let addr = h.format_coord(ParsedCoord::Square(c));
        match h.parse_coord(&addr) {
            Ok(ParsedCoord::Square(back)) => assert!(
                back.col == c.col && back.row == c.row,
                "square round-trip via {addr:?}: got ({}, {}), want ({}, {})",
                back.col,
                back.row,
                c.col,
                c.row,
            ),
            Ok(other) => panic!("square handle parsed {addr:?} as {other:?}"),
            Err(e) => panic!("square handle could not re-parse {addr:?}: {e:?}"),
        }
    }
}

#[test]
fn hex_addresses_round_trip() {
    let h = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let c = gen_hex(&mut rng);
        let addr = h.format_coord(ParsedCoord::Hex(c));
        match h.parse_coord(&addr) {
            Ok(ParsedCoord::Hex(back)) => assert!(
                back.q == c.q && back.r == c.r,
                "hex round-trip via {addr:?}: got ({}, {}), want ({}, {})",
                back.q,
                back.r,
                c.q,
                c.r,
            ),
            Ok(other) => panic!("hex handle parsed {addr:?} as {other:?}"),
            Err(e) => panic!("hex handle could not re-parse {addr:?}: {e:?}"),
        }
    }
}

#[test]
fn neighbour_count_is_fixed_per_lattice() {
    // A square cell has 4 edge-neighbours; a hex cell has 6 — for *every*
    // cell, the lattices being unbounded.
    let sq = handle(LatticeKind::Square);
    let hx = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let sa = square_addr(&mut rng, &sq);
        assert_eq!(
            sq.neighbor_addresses(&sa).unwrap().len(),
            4,
            "square cell {sa} should have 4 neighbours",
        );
        let ha = hex_addr(&mut rng, &hx);
        assert_eq!(
            hx.neighbor_addresses(&ha).unwrap().len(),
            6,
            "hex cell {ha} should have 6 neighbours",
        );
    }
}

/// `B` is a neighbour of `A` iff `A` is a neighbour of `B`.
fn assert_neighbours_symmetric(h: &LatticeHandle, a: &str) {
    let a_canon = h.canonicalize(a).unwrap();
    for b in h.neighbor_addresses(a).unwrap() {
        let back = h.neighbor_addresses(&b).unwrap();
        assert!(
            back.contains(&a_canon),
            "{a} has neighbour {b}, but {b} does not have neighbour {a}",
        );
    }
}

#[test]
fn neighbours_are_symmetric() {
    let sq = handle(LatticeKind::Square);
    let hx = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let sa = square_addr(&mut rng, &sq);
        assert_neighbours_symmetric(&sq, &sa);
        let ha = hex_addr(&mut rng, &hx);
        assert_neighbours_symmetric(&hx, &ha);
    }
}

#[test]
fn neighbours_are_one_step_away() {
    let sq = handle(LatticeKind::Square);
    let hx = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        // Square: an edge-neighbour differs by exactly one in one axis.
        let center = gen_square(&mut rng);
        let sa = sq.format_coord(ParsedCoord::Square(center));
        for n in sq.neighbor_addresses(&sa).unwrap() {
            match sq.parse_coord(&n).unwrap() {
                ParsedCoord::Square(nc) => {
                    let steps = (nc.col - center.col).abs() + (nc.row - center.row).abs();
                    assert_eq!(
                        steps, 1,
                        "square neighbour {n} of {sa} is not one step away"
                    );
                }
                other => panic!("square neighbour parsed as {other:?}"),
            }
        }
        // Hex: an edge-neighbour is at axial distance exactly 1.
        let hc = gen_hex(&mut rng);
        let ha = hx.format_coord(ParsedCoord::Hex(hc));
        for n in hx.neighbor_addresses(&ha).unwrap() {
            match hx.parse_coord(&n).unwrap() {
                ParsedCoord::Hex(nc) => assert_eq!(
                    hc.distance(nc),
                    1,
                    "hex neighbour {n} of {ha} is not one step away",
                ),
                other => panic!("hex neighbour parsed as {other:?}"),
            }
        }
    }
}

#[test]
fn square_range_enumerates_the_rectangle() {
    let h = handle(LatticeKind::Square);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let a = gen_square(&mut rng);
        // A nearby second corner keeps the rectangle small and fast.
        let b = SquareCoord {
            col: a.col + rng.range(-15, 15),
            row: a.row + rng.range(-15, 15),
        };
        let aa = h.format_coord(ParsedCoord::Square(a));
        let bb = h.format_coord(ParsedCoord::Square(b));
        let cells = h.enumerate_range(&aa, &bb).unwrap();

        let (min_c, max_c) = (a.col.min(b.col), a.col.max(b.col));
        let (min_r, max_r) = (a.row.min(b.row), a.row.max(b.row));
        let expected = ((max_c - min_c + 1) * (max_r - min_r + 1)) as usize;

        let set: HashSet<String> = cells.iter().cloned().collect();
        assert_eq!(
            set.len(),
            cells.len(),
            "range {aa}:{bb} contains duplicates"
        );
        assert_eq!(cells.len(), expected, "range {aa}:{bb} cell count");

        for addr in &cells {
            match h.parse_coord(addr).unwrap() {
                ParsedCoord::Square(c) => assert!(
                    c.col >= min_c && c.col <= max_c && c.row >= min_r && c.row <= max_r,
                    "range {aa}:{bb} produced out-of-rectangle cell {addr}",
                ),
                other => panic!("square range produced {other:?}"),
            }
        }
        assert!(
            set.contains(&h.canonicalize(&aa).unwrap())
                && set.contains(&h.canonicalize(&bb).unwrap()),
            "range {aa}:{bb} omits a corner",
        );
        // Corner order must not matter.
        let swapped: HashSet<String> = h.enumerate_range(&bb, &aa).unwrap().into_iter().collect();
        assert_eq!(
            swapped, set,
            "range {aa}:{bb} is not corner-order invariant"
        );
    }
}

#[test]
fn hex_range_enumerates_the_parallelogram() {
    let h = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let a = gen_hex(&mut rng);
        let b = HexCoord::new(a.q + rng.range(-12, 12), a.r + rng.range(-12, 12));
        let aa = h.format_coord(ParsedCoord::Hex(a));
        let bb = h.format_coord(ParsedCoord::Hex(b));
        let cells = h.enumerate_range(&aa, &bb).unwrap();

        let (min_q, max_q) = (a.q.min(b.q), a.q.max(b.q));
        let (min_r, max_r) = (a.r.min(b.r), a.r.max(b.r));
        let expected = ((max_q - min_q + 1) * (max_r - min_r + 1)) as usize;

        let set: HashSet<String> = cells.iter().cloned().collect();
        assert_eq!(
            set.len(),
            cells.len(),
            "range {aa}:{bb} contains duplicates"
        );
        assert_eq!(cells.len(), expected, "range {aa}:{bb} cell count");

        for addr in &cells {
            match h.parse_coord(addr).unwrap() {
                ParsedCoord::Hex(c) => assert!(
                    c.q >= min_q && c.q <= max_q && c.r >= min_r && c.r <= max_r,
                    "range {aa}:{bb} produced out-of-parallelogram cell {addr}",
                ),
                other => panic!("hex range produced {other:?}"),
            }
        }
        assert!(
            set.contains(&h.canonicalize(&aa).unwrap())
                && set.contains(&h.canonicalize(&bb).unwrap()),
            "range {aa}:{bb} omits a corner",
        );
        let swapped: HashSet<String> = h.enumerate_range(&bb, &aa).unwrap().into_iter().collect();
        assert_eq!(
            swapped, set,
            "range {aa}:{bb} is not corner-order invariant"
        );
    }
}

#[test]
fn radius_count_matches_the_lattice_formula() {
    // Square disc of radius r is a (2r+1)x(2r+1) block; hex disc of radius
    // r holds 1 + 3r(r+1) cells.
    let sq = handle(LatticeKind::Square);
    let hx = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let sa = square_addr(&mut rng, &sq);
        let ha = hex_addr(&mut rng, &hx);
        for r in 0i64..=4 {
            let sn = sq.cells_within_addresses(&sa, r).unwrap().len();
            assert_eq!(
                sn,
                ((2 * r + 1) * (2 * r + 1)) as usize,
                "square radius {r}"
            );
            let hn = hx.cells_within_addresses(&ha, r).unwrap().len();
            assert_eq!(hn, (1 + 3 * r * (r + 1)) as usize, "hex radius {r}");
        }
    }
}

#[test]
fn radius_discs_are_monotonic() {
    // A radius-r disc is contained in the radius-(r+1) disc.
    let sq = handle(LatticeKind::Square);
    let hx = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        for (h, addr) in [
            (&sq, square_addr(&mut rng, &sq)),
            (&hx, hex_addr(&mut rng, &hx)),
        ] {
            for r in 0i64..3 {
                let inner: HashSet<String> = h
                    .cells_within_addresses(&addr, r)
                    .unwrap()
                    .into_iter()
                    .collect();
                let outer: HashSet<String> = h
                    .cells_within_addresses(&addr, r + 1)
                    .unwrap()
                    .into_iter()
                    .collect();
                assert!(
                    inner.is_subset(&outer),
                    "{addr}: radius-{r} disc is not contained in radius-{}",
                    r + 1,
                );
            }
        }
    }
}

#[test]
fn hex_radius_one_disc_is_the_centre_plus_its_neighbours() {
    // For a hex cell the radius-1 disc is exactly the cell and its six
    // neighbours — a cross-check between the disc and neighbour code paths.
    let h = handle(LatticeKind::HexPointy);
    let mut rng = Rng::new(SEED);
    for _ in 0..ITERS {
        let addr = hex_addr(&mut rng, &h);
        let disc: HashSet<String> = h
            .cells_within_addresses(&addr, 1)
            .unwrap()
            .into_iter()
            .collect();
        let mut expected: HashSet<String> =
            h.neighbor_addresses(&addr).unwrap().into_iter().collect();
        expected.insert(h.canonicalize(&addr).unwrap());
        assert_eq!(
            disc, expected,
            "hex radius-1 disc ≠ centre ∪ neighbours for {addr}"
        );
    }
}
