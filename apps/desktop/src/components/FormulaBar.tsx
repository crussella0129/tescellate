import { useEffect, useRef, useState } from 'react';
import type { EngineKind } from '../types';

interface Props {
  engine: EngineKind;
  onEngineChange: (e: EngineKind) => void;
  /** Canonical address of the selected cell, e.g. "A1". */
  address: string | null;
  /** Current source (formula text) of the selected cell, if any. */
  source: string;
  /** If the selected cell is a spill target, this is its source cell's
   * address. The bar surfaces a chip pointing at it. */
  spilledFrom: string | null;
  /** Called when the user commits the edit (Enter / blur). */
  onCommit: (src: string) => void;
}

const ENGINE_LABELS: Record<EngineKind, string> = {
  excel_lite: 'xl',
  python: 'py',
  rhai: 'rs (preview)',
  rust_native: 'rs (native)',
};

export function FormulaBar({
  engine,
  onEngineChange,
  address,
  source,
  spilledFrom,
  onCommit,
}: Props) {
  const [draft, setDraft] = useState(source);
  const inputRef = useRef<HTMLInputElement>(null);

  // When the selected cell changes upstream, replace the draft.
  useEffect(() => {
    setDraft(source);
  }, [source, address]);

  const commit = () => {
    if (draft !== source) {
      onCommit(draft);
    }
  };

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
        ref={inputRef}
        className="formula-input"
        spellCheck={false}
        placeholder="Enter formula, e.g. =SUM(A1:A10)"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            commit();
            e.currentTarget.blur();
          } else if (e.key === 'Escape') {
            setDraft(source);
            e.currentTarget.blur();
          }
        }}
      />
    </div>
  );
}
