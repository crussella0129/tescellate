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

    fn address(&self, _c: Self::Coord) -> String {
        // TODO(phase-1): A1 / AB42 encoding.
        unimplemented!("square address encoding lands in Phase 1")
    }

    fn parse_address(&self, _s: &str) -> Result<Self::Coord, AddressError> {
        unimplemented!("square address parsing lands in Phase 1")
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
}
