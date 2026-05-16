/**
 * Hex-lattice view — axial `(q, r)` addressing per Red Blob Games
 * convention, mirroring `tescellate-tess::hex` on the Rust side.
 *
 * Pointy-top: vertices at 30°/90°/…; flat-top: vertices at 0°/60°/….
 * The hex's `hex_size` (circumradius) is derived from the requested
 * cell pitch so a single cell paints to roughly the area of a square
 * cell at the same `cellSize`.
 *
 * Negative axial coords are allowed (`H(-1, 0)`, etc.) and rendered.
 * Until camera/scroll lands the viewport is centered on the origin,
 * so a hex sheet shows H(0,0) somewhere near the upper-left and the
 * user can interact with everything that fits on screen.
 */

import type { LatticeView } from './types';

interface HexCoord {
  q: number;
  r: number;
}

function parseHex(addr: string): HexCoord | null {
  const m = /^H\(\s*(-?\d+)\s*,\s*(-?\d+)\s*\)$/.exec(addr.trim());
  if (!m) return null;
  return { q: parseInt(m[1], 10), r: parseInt(m[2], 10) };
}

function formatHex(c: HexCoord): string {
  return `H(${c.q},${c.r})`;
}

/** Cube-round fractional axial to nearest hex — mirrors
 * `axial_round` in `tescellate-tess::hex`. */
function axialRound(qFrac: number, rFrac: number): HexCoord {
  const sFrac = -qFrac - rFrac;
  let q = Math.round(qFrac);
  let r = Math.round(rFrac);
  const s = Math.round(sFrac);
  const qDiff = Math.abs(q - qFrac);
  const rDiff = Math.abs(r - rFrac);
  const sDiff = Math.abs(s - sFrac);
  if (qDiff > rDiff && qDiff > sDiff) {
    q = -r - s;
  } else if (rDiff > sDiff) {
    r = -q - s;
  }
  return { q, r };
}

