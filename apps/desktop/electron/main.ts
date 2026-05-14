/**
 * Electron main process.
 *
 * Owns:
 *  - the browser window
 *  - the lifecycle of the Rust core subprocess (`tescellate-core` binary)
 *  - JSON-RPC frame multiplexing between renderer and core
 *
 * The renderer never speaks to the core directly; everything goes through
 * IPC channels here. This keeps the renderer free of Node APIs and lets us
 * swap the transport (stdio → socket) without touching the UI.
 */

import { app, BrowserWindow, dialog, ipcMain, Menu, type MenuItemConstructorOptions } from 'electron';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

let mainWindow: BrowserWindow | null = null;
let core: ChildProcessWithoutNullStreams | null = null;
let currentPath: string | null = null;

// LSP-style framing state.
let rxBuffer = Buffer.alloc(0);
let rxExpectedLength: number | null = null;

// Map JSON-RPC ids to renderer reply channels.
let nextRequestId = 1;
const pending = new Map<number, (msg: unknown) => void>();

function locateCoreBinary(): string {
  // In dev we expect `cargo build` to have produced the debug binary.
  // In a packaged app it will be alongside the resources; that path is
  // resolved in Phase 1+ when we wire up electron-builder.
  const isDev = !app.isPackaged;
  const ext = process.platform === 'win32' ? '.exe' : '';
  if (isDev) {
    return resolve(__dirname, '..', '..', '..', '..', 'target', 'debug', `tescellate-core${ext}`);
  }
  return resolve(process.resourcesPath, 'core', `tescellate-core${ext}`);
}

function startCore() {
  const bin = locateCoreBinary();
  core = spawn(bin, [], { stdio: ['pipe', 'pipe', 'pipe'] });

  core.stdout.on('data', onCoreData);
  core.stderr.on('data', (chunk: Buffer) => {
    // Surface stderr to the dev console; in Phase 1 we'll add structured logging.
    console.error('[core]', chunk.toString('utf8').trimEnd());
  });
  core.on('exit', (code) => {
    console.error(`[core] exited with code ${code}`);
    core = null;
  });
}

function onCoreData(chunk: Buffer) {
  rxBuffer = Buffer.concat([rxBuffer, chunk]);
  while (true) {
    if (rxExpectedLength === null) {
      const headerEnd = rxBuffer.indexOf('\r\n\r\n');
      if (headerEnd < 0) return;
      const header = rxBuffer.slice(0, headerEnd).toString('utf8');
      const match = /Content-Length:\s*(\d+)/i.exec(header);
      if (!match) {
        // Malformed; drop the malformed header bytes and continue.
        rxBuffer = rxBuffer.slice(headerEnd + 4);
        continue;
      }
      rxExpectedLength = parseInt(match[1], 10);
      rxBuffer = rxBuffer.slice(headerEnd + 4);
    }
    if (rxBuffer.length < rxExpectedLength) return;
    const body = rxBuffer.slice(0, rxExpectedLength);
    rxBuffer = rxBuffer.slice(rxExpectedLength);
    rxExpectedLength = null;
    try {
      const msg = JSON.parse(body.toString('utf8'));
      const id = typeof msg?.id === 'number' ? msg.id : null;
      if (id !== null && pending.has(id)) {
        pending.get(id)!(msg);
        pending.delete(id);
      } else if (mainWindow) {
        // Server-initiated notification — forward to renderer.
        mainWindow.webContents.send('core:notification', msg);
      }
    } catch (e) {
      console.error('[core] failed to parse frame:', e);
    }
  }
}

