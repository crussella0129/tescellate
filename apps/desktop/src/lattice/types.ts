/**
 * Renderer-facing lattice abstraction. Mirrors the role
 * `tescellate-tess::LatticeHandle` plays on the Rust side: the
 * `GridCanvas` only talks to a `LatticeView`, never to a specific
 * lattice. Two implementations today (`square`, `hex`); future
 * tessellations slot in here.
 *
 * All cell identifiers are address *strings* — same convention the
 * Rust core uses for cell keys. No coordinate types leak across the
 * `GridCanvas` ↔ `App` seam.
 */

import type { LatticeKind } from '../types';

export interface LatticeView {
  kind: LatticeKind;

  /** Width / height in pixels of the row-number / column-letter gutter
   * that surrounds the visible cell area. Hex sheets have no gutter
   * (`headerWidth = headerHeight = 0`). */
  readonly headerWidth: number;
  readonly headerHeight: number;

  /** Convert a pixel position (relative to the canvas top-left) to a
   * cell address. Returns `null` when the position is outside any cell
   * (e.g. inside the header gutter on a square sheet). */
  cellAtPixel(x: number, y: number): string | null;

  /** Centroid of `addr` in canvas pixels. `null` if the address is
   * unparseable for this lattice. */
  cellCentroid(addr: string): [number, number] | null;

  /** Axis-aligned bounding box for the cell. Used to clip rendered
   * text. */
  cellBBox(addr: string): { x: number; y: number; width: number; height: number } | null;

  /** Trace the cell's outline into the canvas's current path. Caller
   * is responsible for `beginPath()`, `fill()` / `stroke()`. */
  pathCell(ctx: CanvasRenderingContext2D, addr: string): void;

  /** Every cell address whose centroid lies within the viewport. The
   * renderer iterates this for value rendering and grid lines. */
  visibleAddresses(viewportWidth: number, viewportHeight: number): string[];

  /** Render the column/row headers. Square draws labels; hex no-ops. */
  drawHeaders(ctx: CanvasRenderingContext2D, viewportWidth: number, viewportHeight: number): void;

  /** Apply a directional move. The `dCol` / `dRow` model is square-
   * lattice native; each lattice maps it to its own neighbors. Hex
   * uses these as "screen up/down/left/right". */
  moveAddress(from: string, dCol: number, dRow: number): string | null;

  /** Enumerate every cell address inside the range `start:end`. Square
   * = rectangle. Hex = axial-aligned parallelogram (so the hull may
   * include cells the user didn't drag *through* — same semantics as
   * `tescellate-tess::axial_parallelogram`). */
  rangeAddresses(start: string, end: string): string[];

  /** Text-anchor hint for drawing a cell's value. The alignment is per-
   * cell (numbers usually right-aligned), so the lattice picks the
   * baseline pixel. */
  textAnchor(
    addr: string,
    isNumber: boolean,
  ): { x: number; y: number; align: 'left' | 'right' | 'center' } | null;

  /** Format an address back to its canonical form (square `A1`, hex
   * `H(q,r)`). Used by the formula-bar address chip. */
  canonicalAddress(addr: string): string | null;
}
