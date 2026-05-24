//! Square lattice — Phase 1's first concrete implementation. Stub only;
//! address parsing and rendering land in Phase 1 proper.

use crate::{AddressError, Direction, Lattice, LatticeKind, Point2};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

/// Excel-compatible `(col, row)` coordinate. Both are 0-indexed internally;
/// the address parser converts to/from `A1`-style 1-indexed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SquareCoord {
    pub col: i32,
    pub row: i32,
}

pub struct SquareLattice {
    /// Cell side length in lattice units. Real cell rendering size is this
    /// times the camera zoom (zoom lives in the renderer, not here).
    pub cell_size: f32,
}

impl Default for SquareLattice {
    fn default() -> Self {
        Self { cell_size: 64.0 }
    }
}

impl Lattice for SquareLattice {
    type Coord = SquareCoord;

    fn kind(&self) -> LatticeKind {
        LatticeKind::Square
    }

    fn address(&self, c: Self::Coord) -> String {
        format_a1(c)
    }

    fn parse_address(&self, s: &str) -> Result<Self::Coord, AddressError> {
        parse_a1(s)
    }

    fn vertices(&self, c: Self::Coord) -> SmallVec<[Point2; 8]> {
        let s = self.cell_size;
        let x = c.col as f32 * s;
        let y = c.row as f32 * s;
        smallvec![
            Vec2::new(x, y),
            Vec2::new(x + s, y),
            Vec2::new(x + s, y + s),
            Vec2::new(x, y + s),
        ]
    }

    fn centroid(&self, c: Self::Coord) -> Point2 {
        let s = self.cell_size;
        Vec2::new(c.col as f32 * s + s * 0.5, c.row as f32 * s + s * 0.5)
    }

    fn neighbors(&self, c: Self::Coord) -> SmallVec<[(Direction, Self::Coord); 8]> {
        smallvec![
            (
                Direction::N,
                SquareCoord {
                    col: c.col,
                    row: c.row - 1
                }
            ),
            (
                Direction::E,
                SquareCoord {
                    col: c.col + 1,
                    row: c.row
                }
            ),
            (
                Direction::S,
                SquareCoord {
                    col: c.col,
                    row: c.row + 1
                }
            ),
            (
                Direction::W,
                SquareCoord {
                    col: c.col - 1,
                    row: c.row
                }
            ),
        ]
    }

    fn cell_at(&self, p: Point2) -> Option<Self::Coord> {
        let s = self.cell_size;
        Some(SquareCoord {
            col: (p.x / s).floor() as i32,
            row: (p.y / s).floor() as i32,
        })
    }
}

/// Format a square coord as Excel-style `A1`. Negative coords (only reachable
/// via internal viewport math) format with a leading minus sign on the row.
fn format_a1(c: SquareCoord) -> String {
    let mut letters = String::new();
    let mut n: i64 = c.col as i64;
    if n < 0 {
        // Negative column has no Excel-equivalent text; render bracketed.
        return format!("[c{},r{}]", c.col, c.row + 1);
    }
    loop {
        letters.insert(0, (b'A' + (n % 26) as u8) as char);
        n = n / 26 - 1;
        if n < 0 {
            break;
        }
    }
    format!("{letters}{}", c.row + 1)
}

fn parse_a1(s: &str) -> Result<SquareCoord, AddressError> {
    // Internal bracketed form, used when round-tripping negative coords.
    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let (cs, rs) = inner
            .split_once(',')
            .ok_or_else(|| AddressError::Parse(s.into()))?;
        let col: i32 = cs
            .strip_prefix('c')
            .ok_or_else(|| AddressError::Parse(s.into()))?
            .parse()
            .map_err(|_| AddressError::Parse(s.into()))?;
        let row1: i32 = rs
            .strip_prefix('r')
            .ok_or_else(|| AddressError::Parse(s.into()))?
            .parse()
            .map_err(|_| AddressError::Parse(s.into()))?;
        return Ok(SquareCoord { col, row: row1 - 1 });
    }

    let bytes = s.as_bytes();
    let split = bytes
        .iter()
        .position(|b| b.is_ascii_digit())
        .ok_or_else(|| AddressError::Parse(s.into()))?;
    if split == 0 {
        return Err(AddressError::Parse(s.into()));
    }
    let letters = &s[..split];
    let digits = &s[split..];

    let mut col: i64 = 0;
    for c in letters.chars() {
        if !c.is_ascii_uppercase() {
            return Err(AddressError::Parse(s.into()));
        }
        col = col * 26 + (c as i64 - 'A' as i64 + 1);
        if col > i32::MAX as i64 {
            return Err(AddressError::OutOfRange);
        }
    }
    let col = (col - 1) as i32;

    let row1: i32 = digits.parse().map_err(|_| AddressError::Parse(s.into()))?;
    if row1 < 1 {
        return Err(AddressError::OutOfRange);
    }
    Ok(SquareCoord { col, row: row1 - 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centroid_is_inside_vertices() {
        let lat = SquareLattice::default();
        let c = SquareCoord { col: 3, row: 5 };
        let centroid = lat.centroid(c);
        let verts = lat.vertices(c);
        let xs: Vec<f32> = verts.iter().map(|v| v.x).collect();
        let ys: Vec<f32> = verts.iter().map(|v| v.y).collect();
        let xmin = xs.iter().cloned().fold(f32::INFINITY, f32::min);
        let xmax = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let ymin = ys.iter().cloned().fold(f32::INFINITY, f32::min);
        let ymax = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(centroid.x > xmin && centroid.x < xmax);
        assert!(centroid.y > ymin && centroid.y < ymax);
    }

    #[test]
    fn cell_at_round_trips() {
        let lat = SquareLattice::default();
        let c = SquareCoord { col: 7, row: -2 };
        let centroid = lat.centroid(c);
        assert_eq!(lat.cell_at(centroid), Some(c));
    }

    #[test]
    fn a1_format_known_cases() {
        let cases = [
            (SquareCoord { col: 0, row: 0 }, "A1"),
            (SquareCoord { col: 25, row: 0 }, "Z1"),
            (SquareCoord { col: 26, row: 0 }, "AA1"),
            (SquareCoord { col: 27, row: 41 }, "AB42"),
            (SquareCoord { col: 51, row: 0 }, "AZ1"),
            (SquareCoord { col: 52, row: 0 }, "BA1"),
            (SquareCoord { col: 701, row: 0 }, "ZZ1"),
            (SquareCoord { col: 702, row: 0 }, "AAA1"),
        ];
        for (c, s) in cases {
            assert_eq!(format_a1(c), s, "format mismatch for {c:?}");
            assert_eq!(parse_a1(s).unwrap(), c, "parse mismatch for {s}");
        }
    }

    #[test]
    fn a1_round_trips() {
        for col in 0..1500i32 {
            for row in [0, 1, 41, 999] {
                let c = SquareCoord { col, row };
                let s = format_a1(c);
                assert_eq!(parse_a1(&s).unwrap(), c, "round trip failed at {s}");
            }
        }
    }

    #[test]
    fn a1_rejects_malformed() {
        let bad = ["", "1", "A", "1A", "AAQ", "A0", " A1", "A-1"];
        for s in bad {
            assert!(parse_a1(s).is_err(), "expected error for {s:?}");
        }
    }

    #[test]
    fn negative_coord_uses_internal_form() {
        let c = SquareCoord { col: -1, row: -3 };
        let s = format_a1(c);
        assert_eq!(parse_a1(&s).unwrap(), c);
    }
}
