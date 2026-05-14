import { useEffect, useRef } from 'react';
import type { CellSnapshot } from '../ipc';
import { formatValue } from '../ipc';
import { toAddress, type Coord } from '../address';

interface Props {
  cellSize: number;
  snapshots: Map<string, CellSnapshot>;
  /** The cell whose formula is being edited; ringed in solid colour. */
  activeCell: Coord | null;
  /** The latest-clicked cell. Equals activeCell when not editing; can
   * differ during edit-mode while picking refs. */
  cursorCell: Coord | null;
  /** When true, the active-cell ring is drawn with a "marching ants"
   * style and clicks add references instead of moving the active cell. */
  editing: boolean;
  onSelect: (c: Coord) => void;
}

/**
 * Phase 1 square-grid renderer. Canvas 2D, no virtualization yet —
 * Phase 4 introduces WebGL + viewport culling.
 */
export function GridCanvas({
  cellSize,
  snapshots,
  activeCell,
  cursorCell,
  editing,
  onSelect,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rowHeader = 28;
  const colHeader = 48;

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

      // Cursor highlight (filled). Drawn under grid lines and the active
      // ring so the ring stays visible on top.
      const cursor = cursorCell;
      if (cursor) {
        const sx = colHeader + cursor.col * cellSize;
        const sy = rowHeader + cursor.row * cellSize;
        if (sx >= colHeader && sy >= rowHeader && sx < w && sy < h) {
          ctx.fillStyle = '#1f4068';
          ctx.fillRect(sx, sy, cellSize, cellSize);
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
          const isNumber = snap.value.kind === 'number' || snap.value.kind === 'integer';
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

      // Active-cell ring. Solid when not editing; dashed (marching ants)
      // while editing to signal "this is where Enter will commit".
      if (activeCell) {
        const ax = colHeader + activeCell.col * cellSize;
        const ay = rowHeader + activeCell.row * cellSize;
        if (ax >= colHeader && ay >= rowHeader && ax < w && ay < h) {
          ctx.lineWidth = 2;
          ctx.strokeStyle = editing ? '#4a90e2' : '#88b5f5';
          ctx.setLineDash(editing ? [4, 3] : []);
          ctx.strokeRect(ax + 1, ay + 1, cellSize - 2, cellSize - 2);
          ctx.setLineDash([]);
        }
      }
    };

    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    return () => ro.disconnect();
  }, [cellSize, snapshots, activeCell, cursorCell, editing]);

  const onClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    if (x < colHeader || y < rowHeader) return;
    const col = Math.floor((x - colHeader) / cellSize);
    const row = Math.floor((y - rowHeader) / cellSize);
    onSelect({ col, row });
  };

  return (
    <canvas
      ref={canvasRef}
      onClick={onClick}
      style={{ width: '100%', height: '100%', display: 'block', cursor: 'cell' }}
    />
  );
}
