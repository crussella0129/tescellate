import type { LatticeKind } from '../types';
import { createHexLattice } from './hex';
import { createSquareLattice } from './square';
import type { LatticeView } from './types';

export type { LatticeView } from './types';
export { createSquareLattice } from './square';
export { createHexLattice } from './hex';

/** Construct the canonical `LatticeView` for a given lattice kind, or
 * `null` for lattices that don't yet have a renderer impl. Mirrors
 * `tescellate-tess::LatticeHandle::for_kind`. */
export function createLatticeView(kind: LatticeKind, cellSize: number): LatticeView | null {
  switch (kind) {
    case 'square':
      return createSquareLattice(cellSize);
    case 'hex_pointy':
      return createHexLattice(cellSize, 'pointy');
    case 'hex_flat':
      return createHexLattice(cellSize, 'flat');
    case 'triangle':
    case 'parallelogram':
      // Phase 3+.
      return null;
  }
}
