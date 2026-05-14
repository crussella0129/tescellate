import { useCallback, useEffect, useMemo, useState } from 'react';
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
  const [selection, setSelection] = useState<Coord | null>({ col: 0, row: 0 });
  const [snapshots, setSnapshots] = useState<Map<string, CellSnapshot>>(new Map());

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

  // Initial snapshot load.
  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Re-snapshot after File→Open / File→New.
  useEffect(() => {
    return window.tescellate.onWorkbookOpened(() => {
      void refresh();
    });
  }, [refresh]);

  const selectedAddress = useMemo(
    () => (selection ? toAddress(selection) : null),
    [selection],
  );

  const selectedSnapshot = useMemo(() => {
    if (!selectedAddress) return null;
    return snapshots.get(selectedAddress) ?? null;
  }, [snapshots, selectedAddress]);

  const selectedSource = useMemo(() => {
    if (!selectedSnapshot) return '';
    if (selectedSnapshot.spilled_from) {
      // Show the source cell's formula so the user can see what produced
      // this value. Editing the bar will write the spilled cell's address
      // (breaking the spill, which mirrors Excel behaviour).
      return snapshots.get(selectedSnapshot.spilled_from)?.source ?? '';
    }
    return selectedSnapshot.source ?? '';
  }, [selectedSnapshot, snapshots]);

  const onCommit = useCallback(
    async (src: string) => {
      if (!selectedAddress) return;
      try {
        const changed = await ipc.setCell(
          SHEET_ID,
          selectedAddress,
          src.trim() === '' ? null : src,
        );
        // Merge the changed cells into the existing snapshot map without a
        // full refresh — cheap and correct because the core returns exactly
        // the cells that changed.
        setSnapshots((prev) => {
          const next = new Map(prev);
          for (const s of changed) next.set(s.address, s);
          // Also clear the entry if the source went empty and the value is empty.
          if (src.trim() === '' && next.get(selectedAddress)?.value.kind === 'empty') {
            next.delete(selectedAddress);
          }
          return next;
        });
      } catch (e) {
        console.error('cell.set failed:', e);
      }
    },
    [selectedAddress],
  );

  return (
    <>
      <FormulaBar
        engine={engine}
        onEngineChange={setEngine}
        address={selectedAddress}
        source={selectedSource}
        spilledFrom={selectedSnapshot?.spilled_from ?? null}
        onCommit={onCommit}
      />
      <div style={{ flex: 1, position: 'relative' }}>
        <GridCanvas
          cellSize={CELL_SIZE}
          snapshots={snapshots}
          selection={selection}
          onSelect={setSelection}
        />
      </div>
    </>
  );
}
