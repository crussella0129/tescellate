//! Native compile pipeline — turns a transpiled Carbide formula into a
//! compiled `cdylib` and calls it in-process via `libloading`. See
//! `docs/all-rust-roadmap.md` v5.
//!
//! Feature-gated (`native`) because it pulls in `libloading` and shells
//! out to `cargo` at runtime.
//!
//! ## Soundness
//!
//! The compiled `cdylib` and the host exchange Rust types (`&dyn EvalCtx`,
//! `Result<CellValue, EvalError>`) across the library boundary. Rust does
//! not *guarantee* a stable ABI, so this is sound only when the cdylib is
//! built with the same rustc and the same `tescellate-formula` source as
//! the host. The pipeline ensures exactly that: the generated crate
//! path-depends on this very crate and is built by the toolchain that is
//! running. This is the accepted tradeoff noted in PLAN.md §12 — native
//! compiled formulas run in-process.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;

use crate::excellite::ast::Expr;
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

/// The exported symbol every generated cdylib carries (NUL-terminated for
/// `libloading`).
const SYMBOL: &[u8] = b"carbide_formula\0";

/// Failure compiling or loading a native formula.
#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    #[error("native compile: io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("native compile: the generated cdylib failed to build:\n{0}")]
    Compile(String),
    #[error("native compile: failed to load the cdylib: {0}")]
    Load(String),
}

/// A Carbide formula compiled to a native dynamic library and loaded into
/// this process. Evaluating it is a direct call into compiled machine code.
pub struct NativeFormula {
    lib: libloading::Library,
}

impl NativeFormula {
    /// Evaluate the compiled formula against `ctx`.
    pub fn eval(&self, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
        type CarbideFormulaFn = unsafe fn(&dyn EvalCtx) -> Result<CellValue, EvalError>;
        // SAFETY: `lib` is a cdylib this process compiled moments ago from
        // this crate's own source with the running rustc, so the Rust ABI
        // of `carbide_formula` matches this signature (see the module note).
        unsafe {
            let func: libloading::Symbol<CarbideFormulaFn> = self
                .lib
                .get(SYMBOL)
                .expect("carbide_formula symbol present in a cdylib we built");
            (*func)(ctx)
        }
    }
}

/// Transpile `expr`, compile it to a `cdylib`, and load it.
pub fn compile(expr: &Expr) -> Result<NativeFormula, NativeError> {
    let body = super::transpile_expr(expr);

    // A per-formula directory keyed by a content hash: parallel compiles of
    // different formulas never collide, and each keeps its own `target/`,
    // so cargo's own caching makes an unchanged rebuild a near-no-op.
    let mut hasher = DefaultHasher::new();
    body.hash(&mut hasher);
    let key = hasher.finish();
    let dir = std::env::temp_dir().join(format!("tescellate_native_{key:016x}"));
    std::fs::create_dir_all(dir.join("src"))?;

    let formula_crate = env!("CARGO_MANIFEST_DIR"); // .../crates/tescellate-formula
    let lib_rs = format!(
        "#![allow(unused_variables)]\n\
         use tescellate_formula::transpile::rt::*;\n\n\
         #[no_mangle]\n\
         pub fn carbide_formula(ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {{\n    \
         Ok({body})\n}}\n"
    );
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
    Ok(NativeFormula { lib })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excellite::parse::parse;
    use crate::transpile::MapCtx;

    #[test]
    fn compiles_and_runs_arithmetic() {
        let f = compile(&parse("1 + 2 * 3").unwrap()).expect("compile arithmetic");
        assert_eq!(f.eval(&MapCtx::default()).unwrap(), CellValue::Number(7.0));
    }

    #[test]
    fn compiles_and_runs_with_cell_refs() {
        let f = compile(&parse("A1 + A2").unwrap()).expect("compile cell-ref formula");
        let ctx = MapCtx::from_pairs(&[
            ("A1", CellValue::Number(10.0)),
            ("A2", CellValue::Number(5.0)),
        ]);
        assert_eq!(f.eval(&ctx).unwrap(), CellValue::Number(15.0));
    }

    #[test]
    fn compiles_and_runs_a_function_call() {
        let f = compile(&parse("SUM(1, 2, 3, 4)").unwrap()).expect("compile SUM");
        assert_eq!(f.eval(&MapCtx::default()).unwrap(), CellValue::Number(10.0));
    }
}
