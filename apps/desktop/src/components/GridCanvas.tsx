import { useCallback, useEffect, useRef } from 'react';
import type { CellSnapshot } from '../ipc';
import { formatValue } from '../ipc';
import { toAddress, type Coord } from '../address';

export interface SelectionRange {
  anchor: Coord;
  focus: Coord;
}

interface Props {
  cellSize: number;
  snapshots: Map<string, CellSnapshot>;
  /** The true spreadsheet cursor — what Enter commits to and what the
   * filled highlight + ring mark when *not* editing. */
  activeCell: Coord | null;
  /** Only set during edit-mode cell-picking — the cell most recently
   * clicked to insert as a reference. Rendered as a dimmer filled
   * highlight separate from the active ring. `null` outside editing. */
  pickPreview: Coord | null;
  /** Multi-cell rectangular selection. Set by click-and-drag (or by
   * a single click, which is a 1×1 range that we render as if it were a
   * scalar). The anchor is where the drag started; the focus is where
   * it ended. */
  selectionRange: SelectionRange | null;
  /** When true, the active-cell ring is drawn dashed ("marching ants")
   * and clicks add references instead of moving the active cell. */
  editing: boolean;
  /** Bumping this value asks the canvas to claim keyboard focus. App uses
   * it after wizard-close / formula commit / formula cancel so the user's
   * next keystroke is captured by the grid, not by document.body. */
  focusTick: number;
  onSelect: (c: Coord) => void;
  /** Called when a drag selects a rectangular region. `anchor === focus`
   * means a single-cell drag (treated like a click by the parent). */
  onRangeSelect: (range: SelectionRange) => void;
  /** A printable key was pressed while a cell was selected — start editing
   * with that character as the initial draft. */
  onStartEditWith: (initial: string) => void;
  /** F2 / typing-into-existing — start editing the cell's current source. */
  onStartEdit: () => void;
  /** Delete / Backspace clears the active cell. */
  onClear: () => void;
  /** Move the active+cursor cell by (dCol, dRow). */
  onMove: (dCol: number, dRow: number) => void;
}

/**
 * Phase 1 square-grid renderer. Canvas 2D, no virtualization yet —
 * Phase 4 introduces WebGL + viewport culling.
 */
