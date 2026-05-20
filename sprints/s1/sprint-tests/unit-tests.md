# Sprint 1 Unit Tests

## T-101b (keymap bindings)
- `save_open_keymap_bindings`: Ctrl+S → Save, Ctrl+Shift+S → SaveAs, Ctrl+O → Open; plain S/O → None. **pass**

## T-102a (autosave write)
- `autosave_to_local_storage_is_noop_on_native`: any payload (including oversize) returns without panic. **pass**

## T-102b (autosave read)
- `load_from_local_storage_returns_none_on_native`: native returns `None`. **pass**

## Deferred to manual E2E
- T-101c (ribbon emits Save/Open) — covered by browser-side click.
- T-101d/e/f Save/Open flows — dialog interactions; covered by §E2E.
- T-102c mark_dirty wiring — code review at commit time.
- T-102d maybe_autosave timing — covered by §E2E (edit + 2 s + F5).

## Run summary
- `cargo test --manifest-path apps/tescellate-ui/Cargo.toml --lib`: **246 passed, 0 failed** (243 carried over from v144 + 3 new).
- `cargo test --workspace`: all green.
