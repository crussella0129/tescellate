import { useCallback, useState } from 'react';
import { FormulaBar } from './components/FormulaBar';
import { GridCanvas } from './components/GridCanvas';
import type { EngineKind } from './types';

export function App() {
  const [engine, setEngine] = useState<EngineKind>('excel_lite');
  const [formula, setFormula] = useState('');
  const [pingResult, setPingResult] = useState<string>('(not pinged)');

  const pingCore = useCallback(async () => {
    try {
      const resp = (await window.tescellate.coreRequest({
        method: 'ping',
        params: { from: 'renderer', at: Date.now() },
      })) as { result?: unknown; error?: { message: string } };
      if (resp.error) {
        setPingResult(`error: ${resp.error.message}`);
      } else {
        setPingResult(JSON.stringify(resp.result));
      }
    } catch (e) {
      setPingResult(`exception: ${(e as Error).message}`);
    }
  }, []);

  return (
    <>
      <FormulaBar
        engine={engine}
        onEngineChange={setEngine}
        value={formula}
        onChange={setFormula}
      />
      <div style={{ flex: 1, position: 'relative' }}>
        <GridCanvas />
        <div className="ping-panel">
          <button onClick={pingCore}>Ping Rust core</button>
          <code>{pingResult}</code>
        </div>
      </div>
    </>
  );
}