export function GridCanvas({
  cellSize,
  snapshots,
  activeCell,
  pickPreview,
  selectionRange,
  editing,
  focusTick,
  onSelect,
  onRangeSelect,
  onStartEditWith,
  onStartEdit,
  onClear,
  onMove,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rowHeader = 28;
  const colHeader = 48;

  // Drag-tracking state. A drag starts on mousedown over a cell; we attach
  // window-level mousemove/mouseup listeners so the user can drag past the
  // window edge without losing tracking. `lastDragCoord` lets us skip
  // re-emitting onRangeSelect on every pixel when the user's mouse stays
  // within one cell.
  const dragAnchorRef = useRef<Coord | null>(null);
  const lastDragCoordRef = useRef<Coord | null>(null);

  // Claim focus on mount AND whenever `focusTick` bumps.
  useEffect(() => {
    const id = requestAnimationFrame(() => {
      canvasRef.current?.focus();
    });
    return () => cancelAnimationFrame(id);
  }, [focusTick]);

  const cellAtClientPos = useCallback(
    (clientX: number, clientY: number): Coord | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      const x = clientX - rect.left;
      const y = clientY - rect.top;
      if (x < colHeader || y < rowHeader) return null;
      return {
        col: Math.floor((x - colHeader) / cellSize),
        row: Math.floor((y - rowHeader) / cellSize),
      };
    },
    [cellSize, colHeader, rowHeader],
  );

  // Window-level drag listeners. Kept always-attached so we never miss a
  // mouseup outside the canvas — but they're cheap no-ops when no drag
  // is in flight.
  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragAnchorRef.current) return;
      const focus = cellAtClientPos(e.clientX, e.clientY);
      if (!focus) return;
      const prev = lastDragCoordRef.current;
      if (prev && prev.col === focus.col && prev.row === focus.row) return;
      lastDragCoordRef.current = focus;
      onRangeSelect({ anchor: dragAnchorRef.current, focus });
    };
    const onMouseUp = () => {
      dragAnchorRef.current = null;
      lastDragCoordRef.current = null;
    };
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };
  }, [cellAtClientPos, onRangeSelect]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const draw = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      ctx.fillStyle = '#1b1d20';
      ctx.fillRect(0, 0, w, h);

      const cols = Math.ceil((w - colHeader) / cellSize) + 1;
      const rows = Math.ceil((h - rowHeader) / cellSize) + 1;

      const cellRect = (c: Coord) => ({
        x: Math.floor(colHeader + c.col * cellSize),
        y: Math.floor(rowHeader + c.row * cellSize),
      });

      // Rectangular hull for the multi-cell selection range. When the
      // range collapses to its anchor (anchor === focus), this is the
      // same as the single-cell highlight that activeCell paints.
      if (selectionRange) {
        const c0 = Math.min(selectionRange.anchor.col, selectionRange.focus.col);
        const c1 = Math.max(selectionRange.anchor.col, selectionRange.focus.col);
        const r0 = Math.min(selectionRange.anchor.row, selectionRange.focus.row);
        const r1 = Math.max(selectionRange.anchor.row, selectionRange.focus.row);
        const { x, y } = cellRect({ col: c0, row: r0 });
        const wd = (c1 - c0 + 1) * cellSize;
        const hd = (r1 - r0 + 1) * cellSize;
        if (x + wd > colHeader && y + hd > rowHeader) {
          ctx.fillStyle = '#1f4068';
          ctx.fillRect(x, y, wd, hd);
        }
      } else if (activeCell) {
        // Single-cell highlight when no range is active.
        const { x, y } = cellRect(activeCell);
        if (x >= colHeader && y >= rowHeader && x < w && y < h) {
          ctx.fillStyle = '#1f4068';
          ctx.fillRect(x, y, cellSize, cellSize);
        }
      }

      // Pick-preview highlight on the most-recently-picked cell during
      // edit-mode ref insertion.
      if (editing && pickPreview) {
        const { x, y } = cellRect(pickPreview);
        if (x >= colHeader && y >= rowHeader && x < w && y < h) {
          ctx.fillStyle = '#2a4d7a';
          ctx.fillRect(x, y, cellSize, cellSize);
        }
      }

      // Grid lines.
      ctx.strokeStyle = '#2a2e34';
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let c = 0; c <= cols; c += 1) {
        const x = colHeader + c * cellSize + 0.5;
        ctx.moveTo(x, rowHeader);
        ctx.lineTo(x, h);
      }
      for (let r = 0; r <= rows; r += 1) {
        const y = rowHeader + r * cellSize + 0.5;
        ctx.moveTo(colHeader, y);
        ctx.lineTo(w, y);
      }
      ctx.stroke();

      // Header backgrounds.
      ctx.fillStyle = '#14171a';
      ctx.fillRect(0, 0, w, rowHeader);
      ctx.fillRect(0, 0, colHeader, h);

      // Header text.
      ctx.fillStyle = '#7a8593';
      ctx.font = '11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      for (let c = 0; c < cols; c += 1) {
        const label = toAddress({ col: c, row: 0 }).replace(/[0-9]+$/, '');
        const x = colHeader + c * cellSize + cellSize / 2;
        ctx.fillText(label, x, rowHeader / 2);
      }
      ctx.textAlign = 'right';
      for (let r = 0; r < rows; r += 1) {
        const y = rowHeader + r * cellSize + cellSize / 2;
        ctx.fillText(`${r + 1}`, colHeader - 8, y);
      }

      // Cell values.
      ctx.font = '12px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
      ctx.textBaseline = 'middle';
      for (let r = 0; r < rows; r += 1) {
        for (let c = 0; c < cols; c += 1) {
          const addr = toAddress({ col: c, row: r });
          const snap = snapshots.get(addr);
          if (!snap) continue;
          const text = formatValue(snap.value);
          if (!text) continue;
          const isNumber =
            snap.value.kind === 'number' || snap.value.kind === 'integer';
          ctx.textAlign = isNumber ? 'right' : 'left';
          ctx.fillStyle =
            snap.value.kind === 'error'
              ? '#ff6b6b'
              : snap.spilled_from
                ? '#9da7b3'
                : '#e7e9ec';
          const x = isNumber
            ? colHeader + (c + 1) * cellSize - 6
            : colHeader + c * cellSize + 6;
          const y = rowHeader + r * cellSize + cellSize / 2;
          ctx.save();
          ctx.beginPath();
          ctx.rect(
            colHeader + c * cellSize + 1,
            rowHeader + r * cellSize + 1,
            cellSize - 2,
            cellSize - 2,
          );
          ctx.clip();
          ctx.fillText(text, x, y);
          ctx.restore();
        }
      }

      // Active-cell ring — drawn last so it sits on top of values.
      if (activeCell) {
        const { x, y } = cellRect(activeCell);
        if (x >= colHeader && y >= rowHeader && x < w && y < h) {
          ctx.lineWidth = 2;
          ctx.strokeStyle = editing ? '#4a90e2' : '#88b5f5';
          ctx.setLineDash(editing ? [4, 3] : []);
          ctx.strokeRect(x + 1, y + 1, cellSize - 2, cellSize - 2);
          ctx.setLineDash([]);
        }
      }
    };

    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    return () => ro.disconnect();
  }, [cellSize, snapshots, activeCell, pickPreview, selectionRange, editing]);

  const onMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return; // primary mouse button only
    const coord = cellAtClientPos(e.clientX, e.clientY);
    if (!coord) return;
    dragAnchorRef.current = coord;
    lastDragCoordRef.current = coord;
    // Single-click semantics fire immediately; the range path takes over
    // only if the user actually drags off this cell.
    onSelect(coord);
    if (!editing) {
      e.currentTarget.focus();
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;

    if (e.key.length === 1) {
      onStartEditWith(e.key);
      e.preventDefault();
      return;
    }

    switch (e.key) {
      case 'F2':
        onStartEdit();
        e.preventDefault();
        break;
      case 'Delete':
      case 'Backspace':
        onClear();
        e.preventDefault();
        break;
      case 'ArrowUp':
        onMove(0, -1);
        e.preventDefault();
        break;
      case 'ArrowDown':
      case 'Enter':
        onMove(0, 1);
        e.preventDefault();
        break;
      case 'ArrowLeft':
        onMove(-1, 0);
        e.preventDefault();
        break;
      case 'ArrowRight':
      case 'Tab':
        onMove(1, 0);
        e.preventDefault();
        break;
      default:
        break;
    }
  };

  return (
    <canvas
      ref={canvasRef}
      tabIndex={0}
      onMouseDown={onMouseDown}
      onKeyDown={onKeyDown}
      style={{
        width: '100%',
        height: '100%',
        display: 'block',
        cursor: 'cell',
        outline: 'none',
      }}
    />
  );
}
