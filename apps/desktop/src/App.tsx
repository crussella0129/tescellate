import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { FormulaBar } from './components/FormulaBar';
import { GridCanvas, type SelectionRange } from './components/GridCanvas';
import { WizardModal } from './components/WizardModal';
import { createLatticeView, type LatticeView } from './lattice';
import type { EngineKind } from './types';
import { ipc, type CellSnapshot, type SheetInfo } from './ipc';

const CELL_SIZE = 96;

/** Per-lattice snapshot window. Square uses A1:Z100 (the existing
 * Phase 1 baseline). Hex sheets ask the core for an axial-parallelogram
 * surrounding the origin that's big enough to fill a typical viewport
 * (negative quadrants included). */
function snapshotWindow(lattice: LatticeView): { start: string; end: string } {
  if (lattice.kind === 'square') {
    return { start: 'A1', end: 'Z100' };
  }
  // Hex: an axial-aligned parallelogram from H(-20,-20) to H(20,40).
  // Wider in r than q since pointy-top stacks vertically. Anything
  // outside this returns null from get_cell; the renderer just shows
  // empty cells.
  return { start: 'H(-20,-20)', end: 'H(20,40)' };
}

/** Strip the trailing single-cell ref or range token from `text`,
 * understanding both square (`A1` / `A1:B5`) and hex (`H(q,r)` /
 * `H(q,r):H(q,r)`) syntax. Used by the formula bar when a drag
 * converts the just-inserted anchor into a full range. */
function stripTrailingRefOrRange(text: string): string {
  const re = /(?:H\(-?\d+,-?\d+\)|[A-Z]+[0-9]+)(?::(?:H\(-?\d+,-?\d+\)|[A-Z]+[0-9]+))?$/;
  return text.replace(re, '');
}

/** Build the canonical `start:end` range string for a SelectionRange. */
function rangeText(range: SelectionRange): string {
  return `${range.start}:${range.end}`;
}

