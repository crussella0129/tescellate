/**
 * New-workbook wizard. Three steps:
 *   1. Tessellation — pick the cell-shape lattice (only square is wired up
 *      for Phase 1; the others render as "Phase 2+" placeholders so the
 *      user can see what's coming).
 *   2. Extent — Unbounded (sparse, infinite scroll) or Bounded with cols and
 *      rows expressed as live-evaluated arithmetic formulas (so `100*100`
 *      and `10000` are equivalent).
 *   3. Name + Create.
 *
 * The arithmetic inputs round-trip through `formula.eval` so they share the
 * same parser/evaluator the cells use. No duplicated arithmetic on the JS
 * side; whatever Excel-lite accepts, the wizard accepts.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { ipc, type LatticeKind, type ExtentParams } from '../ipc';

interface Props {
  onComplete: (result: { sheet: number; name: string }) => void;
  onCancel: () => void;
}

const TILINGS: Array<{
  kind: LatticeKind;
  label: string;
  glyph: string;
  description: string;
  phase: number;
}> = [
  {
    kind: 'square',
    label: 'Square',
    glyph: '▦',
    description: 'Classic spreadsheet grid. 4 neighbours.',
    phase: 1,
  },
  {
    kind: 'hex_pointy',
    label: 'Hex (pointy)',
    glyph: '⬢',
    description: 'Hexagons, point at the top. 6 neighbours.',
    phase: 2,
  },
  {
    kind: 'hex_flat',
    label: 'Hex (flat)',
    glyph: '⬣',
    description: 'Hexagons, flat at the top. 6 neighbours.',
    phase: 2,
  },
  {
    kind: 'triangle',
    label: 'Triangle',
    glyph: '▲',
    description: 'Alternating up/down triangles. 3 neighbours.',
    phase: 3,
  },
  {
    kind: 'parallelogram',
    label: 'Parallelogram',
    glyph: '▱',
    description: 'Skewed grid for isometric layouts.',
    phase: 3,
  },
];

/** Evaluate an arithmetic expression in the core. Returns the integer
 * value (rounded toward zero) on success, or an error message. */
async function evalToInt(source: string): Promise<{ ok: true; value: number } | { ok: false; error: string }> {
  if (!source.trim()) return { ok: false, error: 'enter a value' };
  try {
    const v = await ipc.evalLiteral(source);
    let n: number;
    if (v.kind === 'number') n = v.value;
    else if (v.kind === 'integer') n = v.value;
    else if (v.kind === 'bool') n = v.value ? 1 : 0;
    else return { ok: false, error: `expected a number, got ${v.kind}` };
    if (!Number.isFinite(n)) return { ok: false, error: 'not finite' };
    if (n < 1) return { ok: false, error: 'must be at least 1' };
    return { ok: true, value: Math.floor(n) };
  } catch (e) {
    return { ok: false, error: (e as Error).message };
  }
}

