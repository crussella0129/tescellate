import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { FormulaBar } from './components/FormulaBar';
import { GridCanvas, type SelectionRange } from './components/GridCanvas';
import { WizardModal } from './components/WizardModal';
import type { EngineKind } from './types';
import { ipc, type CellSnapshot, type SheetInfo } from './ipc';
import { toAddress, type Coord } from './address';

const CELL_SIZE = 96;
const SNAPSHOT_RANGE = { start: 'A1', end: 'Z100' };

/** When the user drags out a range during edit-mode, the bar already
 * has the anchor's address (inserted by the initial click). We strip that
 * trailing address-or-range and substitute the full range. Pattern: an
 * optional `[A-Z]+[0-9]+(:[A-Z]+[0-9]+)?` at the very end of `text`. */
function stripTrailingRefOrRange(text: string, _anchor: string, _focus: string): string {
  return text.replace(/[A-Z]+[0-9]+(:[A-Z]+[0-9]+)?$/, '');
}

export function App() {
  const [engine, setEngine] = useState<EngineKind>('excel_lite');
  const [sheet, setSheet] = useState<SheetInfo | null>(null);
  const [wizardOpen, setWizardOpen] = useState(false);

  // `activeCell` is the spreadsheet's true cursor — what Enter commits to,
  // what the grid's solid ring marks. During edit-mode cell-picking,
  // `pickPreview` tracks the most-recently-clicked cell for the dimmer
  // filled highlight; outside editing, the highlight is the active cell.
  // Keeping these conceptually distinct means a future bug can't drift the
  // two out of sync when they should be identical (which is what was
  // happening before — cursor highlight visibly trailing the active ring).
  const [activeCell, setActiveCell] = useState<Coord | null>({ col: 0, row: 0 });
  const [pickPreview, setPickPreview] = useState<Coord | null>(null);
  // Multi-cell rectangular selection from click-and-drag. When set, the
  // grid paints a filled hull and `activeCell` stays at the anchor (so
  // Enter commits to the anchor, like Excel/Sheets).
  const [selectionRange, setSelectionRange] = useState<SelectionRange | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState('');
  const [snapshots, setSnapshots] = useState<Map<string, CellSnapshot>>(new Map());

  const inputRef = useRef<HTMLInputElement>(null);
  // Bumping this counter asks GridCanvas to claim keyboard focus. We bump
  // after wizard-close, formula commit, and formula cancel so cursor keys
  // start working immediately without an explicit cell click.
  const [gridFocusTick, setGridFocusTick] = useState(0);
  const bumpGridFocus = useCallback(() => setGridFocusTick((n) => n + 1), []);

  const sheetId = sheet?.id ?? null;

  const activeAddress = useMemo(
    () => (activeCell ? toAddress(activeCell) : null),
    [activeCell],
  );

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

  /** Re-query the core for the workbook's current shape. If no sheet
   * exists, surface the wizard so the user can pick a tessellation. */
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
    if (sheetId == null) return;
    try {
      const snap = await ipc.snapshotRange(sheetId, SNAPSHOT_RANGE.start, SNAPSHOT_RANGE.end);
      const m = new Map<string, CellSnapshot>();
      for (const s of snap) m.set(s.address, s);
      setSnapshots(m);
    } catch (e) {
      console.error('snapshot failed:', e);
    }
  }, [sheetId]);

  useEffect(() => {
    void refreshWorkbookInfo();
  }, [refreshWorkbookInfo]);

  useEffect(() => {
    if (sheetId != null) void refreshSnapshot();
  }, [sheetId, refreshSnapshot]);

  // Main process tells us when the workbook changes (File menu actions).
  useEffect(() => {
    return window.tescellate.onWorkbookOpened((payload) => {
      // `path: null` means workbook.new — show wizard. `path: string` means
      // a file was loaded — refresh state.
      if (payload.path === null) {
        void refreshWorkbookInfo();
      } else {
        void refreshWorkbookInfo().then(() => refreshSnapshot());
      }
    });
  }, [refreshWorkbookInfo, refreshSnapshot]);

  /** Splice `text` into the formula bar at the current caret. Used by
   * both single-cell ref insertion and range insertion. */
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
    (coord: Coord) => {
      // Single-click resets any prior range and treats this as a 1×1
      // selection. If the user actually drags, `onRangeSelect` will
      // overwrite this with the rectangular hull.
      setSelectionRange({ anchor: coord, focus: coord });
      if (!editing) {
        setActiveCell(coord);
        setPickPreview(null);
        return;
      }
      // Editing: cell click inserts the address as a reference and lights
      // up the clicked cell as a pick preview.
      setPickPreview(coord);
      insertIntoDraft(toAddress(coord));
    },
    [editing, insertIntoDraft],
  );

  /** Click-and-drag selected a rectangular region of the grid. When
   * editing, insert an `<anchor>:<focus>` range at the bar caret —
   * overwriting whatever the single-click `onSelect` just inserted, so
   * the bar ends up with the range token instead of just the anchor. */
  const onRangeSelect = useCallback(
    (range: SelectionRange) => {
      setSelectionRange(range);
      if (!editing) return;
      // Replace the just-inserted anchor address with the full range.
      // We track the last-inserted text length: anchor's address.
      const anchorAddr = toAddress(range.anchor);
      const focusAddr = toAddress(range.focus);
      // Build the canonical `A1:B5` form with start = top-left, end = bottom-right.
      const c0 = Math.min(range.anchor.col, range.focus.col);
      const c1 = Math.max(range.anchor.col, range.focus.col);
      const r0 = Math.min(range.anchor.row, range.focus.row);
      const r1 = Math.max(range.anchor.row, range.focus.row);
      const rangeText = `${toAddress({ col: c0, row: r0 })}:${toAddress({ col: c1, row: r1 })}`;
      // Rewrite the trailing anchor (or previous range) at the bar caret
      // into the new range. We achieve this by scanning back from the
      // caret for an `A1`-or-`A1:B5` token and replacing it.
      const input = inputRef.current;
      if (!input) {
        setDraft((d) => {
          const stripped = stripTrailingRefOrRange(d, anchorAddr, focusAddr);
          return stripped + rangeText;
        });
        return;
      }
      const caret = input.selectionStart ?? draft.length;
      const before = draft.slice(0, caret);
      const after = draft.slice(caret);
      const newBefore = stripTrailingRefOrRange(before, anchorAddr, focusAddr);
      const next = newBefore + rangeText + after;
      setDraft(next);
      requestAnimationFrame(() => {
        input.focus();
        const pos = newBefore.length + rangeText.length;
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

  const onMove = useCallback((dCol: number, dRow: number) => {
    setActiveCell((c) =>
      c
        ? { col: Math.max(0, c.col + dCol), row: Math.max(0, c.row + dRow) }
        : { col: 0, row: 0 },
    );
    setPickPreview(null);
    // Arrow keys collapse any multi-cell selection — Excel/Sheets style.
    setSelectionRange(null);
  }, []);

  const onCommit = useCallback(async () => {
    if (!editing) return;
    setEditing(false);
    setPickPreview(null);
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
    setPickPreview(null);
    setDraft(baselineSource);
    bumpGridFocus();
  }, [baselineSource, bumpGridFocus]);

  const onWizardComplete = useCallback(() => {
    setWizardOpen(false);
    void refreshWorkbookInfo();
    bumpGridFocus();
  }, [refreshWorkbookInfo, bumpGridFocus]);

  const onWizardCancel = useCallback(() => {
    // If there's no sheet at all, cancelling the wizard would leave the user
    // staring at nothing — so we only allow cancel when a sheet already
    // exists (i.e. they hit File→New and changed their mind).
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
        {sheetId != null ? (
          <GridCanvas
            cellSize={CELL_SIZE}
            snapshots={snapshots}
            activeCell={activeCell}
            pickPreview={pickPreview}
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
