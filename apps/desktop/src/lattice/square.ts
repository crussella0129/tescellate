/**
 * Square-lattice view — A1-style addressing, 4-neighbor cardinal
 * arithmetic, rectangular range hull. Wraps the address helpers from
 * `../address.ts`.
 */

import { fromAddress, toAddress, type Coord } from '../address';
import type { LatticeView } from './types';

export function createSquareLattice(cellSize: number): LatticeView {
  const headerWidth = 48;
  const headerHeight = 28;

  function rectFor(c: Coord) {
    return {
      x: headerWidth + c.col * cellSize,
      y: headerHeight + c.row * cellSize,
    };
  }

  function cellsInViewport(w: number, h: number): Coord[] {
    const cols = Math.ceil((w - headerWidth) / cellSize) + 1;
    const rows = Math.ceil((h - headerHeight) / cellSize) + 1;
    const out: Coord[] = [];
    for (let r = 0; r < rows; r += 1) {
      for (let c = 0; c < cols; c += 1) {
        out.push({ col: c, row: r });
      }
    }
    return out;
  }

  return {
    kind: 'square',
    headerWidth,
    headerHeight,

    cellAtPixel(x, y) {
      if (x < headerWidth || y < headerHeight) return null;
      const c: Coord = {
        col: Math.floor((x - headerWidth) / cellSize),
        row: Math.floor((y - headerHeight) / cellSize),
      };
      return toAddress(c);
    },

    cellCentroid(addr) {
      const c = fromAddress(addr);
      if (!c) return null;
      const { x, y } = rectFor(c);
      return [x + cellSize / 2, y + cellSize / 2];
    },

    cellBBox(addr) {
      const c = fromAddress(addr);
      if (!c) return null;
      const { x, y } = rectFor(c);
      return { x, y, width: cellSize, height: cellSize };
    },

    pathCell(ctx, addr) {
      const c = fromAddress(addr);
      if (!c) return;
      const { x, y } = rectFor(c);
      ctx.rect(x, y, cellSize, cellSize);
    },

    visibleAddresses(w, h) {
      return cellsInViewport(w, h).map(toAddress);
    },

    drawHeaders(ctx, w, h) {
      // Backgrounds.
      ctx.fillStyle = '#14171a';
      ctx.fillRect(0, 0, w, headerHeight);
      ctx.fillRect(0, 0, headerWidth, h);
      // Text.
      ctx.fillStyle = '#7a8593';
      ctx.font = '11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
      ctx.textBaseline = 'middle';
      ctx.textAlign = 'center';
      const visible = cellsInViewport(w, h);
      const maxCol = visible.reduce((m, c) => Math.max(m, c.col), 0);
      const maxRow = visible.reduce((m, c) => Math.max(m, c.row), 0);
      for (let c = 0; c <= maxCol; c += 1) {
        const label = toAddress({ col: c, row: 0 }).replace(/[0-9]+$/, '');
        ctx.fillText(label, headerWidth + c * cellSize + cellSize / 2, headerHeight / 2);
      }
      ctx.textAlign = 'right';
      for (let r = 0; r <= maxRow; r += 1) {
        ctx.fillText(`${r + 1}`, headerWidth - 8, headerHeight + r * cellSize + cellSize / 2);
      }
    },

    moveAddress(from, dCol, dRow) {
      const c = fromAddress(from);
      if (!c) return null;
      return toAddress({
        col: Math.max(0, c.col + dCol),
        row: Math.max(0, c.row + dRow),
      });
    },

    rangeAddresses(start, end) {
      const a = fromAddress(start);
      const b = fromAddress(end);
      if (!a || !b) return [];
      const c0 = Math.min(a.col, b.col);
      const c1 = Math.max(a.col, b.col);
      const r0 = Math.min(a.row, b.row);
      const r1 = Math.max(a.row, b.row);
      const out: string[] = [];
      for (let r = r0; r <= r1; r += 1) {
        for (let c = c0; c <= c1; c += 1) {
          out.push(toAddress({ col: c, row: r }));
        }
      }
      return out;
    },

    textAnchor(addr, isNumber) {
      const c = fromAddress(addr);
      if (!c) return null;
      const { x, y } = rectFor(c);
      return isNumber
        ? { x: x + cellSize - 6, y: y + cellSize / 2, align: 'right' }
        : { x: x + 6, y: y + cellSize / 2, align: 'left' };
    },

    canonicalAddress(addr) {
      const c = fromAddress(addr);
      return c ? toAddress(c) : null;
    },
  };
}
