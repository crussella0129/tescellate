//! Native compile pipeline — compiles transpiled Carbide into a native
//! `cdylib` and calls it in-process via `libloading`. See
//! `docs/all-rust-roadmap.md` v5.
//!
//! Feature-gated (`native`) because it pulls in `libloading` and shells
//! out to `cargo` at runtime.
//!
//! A [`NativeProgram`] holds one or more formulas compiled together into a
//! single library. [`compile_program`] is backed by a process-wide cache
//! keyed on the transpiled source, so an identical formula set compiles
//! once; on disk, the per-content build directory plus cargo's own
//! incremental tracking handle cross-process reuse and rustc-change
//! invalidation.
//!
//! ## Soundness
//!
//! The cdylib and the host exchange Rust types (`&dyn EvalCtx`,
//! `Result<CellValue, EvalError>`) across the library boundary. Rust does
//! not *guarantee* a stable ABI, so this is sound only when the cdylib is
//! built with the same rustc and the same `tescellate-formula` source as
//! the host. The pipeline ensures exactly that: the generated crate
//! path-depends on this very crate and is built by the toolchain that is
//! running. This is the accepted tradeoff noted in PLAN.md §12 — native
//! compiled formulas run in-process.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use crate::excellite::ast::Expr;
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

/// Failure compiling or loading a native program.
#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    #[error("native compile: io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("native compile: the generated cdylib failed to build:\n{0}")]
    Compile(String),
    #[error("native compile: failed to load the cdylib: {0}")]
    Load(String),
}

/// One or more Carbide formulas compiled together into a native dynamic
/// library and loaded into this process. Evaluating a formula is a direct
/// call into compiled machine code.
pub struct NativeProgram {
    lib: libloading::Library,
    count: usize,
}

impl NativeProgram {
    /// Number of compiled formulas; valid indices are `0..len()`.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the program holds no formulas.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Evaluate the `index`-th compiled formula against `ctx`.
    pub fn eval(&self, index: usize, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
        assert!(
            index < self.count,
            "formula index {index} out of range (program has {} formulas)",
            self.count,
        );
        type FormulaFn = unsafe fn(&dyn EvalCtx) -> Result<CellValue, EvalError>;
        let symbol = format!("carbide_formula_{index}\0");
        // SAFETY: `lib` is a cdylib this process compiled from this crate's
        // own source with the running rustc, so the Rust ABI of the
        // `carbide_formula_*` functions matches this signature.
        unsafe {
            let func: libloading::Symbol<FormulaFn> = self
                .lib
                .get(symbol.as_bytes())
                .expect("carbide_formula_<index> symbol present in a cdylib we built");
            (*func)(ctx)
        }
    }
}

/// Process-wide cache: a program compiled once is reused for an identical
/// formula set. Keyed by a content hash of the transpiled bodies.
fn cache() -> &'static Mutex<HashMap<u64, Arc<NativeProgram>>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<NativeProgram>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Transpile `exprs`, compile them together into one `cdylib`, and load it.
/// An identical formula set returns the cached program without recompiling.
pub fn compile_program(exprs: &[&Expr]) -> Result<Arc<NativeProgram>, NativeError> {
    let bodies: Vec<String> = exprs.iter().map(|e| super::transpile_expr(e)).collect();

    let mut hasher = DefaultHasher::new();
    for body in &bodies {
        body.hash(&mut hasher);
    }
    let key = hasher.finish();

    if let Some(hit) = cache().lock().unwrap().get(&key).cloned() {
        return Ok(hit);
    }
    // The build runs outside the lock — concurrent compiles of *different*
    // formula sets never serialize against each other.
    let program = Arc::new(build_program(key, &bodies)?);
    cache().lock().unwrap().insert(key, program.clone());
    Ok(program)
}

/// Generate, compile, and load the cdylib for a set of transpiled bodies.
fn build_program(key: u64, bodies: &[String]) -> Result<NativeProgram, NativeError> {
    // A per-content directory: parallel compiles of different formula sets
    // never collide, and each keeps its own `target/`, so cargo's caching
    // makes an unchanged rebuild a near-no-op.
    let dir = std::env::temp_dir().join(format!("tescellate_native_{key:016x}"));
    std::fs::create_dir_all(dir.join("src"))?;

    let formula_crate = env!("CARGO_MANIFEST_DIR"); // .../crates/tescellate-formula
    let mut lib_rs =
        String::from("#![allow(unused_variables)]\nuse tescellate_formula::transpile::rt::*;\n\n");
    for (i, body) in bodies.iter().enumerate() {
        lib_rs.push_str(&format!(
            "#[no_mangle]\n\
             pub fn carbide_formula_{i}(ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {{\n    \
             Ok({body})\n}}\n\n"
        ));
    }
    let cargo_toml = format!(
        "[package]\n\
         name = \"transpile_native\"\n\
         version = \"0.0.0\"\n\
         edition = \"2021\"\n\n\
         [lib]\n\
         crate-type = [\"cdylib\"]\n\n\
         [dependencies]\n\
         tescellate-formula = {{ path = {formula_crate:?} }}\n\n\
         [workspace]\n"
    );
    std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;
    std::fs::write(dir.join("src/lib.rs"), lib_rs)?;

    let out = Command::new("cargo")
        .args(["build", "--release", "--quiet"])
        .current_dir(&dir)
        .output()?;
    if !out.status.success() {
        return Err(NativeError::Compile(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }

    let lib_path = dir.join("target/release").join(format!(
        "{}transpile_native{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX,
    ));
    // SAFETY: loading a cdylib this process just compiled from its own source.
    let lib = unsafe { libloading::Library::new(&lib_path) }
        .map_err(|e| NativeError::Load(format!("{}: {e}", lib_path.display())))?;
    Ok(NativeProgram {
        lib,
        count: bodies.len(),
    })
}
