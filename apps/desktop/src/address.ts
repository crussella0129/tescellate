/**
 * Square-lattice address helpers, mirroring `tescellate-tess::square`.
 * The renderer needs to convert between (col, row) and the canonical
 * `A1` strings the core uses for cell keys.
 */

export interface Coord {
  col: number;
  row: number;
}

export function toAddress(c: Coord): string {
  if (c.col < 0) {
    // Mirrors the bracketed internal form on the Rust side.
    return `[c${c.col},r${c.row + 1}]`;
  }
  let n = c.col;
  let s = '';
  // eslint-disable-next-line no-constant-condition
  while (true) {
    s = String.fromCharCode(65 + (n % 26)) + s;
    n = Math.floor(n / 26) - 1;
    if (n < 0) break;
  }
  return `${s}${c.row + 1}`;
}

export function fromAddress(addr: string): Coord | null {
  const match = /^([A-Z]+)([0-9]+)$/.exec(addr);
  if (!match) return null;
  const letters = match[1];
  let col = 0;
  for (const ch of letters) {
    col = col * 26 + (ch.charCodeAt(0) - 64);
  }
  return { col: col - 1, row: parseInt(match[2], 10) - 1 };
}
