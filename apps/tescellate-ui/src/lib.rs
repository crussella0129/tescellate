//! Tescellate's pure-Rust front-end — an egui/eframe application that
//! builds both natively and to WebAssembly.
//!
//! The Rust core (`tescellate-core`, `tescellate-tess`,
//! `tescellate-formula`) is consumed directly as library crates,
//! in-process — there is no JSON-RPC boundary and no subprocess, unlike
//! the Electron front-end this replaces.

mod app;
pub mod clipboard;
pub mod conditional;
pub mod find;
pub mod format;
pub mod formula_mode;
pub mod grid;
pub mod history;
pub mod keymap;
pub mod note;
pub mod ribbon;
pub mod selection;
pub mod sort;
pub mod stats;
pub mod widget;

pub use app::TescellateApp;
