/** Mirrors `tescellate-core::EngineKind` (snake_case via serde). */
export type EngineKind = 'excel_lite' | 'python' | 'rhai' | 'rust_native';

/** Mirrors `tescellate-tess::LatticeKind`. */
export type LatticeKind = 'square' | 'hex_pointy' | 'hex_flat' | 'triangle' | 'parallelogram';
