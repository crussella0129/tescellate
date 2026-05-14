/**
 * Preload bridge — exposes a narrow, typed surface to the renderer for
 * talking to the Rust core via the main process. The renderer has no
 * direct access to Node, ipcRenderer, or the core subprocess.
 */

import { contextBridge, ipcRenderer } from 'electron';

type CoreRequest = { method: string; params: unknown };

const api = {
  coreRequest: (payload: CoreRequest) => ipcRenderer.invoke('core:request', payload),
  onCoreNotification: (cb: (msg: unknown) => void): (() => void) => {
    const listener = (_evt: unknown, msg: unknown) => cb(msg);
    ipcRenderer.on('core:notification', listener);
    return () => {
      ipcRenderer.off('core:notification', listener);
    };
  },
  /** Fired when the main process opens a workbook (via File menu) or
   * resets to a fresh untitled workbook. Renderer should re-snapshot. */
  onWorkbookOpened: (cb: (payload: { path: string | null }) => void): (() => void) => {
    const listener = (_evt: unknown, payload: { path: string | null }) => cb(payload);
    ipcRenderer.on('workbook:opened', listener);
    return () => {
      ipcRenderer.off('workbook:opened', listener);
    };
  },
};

contextBridge.exposeInMainWorld('tescellate', api);

export type TescellateApi = typeof api;
