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

  /**
   * `activeCell` — the cell currently being edited (what Enter commits to).
   * `cursorCell` — the visually highlighted cell. Equals activeCell when
   * not editing; can differ while editing because cell-clicks insert refs
   * rather than changing the edit target.
   */
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

  // When the active cell changes (and we're not mid-edit), sync the draft
  // to that cell's source — taking spill into account so the bar shows the
  // *source's* formula when the active cell is a spill target.
  useEffect(() => {
    if (editing) return;
    if (!activeSnapshot) {
      setDraft('');
      return;
    }
    if (activeSnapshot.spilled_from) {
      setDraft(snapshots.get(activeSnapshot.spilled_from)?.source ?? '');
    } else {
      setDraft(activeSnapshot.source ?? '');
    }
  }, [activeSnapshot, snapshots, editing]);

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

  // When user clicks a grid cell: if not editing, move the active cell.
  // If editing, insert the cell's address at the caret position in the
  // formula input (Excel/Sheets behaviour). Either way, the cursor moves.
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
      // Re-focus and place caret after the inserted ref.
      requestAnimationFrame(() => {
        input.focus();
        const pos = start + ref.length;
        input.setSelectionRange(pos, pos);
      });
    },
    [editing, draft],
  );

  const onCommit = useCallback(async () => {
    if (!editing) return;
    setEditing(false);
    if (!activeAddress) return;
    const baseline = (() => {
      if (!activeSnapshot) return '';
      if (activeSnapshot.spilled_from) {
        return snapshots.get(activeSnapshot.spilled_from)?.source ?? '';
      }
      return activeSnapshot.source ?? '';
    })();
    if (draft === baseline) return;
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
      // After commit, refresh once more to pick up any spill cells whose
      // sources weren't in `changed` (e.g., when source array shrinks).
      void refresh();
    } catch (e) {
      console.error('cell.set failed:', e);
    }
  }, [editing, activeAddress, activeSnapshot, draft, snapshots, refresh]);

  const onCancel = useCallback(() => {
    setEditing(false);
    if (!activeSnapshot) {
      setDraft('');
      return;
    }
    if (activeSnapshot.spilled_from) {
      setDraft(snapshots.get(activeSnapshot.spilled_from)?.source ?? '');
    } else {
      setDraft(activeSnapshot.source ?? '');
    }
  }, [activeSnapshot, snapshots]);

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
        />
      </div>
    </>
  );
}
