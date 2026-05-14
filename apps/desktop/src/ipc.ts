/**
 * Typed wrapper over `window.tescellate.coreRequest`. Each method here
 * maps 1:1 to a JSON-RPC method handled in `crates/tescellate-cli/src/main.rs`.
 */

export type CellValue =
  | { kind: 'empty' }
  | { kind: 'number'; value: number }
  | { kind: 'integer'; value: number }
  | { kind: 'bool'; value: boolean }
  | { kind: 'text'; value: string }
  | { kind: 'error'; value: { code: string; detail?: string } }
  | { kind: 'pending' };

export interface CellSnapshot {
  address: string;
  source: string | null;
  value: CellValue;
}

interface RpcResponse<T> {
  result?: T;
  error?: { code: number; message: string };
}

async function call<T>(method: string, params: unknown = {}): Promise<T> {
  const resp = (await window.tescellate.coreRequest({ method, params })) as RpcResponse<T>;
  if (resp.error) {
    throw new Error(`${method}: ${resp.error.message}`);
  }
  return resp.result as T;
}

export const ipc = {
  ping: () => call<{ ok: boolean; echo: unknown }>('ping'),
  newWorkbook: () => call<{ sheet: number }>('workbook.new'),
  setCell: (sheet: number, address: string, source: string | null) =>
    call<CellSnapshot[]>('cell.set', { sheet, address, source }),
  getCell: (sheet: number, address: string) =>
    call<CellSnapshot | null>('cell.get', { sheet, address }),
  snapshotRange: (sheet: number, start: string, end: string) =>
    call<CellSnapshot[]>('range.snapshot', { sheet, start, end }),
};

/** Format a `CellValue` for display in a cell. */
export function formatValue(v: CellValue): string {
  switch (v.kind) {
    case 'empty':
      return '';
    case 'number': {
      const n = v.value;
      if (Number.isInteger(n) && Math.abs(n) < 1e16) return String(n);
      return String(n);
    }
    case 'integer':
      return String(v.value);
    case 'bool':
      return v.value ? 'TRUE' : 'FALSE';
    case 'text':
      return v.value;
    case 'error':
      return errorLabel(v.value.code);
    case 'pending':
      return '…';
  }
}

function errorLabel(code: string): string {
  switch (code) {
    case 'ref':
      return '#REF!';
    case 'cycle':
      return '#CYCLE!';
    case 'div_zero':
      return '#DIV/0!';
    case 'num':
      return '#NUM!';
    case 'value':
      return '#VALUE!';
    case 'lang':
      return '#LANG!';
    case 'compile':
      return '#COMPILE!';
    case 'timeout':
      return '#TIMEOUT!';
    default:
      return `#${code.toUpperCase()}!`;
  }
}
