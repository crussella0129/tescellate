# Agent Tasks (Persistent Backlog)

- [ ] T-016 (sprint 0): Open + merge PR `webui-v144-tscl-persistence` — touches: (git)
- [ ] T-101 (sprint 1): UI dialog wiring — rfd dep, `Command::{Save,SaveAs,Open}`, async save/open flow on wasm+native, ribbon File group buttons — touches: apps/tescellate-ui/{Cargo.toml,src/keymap.rs,src/app.rs,src/ribbon.rs}
- [ ] T-102 (sprint 1): localStorage autosave — `state_io::{autosave_to_local_storage, load_from_local_storage}`, base64 dep, dirty-flag + 2s debounce, rehydrate-on-boot before seed demos — touches: apps/tescellate-ui/{Cargo.toml,src/state_io.rs,src/app.rs}
