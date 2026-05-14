//! `.tscl` file format I/O. See PLAN.md §9.
//!
//! Phase 0 placeholder — Phase 1 will pull in `zip` and wire up
//! `open` / `save` with the layout documented in the plan.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("format: {0}")]
    Format(String),
}

/// Format version recorded in `manifest.json`. Bump on any breaking change
/// to the on-disk schema; add an upgrade path before merging the bump.
pub const FORMAT_VERSION: u32 = 0;
