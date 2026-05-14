//! `tescellate-core` binary — the headless driver that Electron spawns.
//!
//! Phase 0: serves the `ping` JSON-RPC method over stdio. Phase 1 wires
//! the full method set in PLAN.md §7.1.

fn main() -> std::io::Result<()> {
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout().lock();
    tescellate_ipc::serve(stdin, stdout)
}
