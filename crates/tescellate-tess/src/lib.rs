//! Tessellation lattices for Tescellate.
//!
//! See `PLAN.md` §3 for the full design. This crate currently exposes only
//! the `Lattice` trait and the `LatticeKind` enum; concrete implementations
//! land in Phase 1 (square) and beyond.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;

pub mod square;

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
