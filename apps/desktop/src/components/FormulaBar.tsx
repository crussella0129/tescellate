import { forwardRef } from 'react';
import type { EngineKind } from '../types';

export interface FormulaBarProps {
  engine: EngineKind;
  onEngineChange: (e: EngineKind) => void;
  /** Canonical address of the active (editable) cell, e.g. "A1". */
  address: string | null;
  /** Draft text the user is editing. Controlled by the parent so cell-clicks
   * can insert refs at the caret position. */
  draft: string;
  onDraftChange: (s: string) => void;
  /** Fired the first time the user interacts with the input. */
  onFocus: () => void;
  /** Fired on Enter (true) / blur (true) — parent commits. */
  onCommit: () => void;
  /** Fired on Escape — parent reverts. */
  onCancel: () => void;
  /** If the active cell is a spill target, this is its source cell's address. */
  spilledFrom: string | null;
}

const ENGINE_LABELS: Record<EngineKind, string> = {
  excel_lite: 'xl',
  python: 'py',
  rhai: 'rs (preview)',
  rust_native: 'rs (native)',
};

export const FormulaBar = forwardRef<HTMLInputElement, FormulaBarProps>(function FormulaBar(
  {
    engine,
    onEngineChange,
    address,
    draft,
    onDraftChange,
    onFocus,
    onCommit,
    onCancel,
    spilledFrom,
  },
  ref,
) {
  return (
    <div className="formula-bar">
      <span className="cell-label">{address ?? '—'}</span>
      {spilledFrom && (
        <span className="spill-tag" title={`Spilled from ${spilledFrom}`}>
          ↺ {spilledFrom}
        </span>
      )}
      <select
        className="engine-chip"
        value={engine}
        onChange={(e) => onEngineChange(e.target.value as EngineKind)}
        title="Formula language for this cell"
      >
        {(Object.keys(ENGINE_LABELS) as EngineKind[]).map((k) => (
          <option key={k} value={k}>
            {ENGINE_LABELS[k]}
          </option>
        ))}
      </select>
      <input
        ref={ref}
        className="formula-input"
        spellCheck={false}
        placeholder="Enter a formula or value (= prefix optional)"
        value={draft}
        onChange={(e) => onDraftChange(e.target.value)}
        onFocus={onFocus}
        onBlur={(e) => {
          // If focus is moving to the grid canvas, the user is mid-edit and
          // picking a cell to insert as a reference — keep editing alive
          // and don't commit. Without this guard the blur fires before the
          // canvas click handler, flipping editing to false, and the click
          // ends up moving selection instead of inserting an A1 ref.
          const next = e.relatedTarget as HTMLElement | null;
          if (next && next.tagName === 'CANVAS') return;
          onCommit();
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            onCommit();
            e.currentTarget.blur();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            onCancel();
            e.currentTarget.blur();
          }
        }}
      />
    </div>
  );
});
