//! Entry points for the Carbide UI — one binary, two targets.
//!
//! Native (`cargo run`) opens a desktop window via `eframe::run_native`.
//! WebAssembly (`trunk build`) mounts the same app onto an HTML canvas via
//! `eframe::WebRunner`. The application code in `lib.rs` is identical for
//! both — that shared codebase is the whole point of going pure-Rust.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([520.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Carbide",
        options,
        Box::new(|cc| Ok(Box::new(carbide_ui::CarbideApp::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        let canvas = eframe::web_sys::window()
            .expect("no browser window")
            .document()
            .expect("no document")
            .get_element_by_id("carbide_canvas")
            .expect("index.html is missing the #carbide_canvas element")
            .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
            .expect("#carbide_canvas is not a <canvas>");
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(carbide_ui::CarbideApp::new(cc)))),
            )
            .await
            .expect("failed to start the Carbide UI");
    });
}
