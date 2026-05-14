# tescellate-desktop

Electron + Vite + React + TypeScript shell for Tescellate. See top-level [`PLAN.md`](../../PLAN.md) §2 and §8 for the architecture.

## Dev

```sh
# from apps/desktop/
npm install
npm run build:rust   # builds the tescellate-core binary in target/debug/
npm run dev          # spawns Electron with HMR
```

The main process locates the core binary at `target/debug/tescellate-core[.exe]`. Packaging via electron-builder lands in Phase 4+; until then everything runs from the cargo target dir.

## Layout

```
apps/desktop/
├── electron/
│   ├── main.ts        # main process: window + core subprocess + IPC pump
│   └── preload.ts     # contextBridge — exposes `window.tescellate.coreRequest`
├── src/               # React renderer
│   ├── components/
│   │   ├── FormulaBar.tsx
│   │   └── GridCanvas.tsx   # placeholder; real renderer lands Phase 1
│   ├── App.tsx
│   └── types.ts       # TS mirrors of EngineKind / LatticeKind
├── index.html
├── electron.vite.config.ts
└── tsconfig{,.node,.web}.json
```

The renderer never touches Node or the core process directly — everything goes via `window.tescellate.coreRequest(...)` exposed by `preload.ts`.
