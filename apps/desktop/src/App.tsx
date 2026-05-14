import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { FormulaBar } from './components/FormulaBar';
import { GridCanvas } from './components/GridCanvas';
import type { EngineKind } from './types';
import { ipc, type CellSnapshot } from './ipc';
import { toAddress, type Coord } from './address';

const SHEET_ID = 1;
const CELL_SIZE = 96;
const SNAPSHOT_RANGE = { start: 'A1', end: 'Z100' };

export function App() {
  const [engine, setEngine] = useState<EngineKind>('excel_lite');

  // activeCell: target of Enter (the cell being edited).
  // cursorCell: highlight; equals activeCell when not editing; can differ
  //             while editing because cell-clicks insert refs.
  const [activeCell, setActiveCell] = useState<Coord | null>({ col: 0, row: 0 });
  const [cursorCell, setCursorCell] = useState<Coord | null>({ col: 0, row: 0 });
  const [editing, setEditing] = useState(false);

  const [draft, setDraft] = useState('');
  const [snapshots, setSnapshots] = useState<Map<string, CellSnapshot>>(new Map());

  const inputRef = useRef<HTMLInputElement>(null);

  const activeAddress = useMemo(
    () => (activeCell ? toAddress(activeCell) : null),
    [activeCell],
  );

  const activeSnapshot = useMemo(
    () => (activeAddress ? snapshots.get(activeAddress) ?? null : null),
    [snapshots, activeAddress],
  );

  /** What the formula bar should show as a baseline for the active cell:
   * the cell's own source, OR — if the cell is a spill target — the source's
   * formula (so the user sees what produced the visible value). */
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

  const refresh = useCallback(async () => {
    try {
      const snap = await ipc.snapshotRange(SHEET_ID, SNAPSHOT_RANGE.start, SNAPSHOT_RANGE.end);
      const m = new Map<string, CellSnapshot>();
      for (const s of snap) m.set(s.address, s);
      setSnapshots(m);
    } catch (e) {
      console.error('snapshot failed:', e);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    return window.tescellate.onWorkbookOpened(() => {
      void refresh();
    });
  }, [refresh]);

  const onSelect = useCallback(
    (coord: Coord) => {
      setCursorCell(coord);
      if (!editing) {
        setActiveCell(coord);
        return;
      }
      const ref = toAddress(coord);
      const input = inputRef.current;
      if (!input) {
        setDraft((d) => d + ref);
        return;
      }
      const start = input.selectionStart ?? draft.length;
      const end = input.selectionEnd ?? draft.length;
      const next = draft.slice(0, start) + ref + draft.slice(end);
      setDraft(next);
      requestAnimationFrame(() => {
        input.focus();
        const pos = start + ref.length;
        input.setSelectionRange(pos, pos);
      });
    },
    [editing, draft],
  );

  /** Begin editing the active cell with `initial` as the initial draft —
   * triggered by typing on the grid (the dominant "I just want to type"
   * spreadsheet flow). */
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

  /** F2 — edit existing source. */
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

  /** Delete / Backspace — clear the active cell. */
  const onClear = useCallback(async () => {
    if (!activeAddress) return;
    try {
      const changed = await ipc.setCell(SHEET_ID, activeAddress, null);
      setSnapshots((prev) => {
        const next = new Map(prev);
        for (const s of changed) next.set(s.address, s);
        next.delete(activeAddress);
        return next;
      });
      void refresh();
    } catch (e) {
      console.error('cell.set (clear) failed:', e);
    }
  }, [activeAddress, refresh]);

  const onMove = useCallback((dCol: number, dRow: number) => {
    const step = (c: Coord | null): Coord =>
      c
        ? { col: Math.max(0, c.col + dCol), row: Math.max(0, c.row + dRow) }
        : { col: 0, row: 0 };
    setActiveCell(step);
    setCursorCell(step);
  }, []);

  const onCommit = useCallback(async () => {
    if (!editing) return;
    setEditing(false);
    if (!activeAddress) return;
    if (draft === baselineSource) return;
    try {
      const changed = await ipc.setCell(
        SHEET_ID,
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
      void refresh();
    } catch (e) {
      console.error('cell.set failed:', e);
    }
  }, [editing, activeAddress, draft, baselineSource, refresh]);

  const onCancel = useCallback(() => {
    setEditing(false);
    setDraft(baselineSource);
  }, [baselineSource]);

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
        <GridCanvas
          cellSize={CELL_SIZE}
          snapshots={snapshots}
          activeCell={activeCell}
          cursorCell={cursorCell}
          editing={editing}
          onSelect={onSelect}
          onStartEditWith={onStartEditWith}
          onStartEdit={onStartEdit}
          onClear={onClear}
          onMove={onMove}
        />
      </div>
    </>
  );
}