function sendCoreRequest(method: string, params: unknown): Promise<unknown> {
  if (!core) throw new Error('core process not running');
  const id = nextRequestId++;
  const body = JSON.stringify({ jsonrpc: '2.0', id, method, params });
  const frame = `Content-Length: ${Buffer.byteLength(body, 'utf8')}\r\n\r\n${body}`;
  return new Promise((resolveResp) => {
    pending.set(id, (msg) => resolveResp(msg));
    core!.stdin.write(frame, 'utf8');
  });
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 800,
    webPreferences: {
      preload: join(__dirname, '..', 'preload', 'preload.mjs'),
      contextIsolation: true,
      nodeIntegration: false,
      // ESM preload requires sandbox: false. contextIsolation:true is
      // the load-bearing security boundary either way.
      sandbox: false,
    },
  });

  if (process.env.ELECTRON_RENDERER_URL) {
    mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL);
    mainWindow.webContents.openDevTools({ mode: 'right' });
  } else {
    mainWindow.loadFile(join(__dirname, '..', 'renderer', 'index.html'));
  }
}

ipcMain.handle('core:request', async (_evt, payload: { method: string; params: unknown }) => {
  return sendCoreRequest(payload.method, payload.params);
});

function updateTitle() {
  if (!mainWindow) return;
  const name = currentPath ? currentPath.replace(/^.*[\\/]/, '') : 'untitled';
  mainWindow.setTitle(`${name} — Tescellate`);
}

async function openWorkbook() {
  if (!mainWindow) return;
  const r = await dialog.showOpenDialog(mainWindow, {
    title: 'Open workbook',
    properties: ['openFile'],
    filters: [
      { name: 'Tescellate workbook', extensions: ['tscl'] },
      { name: 'All files', extensions: ['*'] },
    ],
  });
  if (r.canceled || r.filePaths.length === 0) return;
  const path = r.filePaths[0];
  const resp = (await sendCoreRequest('workbook.open', { path })) as {
    error?: { message: string };
  };
  if (resp.error) {
    await dialog.showMessageBox(mainWindow, {
      type: 'error',
      message: 'Could not open workbook',
      detail: resp.error.message,
    });
    return;
  }
  currentPath = path;
  updateTitle();
  mainWindow.webContents.send('workbook:opened', { path });
}

async function saveWorkbook(saveAs: boolean) {
  if (!mainWindow) return;
  let path = currentPath;
  if (saveAs || !path) {
    const r = await dialog.showSaveDialog(mainWindow, {
      title: 'Save workbook',
      defaultPath: path ?? 'workbook.tscl',
      filters: [{ name: 'Tescellate workbook', extensions: ['tscl'] }],
    });
    if (r.canceled || !r.filePath) return;
    path = r.filePath;
  }
  const resp = (await sendCoreRequest('workbook.save', { path })) as {
    error?: { message: string };
  };
  if (resp.error) {
    await dialog.showMessageBox(mainWindow, {
      type: 'error',
      message: 'Could not save workbook',
      detail: resp.error.message,
    });
    return;
  }
  currentPath = path;
  updateTitle();
}

function buildMenu() {
  const isMac = process.platform === 'darwin';
  const fileSubmenu: MenuItemConstructorOptions[] = [
    {
      label: 'New',
      accelerator: 'CmdOrCtrl+N',
      click: async () => {
        await sendCoreRequest('workbook.new', {});
        currentPath = null;
        updateTitle();
        mainWindow?.webContents.send('workbook:opened', { path: null });
      },
    },
    {
      label: 'Open…',
      accelerator: 'CmdOrCtrl+O',
      click: () => void openWorkbook(),
    },
    { type: 'separator' },
    {
      label: 'Save',
      accelerator: 'CmdOrCtrl+S',
      click: () => void saveWorkbook(false),
    },
    {
      label: 'Save As…',
      accelerator: 'CmdOrCtrl+Shift+S',
      click: () => void saveWorkbook(true),
    },
    { type: 'separator' },
    { role: isMac ? 'close' : 'quit' },
  ];
  const template: MenuItemConstructorOptions[] = [
    ...(isMac ? [{ role: 'appMenu' as const }] : []),
    { label: 'File', submenu: fileSubmenu },
    { role: 'editMenu' },
    { role: 'viewMenu' },
    { role: 'windowMenu' },
  ];
  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

app.whenReady().then(() => {
  startCore();
  createWindow();
  buildMenu();
  updateTitle();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (core) {
    core.kill();
    core = null;
  }
  if (process.platform !== 'darwin') app.quit();
});