export function WizardModal({ onComplete, onCancel }: Props) {
  const [lattice, setLattice] = useState<LatticeKind>('square');
  const [extentMode, setExtentMode] = useState<'unbounded' | 'bounded'>('bounded');
  const [colsInput, setColsInput] = useState('100');
  const [rowsInput, setRowsInput] = useState('100');
  const [colsResult, setColsResult] = useState<Awaited<ReturnType<typeof evalToInt>> | null>(null);
  const [rowsResult, setRowsResult] = useState<Awaited<ReturnType<typeof evalToInt>> | null>(null);
  const [name, setName] = useState('Untitled');
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Live-evaluate the cols/rows inputs against the core.
  useEffect(() => {
    if (extentMode !== 'bounded') {
      setColsResult(null);
      setRowsResult(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      const c = await evalToInt(colsInput);
      if (!cancelled) setColsResult(c);
    })();
    void (async () => {
      const r = await evalToInt(rowsInput);
      if (!cancelled) setRowsResult(r);
    })();
    return () => {
      cancelled = true;
    };
  }, [colsInput, rowsInput, extentMode]);

  const canSubmit = useMemo(() => {
    if (submitting) return false;
    if (!name.trim()) return false;
    if (extentMode === 'unbounded') return true;
    return colsResult?.ok && rowsResult?.ok;
  }, [submitting, name, extentMode, colsResult, rowsResult]);

  const submit = useCallback(async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setSubmitError(null);
    const extent: ExtentParams =
      extentMode === 'unbounded'
        ? { kind: 'unbounded' }
        : {
            kind: 'bounded',
            cols: colsResult!.ok ? colsResult!.value : 100,
            rows: rowsResult!.ok ? rowsResult!.value : 100,
          };
    try {
      const r = await ipc.createWorkbook({ name: name.trim(), lattice, extent });
      onComplete({ sheet: r.sheet, name: r.name });
    } catch (e) {
      setSubmitError((e as Error).message);
      setSubmitting(false);
    }
  }, [canSubmit, name, lattice, extentMode, colsResult, rowsResult, onComplete]);

  return (
    <div className="wizard-overlay" onMouseDown={onCancel}>
      <div className="wizard-modal" onMouseDown={(e) => e.stopPropagation()}>
        <h2>New workbook</h2>

        <section className="wizard-step">
          <header>
            <span className="wizard-step-num">1</span>
            <h3>Tessellation</h3>
          </header>
          <div className="tiling-grid">
            {TILINGS.map((t) => {
              const enabled = t.phase === 1;
              const selected = lattice === t.kind && enabled;
              return (
                <button
                  key={t.kind}
                  type="button"
                  className={`tiling-card ${selected ? 'selected' : ''} ${enabled ? '' : 'disabled'}`}
                  onClick={() => enabled && setLattice(t.kind)}
                  disabled={!enabled}
                  title={enabled ? t.description : `Available in Phase ${t.phase}`}
                >
                  <div className="tiling-glyph">{t.glyph}</div>
                  <div className="tiling-label">{t.label}</div>
                  {!enabled && <div className="tiling-badge">Phase {t.phase}+</div>}
                </button>
              );
            })}
          </div>
        </section>

        <section className="wizard-step">
          <header>
            <span className="wizard-step-num">2</span>
            <h3>Extent</h3>
          </header>
          <div className="extent-toggle">
            <label>
              <input
                type="radio"
                name="extent"
                checked={extentMode === 'bounded'}
                onChange={() => setExtentMode('bounded')}
              />
              <span>Bounded</span>
              <span className="hint">— fixed cols × rows; out-of-bounds writes are rejected</span>
            </label>
            <label>
              <input
                type="radio"
                name="extent"
                checked={extentMode === 'unbounded'}
                onChange={() => setExtentMode('unbounded')}
              />
              <span>Unbounded</span>
              <span className="hint">— sparse, infinite scroll, only populated cells exist</span>
            </label>
          </div>
          {extentMode === 'bounded' && (
            <div className="bounded-fields">
              <FormulaField label="Cols" value={colsInput} onChange={setColsInput} result={colsResult} />
              <FormulaField label="Rows" value={rowsInput} onChange={setRowsInput} result={rowsResult} />
              <div className="bounded-presets">
                <button type="button" onClick={() => { setColsInput('26'); setRowsInput('100'); }}>
                  Small (26 × 100)
                </button>
                <button type="button" onClick={() => { setColsInput('100'); setRowsInput('500'); }}>
                  Medium (100 × 500)
                </button>
                <button type="button" onClick={() => { setColsInput('1000'); setRowsInput('5000'); }}>
                  Large (1000 × 5000)
                </button>
              </div>
            </div>
          )}
        </section>

        <section className="wizard-step">
          <header>
            <span className="wizard-step-num">3</span>
            <h3>Name</h3>
          </header>
          <input
            type="text"
            className="wizard-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Untitled"
          />
        </section>

        {submitError && <div className="wizard-error">{submitError}</div>}

        <footer className="wizard-actions">
          <button type="button" onClick={onCancel} className="ghost">Cancel</button>
          <button type="button" onClick={submit} disabled={!canSubmit} className="primary">
            {submitting ? 'Creating…' : 'Create workbook'}
          </button>
        </footer>
      </div>
    </div>
  );
}

interface FormulaFieldProps {
  label: string;
  value: string;
  onChange: (v: string) => void;
  result: Awaited<ReturnType<typeof evalToInt>> | null;
}

function FormulaField({ label, value, onChange, result }: FormulaFieldProps) {
  const status =
    result?.ok === true
      ? { mark: '✓', text: result.value.toLocaleString(), good: true }
      : result?.ok === false
        ? { mark: '✕', text: result.error, good: false }
        : { mark: '…', text: '', good: false };
  return (
    <label className="formula-field">
      <span className="formula-field-label">{label}</span>
      <input
        type="text"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      <span className={`formula-field-status ${status.good ? 'good' : 'bad'}`}>
        {status.mark} {status.text}
      </span>
    </label>
  );
}