export function App() {
  const [engine, setEngine] = useState<EngineKind>('excel_lite');
  const [sheet, setSheet] = useState<SheetInfo | null>(null);
  const [wizardOpen, setWizardOpen] = useState(false);

  // Lattice view — recomputed when the sheet changes.
  const lattice = useMemo(() => {
    if (!sheet) return null;
    return createLatticeView(sheet.lattice, CELL_SIZE);
  }, [sheet]);

  // The cursor: canonical cell address (`"A1"` / `"H(0,0)"`).
  const [activeAddress, setActiveAddress] = useState<string | null>(null);
  const [pickAddress, setPickAddress] = useState<string | null>(null);
  const [selectionRange, setSelectionRange] = useState<SelectionRange | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState('');
  const [snapshots, setSnapshots] = useState<Map<string, CellSnapshot>>(new Map());

  const inputRef = useRef<HTMLInputElement>(null);
  const [gridFocusTick, setGridFocusTick] = useState(0);
  const bumpGridFocus = useCallback(() => setGridFocusTick((n) => n + 1), []);

  const sheetId = sheet?.id ?? null;

  // Reset cursor to the lattice's natural origin whenever the sheet changes.
  useEffect(() => {
    if (!lattice) {
      setActiveAddress(null);
      return;
    }
    setActiveAddress(lattice.kind === 'square' ? 'A1' : 'H(0,0)');
    setPickAddress(null);
    setSelectionRange(null);
  }, [lattice]);

  const activeSnapshot = useMemo(
    () => (activeAddress ? snapshots.get(activeAddress) ?? null : null),
    [snapshots, activeAddress],
  );

  const baselineSource = useMemo(() => {
    if (!activeSnapshot) return '';
    if (activeSnapshot.spilled_from) {
      return snapshots.get(activeSnapshot.spilled_from)?.source ?? '';
    }
    return activeSnapshot.source ?? '';
  }, [activeSnapshot, snapshots]);

  useEffect(() => {
    if (editing) return;
    setDraft(baselineSource);
  }, [baselineSource, editing]);

  const refreshWorkbookInfo = useCallback(async () => {
    try {
      const info = await ipc.workbookInfo();
      if (info.sheets.length === 0) {
        setSheet(null);
        setSnapshots(new Map());
        setWizardOpen(true);
      } else {
        setSheet(info.sheets[0]);
        setWizardOpen(false);
      }
    } catch (e) {
      console.error('workbook.info failed:', e);
    }
  }, []);

  const refreshSnapshot = useCallback(async () => {
    if (sheetId == null || !lattice) return;
    try {
      const win = snapshotWindow(lattice);
      const snap = await ipc.snapshotRange(sheetId, win.start, win.end);
      const m = new Map<string, CellSnapshot>();
      for (const s of snap) m.set(s.address, s);
      setSnapshots(m);
    } catch (e) {
      console.error('snapshot failed:', e);
    }
  }, [sheetId, lattice]);

  useEffect(() => {
    void refreshWorkbookInfo();
  }, [refreshWorkbookInfo]);

  useEffect(() => {
    if (sheetId != null) void refreshSnapshot();
  }, [sheetId, refreshSnapshot]);

  useEffect(() => {
    return window.tescellate.onWorkbookOpened((payload) => {
      if (payload.path === null) {
        void refreshWorkbookInfo();
      } else {
        void refreshWorkbookInfo().then(() => refreshSnapshot());
      }
    });
  }, [refreshWorkbookInfo, refreshSnapshot]);

  const insertIntoDraft = useCallback(
    (text: string) => {
      const input = inputRef.current;
      if (!input) {
        setDraft((d) => d + text);
        return;
      }
      const start = input.selectionStart ?? draft.length;
      const end = input.selectionEnd ?? draft.length;
      const next = draft.slice(0, start) + text + draft.slice(end);
      setDraft(next);
      requestAnimationFrame(() => {
        input.focus();
        const pos = start + text.length;
        input.setSelectionRange(pos, pos);
      });
    },
    [draft],
  );

  const onSelect = useCallback(
    (addr: string) => {
      setSelectionRange({ start: addr, end: addr });
      if (!editing) {
        setActiveAddress(addr);
        setPickAddress(null);
        return;
      }
      setPickAddress(addr);
      insertIntoDraft(addr);
    },
    [editing, insertIntoDraft],
  );

  const onRangeSelect = useCallback(
    (range: SelectionRange) => {
      setSelectionRange(range);
      if (!editing) return;
      const text = rangeText(range);
      const input = inputRef.current;
      if (!input) {
        setDraft((d) => stripTrailingRefOrRange(d) + text);
        return;
      }
      const caret = input.selectionStart ?? draft.length;
      const before = draft.slice(0, caret);
      const after = draft.slice(caret);
      const newBefore = stripTrailingRefOrRange(before);
      const next = newBefore + text + after;
      setDraft(next);
      requestAnimationFrame(() => {
        input.focus();
        const pos = newBefore.length + text.length;
        input.setSelectionRange(pos, pos);
      });
    },
    [editing, draft],
  );

  const onStartEditWith = useCallback((initial: string) => {
    setDraft(initial);
    setEditing(true);
    requestAnimationFrame(() => {
      const input = inputRef.current;
      if (input) {
        input.focus();
        const end = input.value.length;
        input.setSelectionRange(end, end);
      }
    });
  }, []);

  const onStartEdit = useCallback(() => {
    setEditing(true);
    requestAnimationFrame(() => {
      const input = inputRef.current;
      if (input) {
        input.focus();
        input.select();
      }
    });
  }, []);

  const onClear = useCallback(async () => {
    if (sheetId == null || !activeAddress) return;
    try {
      const changed = await ipc.setCell(sheetId, activeAddress, null);
      setSnapshots((prev) => {
        const next = new Map(prev);
        for (const s of changed) next.set(s.address, s);
        next.delete(activeAddress);
        return next;
      });
      void refreshSnapshot();
    } catch (e) {
      console.error('cell.set (clear) failed:', e);
    }
  }, [sheetId, activeAddress, refreshSnapshot]);

  const onMove = useCallback(
    (dCol: number, dRow: number) => {
      if (!lattice || !activeAddress) return;
      const next = lattice.moveAddress(activeAddress, dCol, dRow);
      if (!next) return;
      setActiveAddress(next);
      setPickAddress(null);
      setSelectionRange(null);
    },
    [lattice, activeAddress],
  );

  const onCommit = useCallback(async () => {
    if (!editing) return;
    setEditing(false);
    setPickAddress(null);
    bumpGridFocus();
    if (sheetId == null || !activeAddress) return;
    if (draft === baselineSource) return;
    try {
      const changed = await ipc.setCell(
        sheetId,
        activeAddress,
        draft.trim() === '' ? null : draft,
      );
      setSnapshots((prev) => {
        const next = new Map(prev);
        for (const s of changed) next.set(s.address, s);
        if (draft.trim() === '' && next.get(activeAddress)?.value.kind === 'empty') {
          next.delete(activeAddress);
        }
        return next;
      });
      void refreshSnapshot();
    } catch (e) {
      console.error('cell.set failed:', e);
    }
  }, [editing, sheetId, activeAddress, draft, baselineSource, refreshSnapshot, bumpGridFocus]);

  const onCancel = useCallback(() => {
    setEditing(false);
    setPickAddress(null);
    setDraft(baselineSource);
    bumpGridFocus();
  }, [baselineSource, bumpGridFocus]);

  const onWizardComplete = useCallback(() => {
    setWizardOpen(false);
    void refreshWorkbookInfo();
    bumpGridFocus();
  }, [refreshWorkbookInfo, bumpGridFocus]);

  const onWizardCancel = useCallback(() => {
    if (sheet != null) setWizardOpen(false);
  }, [sheet]);

  return (
    <>
      <FormulaBar
        ref={inputRef}
        engine={engine}
        onEngineChange={setEngine}
        address={activeAddress}
        draft={draft}
        onDraftChange={setDraft}
        onFocus={() => setEditing(true)}
        onCommit={onCommit}
        onCancel={onCancel}
        spilledFrom={activeSnapshot?.spilled_from ?? null}
      />
      <div style={{ flex: 1, position: 'relative' }}>
        {sheetId != null && lattice ? (
          <GridCanvas
            lattice={lattice}
            snapshots={snapshots}
            activeAddress={activeAddress}
            pickAddress={pickAddress}
            selectionRange={selectionRange}
            editing={editing}
            focusTick={gridFocusTick}
            onSelect={onSelect}
            onRangeSelect={onRangeSelect}
            onStartEditWith={onStartEditWith}
            onStartEdit={onStartEdit}
            onClear={onClear}
            onMove={onMove}
          />
        ) : sheetId != null && !lattice ? (
          <div className="no-sheet-hint">
            This sheet's lattice ({sheet?.lattice}) doesn't have a renderer yet.
          </div>
        ) : (
          <div className="no-sheet-hint">
            No workbook yet — use the wizard to pick a tessellation.
          </div>
        )}
        {wizardOpen && <WizardModal onComplete={onWizardComplete} onCancel={onWizardCancel} />}
      </div>
    </>
  );
}
