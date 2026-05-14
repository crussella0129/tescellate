import type { EngineKind } from '../types';

interface Props {
  engine: EngineKind;
  onEngineChange: (e: EngineKind) => void;
  value: string;
  onChange: (v: string) => void;
}

const ENGINE_LABELS: Record<EngineKind, string> = {
  excel_lite: 'xl',
  python: 'py',
  rhai: 'rs (preview)',
  rust_native: 'rs (native)',
};

export function FormulaBar({ engine, onEngineChange, value, onChange }: Props) {
  return (
    <div className="formula-bar">
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
        className="formula-input"
        spellCheck={false}
        placeholder="Enter formula, e.g. =SUM(A1:A10)"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}
