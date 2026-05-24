//! `carbide-cli` (becomes `carbide-cli` in v161) — placeholder binary.
//!
//! Historically this was the JSON-RPC server driven by the Electron renderer
//! (`apps/desktop`). Both the Electron app and the JSON-RPC IPC crate were
//! removed in v161 (ADR-013 native-first, ADR-015 stale-app cleanup); the
//! egui-native `apps/carbide-ui` now owns the user-facing surface.
//!
//! The crate is kept as a structural entry point for future headless
//! commands (e.g. evaluate a single formula, validate a workbook file,
//! run a batch recompute). For now it just prints a usage stub and exits.

fn main() {
    println!(
        "carbide (cli) — no headless commands implemented yet.\n\
         Use the workbook GUI: `cargo run --manifest-path apps/carbide-ui/Cargo.toml`."
    );
}
