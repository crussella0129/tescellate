import { useCallback, useEffect, useRef } from 'react';
import type { CellSnapshot } from '../ipc';
import { formatValue } from '../ipc';
import type { LatticeView } from '../lattice';

export interface SelectionRange {
  /** Anchor address (where the drag started). */
  start: string;
  /** Focus address (where the drag currently sits). */
  end: string;
}

interface Props {
  lattice: LatticeView;
  snapshots: Map<string, CellSnapshot>;
  /** The true spreadsheet cursor — what Enter commits to and what the
   * filled highlight + ring mark when *not* editing. Canonical
   * address ("A1" / "H(0,0)"). */
  activeAddress: string | null;
  /** Only set during edit-mode cell-picking — the most-recently-clicked
   * cell, drawn as a dimmer filled highlight. `null` outside editing. */
  pickAddress: string | null;
  /** Multi-cell selection from click-and-drag (or a 1×1 from a single
   * click). Painted as a hull over every cell in the range. */
  selectionRange: SelectionRange | null;
  /** When true, the active-cell ring is drawn dashed ("marching ants")
   * and clicks add references instead of moving the active cell. */
  editing: boolean;
  /** Bumping this value asks the canvas to claim keyboard focus. */
  focusTick: number;
  onSelect: (addr: string) => void;
  /** Called when a drag selects a range. `start === end` means a
   * single-cell drag (parent treats as a click). */
  onRangeSelect: (range: SelectionRange) => void;
  /** A printable key was pressed while a cell was selected — start
   * editing with that character as the initial draft. */
  onStartEditWith: (initial: string) => void;
  /** F2 / start editing the active cell's current source. */
  onStartEdit: () => void;
  /** Delete / Backspace clears the active cell. */
  onClear: () => void;
  /** Move the active cell. The lattice maps the screen-space delta to
   * its own coord arithmetic. */
  onMove: (dCol: number, dRow: number) => void;
}

/**
 * Lattice-agnostic grid renderer. All cell math lives behind the
 * `LatticeView` interface — see `apps/desktop/src/lattice/`. The
 * canvas itself only knows how to paint shapes, headers, and text;
 * the shapes themselves come from the lattice.
 */
export function GridCanvas({
  lattice,
  snapshots,
  activeAddress,
  pickAddress,
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

  // Drag tracking — see Phase 1.7 PR for the rationale on window-level
  // mousemove/mouseup. `lastDragAddr` skips re-emitting onRangeSelect
  // when the mouse stays within the same cell.
  const dragStartRef = useRef<string | null>(null);
  const lastDragAddrRef = useRef<string | null>(null);

  useEffect(() => {
    const id = requestAnimationFrame(() => {
      canvasRef.current?.focus();
    });
    return () => cancelAnimationFrame(id);
  }, [focusTick]);

  const addressAtClient = useCallback(
    (clientX: number, clientY: number): string | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      return lattice.cellAtPixel(clientX - rect.left, clientY - rect.top);
    },
    [lattice],
  );

  // Always-attached drag listeners. Cheap when no drag is in flight.
  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragStartRef.current) return;
      const focus = addressAtClient(e.clientX, e.clientY);
      if (!focus) return;
      if (lastDragAddrRef.current === focus) return;
      lastDragAddrRef.current = focus;
      onRangeSelect({ start: dragStartRef.current, end: focus });
    };
    const onMouseUp = () => {
      dragStartRef.current = null;
      lastDragAddrRef.current = null;
    };
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };
  }, [addressAtClient, onRangeSelect]);

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

      // 1. Range hull — painted before grid lines and values so the
      //    fill sits underneath the cell content.
      if (selectionRange) {
        ctx.fillStyle = '#1f4068';
        for (const addr of lattice.rangeAddresses(selectionRange.start, selectionRange.end)) {
          ctx.beginPath();
          lattice.pathCell(ctx, addr);
          ctx.fill();
        }
      } else if (activeAddress) {
        ctx.fillStyle = '#1f4068';
        ctx.beginPath();
        lattice.pathCell(ctx, activeAddress);
        ctx.fill();
      }

      // 2. Pick-preview highlight during edit-mode ref insertion.
      if (editing && pickAddress) {
        ctx.fillStyle = '#2a4d7a';
        ctx.beginPath();
        lattice.pathCell(ctx, pickAddress);
        ctx.fill();
      }

      // 3. Grid lines — every visible cell traces its outline once.
      ctx.strokeStyle = '#2a2e34';
      ctx.lineWidth = 1;
      const visible = lattice.visibleAddresses(w, h);
      for (const addr of visible) {
        ctx.beginPath();
        lattice.pathCell(ctx, addr);
        ctx.stroke();
      }

      // 4. Headers (square only — hex no-ops).
      lattice.drawHeaders(ctx, w, h);

      // 5. Values.
      ctx.font = '12px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
      ctx.textBaseline = 'middle';
      for (const addr of visible) {
        const snap = snapshots.get(addr);
        if (!snap) continue;
        const text = formatValue(snap.value);
        if (!text) continue;
        const isNumber = snap.value.kind === 'number' || snap.value.kind === 'integer';
        const anchor = lattice.textAnchor(addr, isNumber);
        const bbox = lattice.cellBBox(addr);
        if (!anchor || !bbox) continue;
        ctx.textAlign = anchor.align;
        ctx.fillStyle =
          snap.value.kind === 'error'
            ? '#ff6b6b'
            : snap.spilled_from
              ? '#9da7b3'
              : '#e7e9ec';
        ctx.save();
        ctx.beginPath();
        ctx.rect(bbox.x + 1, bbox.y + 1, bbox.width - 2, bbox.height - 2);
        ctx.clip();
        ctx.fillText(text, anchor.x, anchor.y);
        ctx.restore();
      }

      // 6. Active-cell ring — drawn last so it sits on top.
      if (activeAddress) {
        ctx.lineWidth = 2;
        ctx.strokeStyle = editing ? '#4a90e2' : '#88b5f5';
        ctx.setLineDash(editing ? [4, 3] : []);
        ctx.beginPath();
        lattice.pathCell(ctx, activeAddress);
        ctx.stroke();
        ctx.setLineDash([]);
      }
    };

    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    return () => ro.disconnect();
  }, [lattice, snapshots, activeAddress, pickAddress, selectionRange, editing]);

  const onMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    const addr = addressAtClient(e.clientX, e.clientY);
    if (!addr) return;
    dragStartRef.current = addr;
    lastDragAddrRef.current = addr;
    onSelect(addr);
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
