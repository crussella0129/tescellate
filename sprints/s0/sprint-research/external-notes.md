# External-source notes (Sprint 0)

## web-sys Blob download pattern

```rust
use wasm_bindgen::JsCast;
use web_sys::{Blob, HtmlAnchorElement, Url};

fn trigger_download(bytes: &[u8], filename: &str) -> Result<(), JsValue> {
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array);
    let blob = Blob::new_with_u8_array_sequence(&parts)?;
    let url = Url::create_object_url_with_blob(&blob)?;
    let document = web_sys::window().unwrap().document().unwrap();
    let a: HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    a.set_href(&url);
    a.set_download(filename);
    a.click();
    Url::revoke_object_url(&url)?;
    Ok(())
}
```

Browser security: `a.click()` must happen inside the synchronous handler of a
user gesture event. egui passes input through `InputState`, and `update()` is
called in response to a redraw triggered by a user-gesture event, so the call
stack still counts. Confirmed by inspection of similar projects (`eframe`'s own
file-drop example).

## rfd async path

`rfd::AsyncFileDialog::new().save_file().await` resolves to `Option<FileHandle>`
on every backend. On wasm, the handle wraps a `Blob`; on native, a path. The
`FileHandle::write` helper accepts `&[u8]` on both backends. Same for
`pick_file().await` and `FileHandle::read`. Means our save/open code is shared
across wasm and native — the only `#[cfg]` is at the `spawn_local` vs.
`block_on` choice for entering the async.

## localStorage quota

- Chrome / Edge: ~10 MiB per origin (split across all keys).
- Firefox: 5 MiB.
- Safari: 5 MiB.

Practical ceiling for autosave: 4 MiB of base64 = ~3 MiB binary. A `.tscl` with
~10K cells of text-and-numbers compresses to well under 100 KiB, so the cap is
only relevant for pathological workbooks; we surface a toast and skip the
autosave when over.

## Wasm-compatible zip stack

Verified the dependency chain:

- `zip = "*"` (current pin uses `deflate-flate2` feature by default)
- `flate2 = "*"` with `rust_backend` selects `miniz_oxide` — pure Rust.

No `libz-sys`, no `bzip2-sys`. Confirmed by the existing build: `wasm32` already
links carbide-store's deps transitively via carbide-core via
carbide-formula. (Cross-check on first build of the sprint.)