export function createHexLattice(cellSize: number, orientation: 'pointy' | 'flat'): LatticeView {
  // Hex circumradius. Picking cellSize / 2 keeps the visual cell pitch
  // roughly comparable to a square sheet at the same cellSize.
  const hex = cellSize / 2;
  const sqrt3 = Math.sqrt(3);

  // No header gutter — hex sheets aren't row/column-indexable. Origin
  // padding leaves room for negative-q / negative-r cells to render
  // into the visible area without scrolling.
  const padX = orientation === 'pointy' ? hex * sqrt3 * 4 : hex * 6;
  const padY = orientation === 'pointy' ? hex * 1.5 * 4 : hex * sqrt3 * 4;

  function centroidPixels(c: HexCoord): [number, number] {
    if (orientation === 'pointy') {
      return [
        padX + hex * (sqrt3 * c.q + (sqrt3 / 2) * c.r),
        padY + hex * (1.5 * c.r),
      ];
    }
    return [
      padX + hex * (1.5 * c.q),
      padY + hex * ((sqrt3 / 2) * c.q + sqrt3 * c.r),
    ];
  }

  function pixelToCoord(x: number, y: number): HexCoord {
    const px = x - padX;
    const py = y - padY;
    let qFrac: number;
    let rFrac: number;
    if (orientation === 'pointy') {
      qFrac = ((sqrt3 / 3) * px - py / 3) / hex;
      rFrac = ((2 / 3) * py) / hex;
    } else {
      qFrac = ((2 / 3) * px) / hex;
      rFrac = (-px / 3 + (sqrt3 / 3) * py) / hex;
    }
    return axialRound(qFrac, rFrac);
  }

  function visibleCoords(w: number, h: number): HexCoord[] {
    // Bound q, r so every centroid in the viewport is covered plus a
    // one-cell margin. Simple analytical bounds rather than a per-pixel
    // sweep: walk a generous (q, r) box and discard centroids outside
    // the viewport.
    const margin = hex * 2;
    const minX = -margin;
    const maxX = w + margin;
    const minY = -margin;
    const maxY = h + margin;
    const c0 = pixelToCoord(minX, minY);
    const c1 = pixelToCoord(maxX, minY);
    const c2 = pixelToCoord(minX, maxY);
    const c3 = pixelToCoord(maxX, maxY);
    const qMin = Math.min(c0.q, c1.q, c2.q, c3.q) - 1;
    const qMax = Math.max(c0.q, c1.q, c2.q, c3.q) + 1;
    const rMin = Math.min(c0.r, c1.r, c2.r, c3.r) - 1;
    const rMax = Math.max(c0.r, c1.r, c2.r, c3.r) + 1;
    const out: HexCoord[] = [];
    for (let r = rMin; r <= rMax; r += 1) {
      for (let q = qMin; q <= qMax; q += 1) {
        const [cx, cy] = centroidPixels({ q, r });
        if (cx >= minX && cx <= maxX && cy >= minY && cy <= maxY) {
          out.push({ q, r });
        }
      }
    }
    return out;
  }

  function tracePath(ctx: CanvasRenderingContext2D, c: HexCoord) {
    const [cx, cy] = centroidPixels(c);
    for (let i = 0; i < 6; i += 1) {
      const angleDeg =
        orientation === 'pointy' ? 60 * i - 30 : 60 * i;
      const angle = (angleDeg * Math.PI) / 180;
      const x = cx + hex * Math.cos(angle);
      const y = cy + hex * Math.sin(angle);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.closePath();
  }

  return {
    kind: orientation === 'pointy' ? 'hex_pointy' : 'hex_flat',
    headerWidth: 0,
    headerHeight: 0,

    cellAtPixel(x, y) {
      return formatHex(pixelToCoord(x, y));
    },

    cellCentroid(addr) {
      const c = parseHex(addr);
      return c ? centroidPixels(c) : null;
    },

    cellBBox(addr) {
      const c = parseHex(addr);
      if (!c) return null;
      const [cx, cy] = centroidPixels(c);
      // For pointy-top: width = sqrt3 * hex, height = 2 * hex.
      // For flat-top : width = 2 * hex,       height = sqrt3 * hex.
      const w = orientation === 'pointy' ? sqrt3 * hex : 2 * hex;
      const h = orientation === 'pointy' ? 2 * hex : sqrt3 * hex;
      return { x: cx - w / 2, y: cy - h / 2, width: w, height: h };
    },

    pathCell(ctx, addr) {
      const c = parseHex(addr);
      if (!c) return;
      tracePath(ctx, c);
    },

    visibleAddresses(w, h) {
      return visibleCoords(w, h).map(formatHex);
    },

    drawHeaders(_ctx, _w, _h) {
      // Hex has no row/column gutter. Future: a small q=0 / r=0 axis
      // indicator near the origin so users can orient. Phase 2.5.
    },

    moveAddress(from, dCol, dRow) {
      const c = parseHex(from);
      if (!c) return null;
      // Map the four screen directions to the closest axial moves.
      // Pointy-top: up/down rotate r ; left/right rotate q. Flat-top:
      // up/down rotate r-ish, left/right rotate q-ish, but the screen
      // axes are swapped relative to pointy-top.
      let nq = c.q;
      let nr = c.r;
      if (orientation === 'pointy') {
        nq += dCol;
        nr += dRow;
      } else {
        // Flat-top: q grows screen-rightward (with a slight diagonal),
        // r grows screen-downward (with a slight diagonal).
        nq += dCol;
        nr += dRow;
      }
      return formatHex({ q: nq, r: nr });
    },

    rangeAddresses(start, end) {
      const a = parseHex(start);
      const b = parseHex(end);
      if (!a || !b) return [];
      const q0 = Math.min(a.q, b.q);
      const q1 = Math.max(a.q, b.q);
      const r0 = Math.min(a.r, b.r);
      const r1 = Math.max(a.r, b.r);
      const out: string[] = [];
      for (let r = r0; r <= r1; r += 1) {
        for (let q = q0; q <= q1; q += 1) {
          out.push(formatHex({ q, r }));
        }
      }
      return out;
    },

    textAnchor(addr, _isNumber) {
      const c = parseHex(addr);
      if (!c) return null;
      const [cx, cy] = centroidPixels(c);
      // Hex cells are too narrow for a left/right shift to feel
      // natural — center every value type.
      return { x: cx, y: cy, align: 'center' };
    },

    canonicalAddress(addr) {
      const c = parseHex(addr);
      return c ? formatHex(c) : null;
    },
  };
}
