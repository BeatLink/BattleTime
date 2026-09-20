// The WaspOS Companion desktop app.
//
// It runs the same interface as the phone app, served from the web export,
// with Bluetooth handled here in the main process.

const { app, BrowserWindow, ipcMain, shell } = require('electron');
const { join } = require('node:path');

const { WatchBle } = require('../node/ble');
const { serve } = require('./server');

const DIST = join(__dirname, '..', 'dist');

let window_ = null;
let ble = null;
let server = null;

function emit(name, payload) {
  if (window_ && !window_.isDestroyed()) {
    window_.webContents.send('ble:event', name, payload);
  }
}

async function createWindow() {
  // In development the Expo dev server is used, so the interface reloads.
  const devUrl = process.env.WASPOS_DEV_URL;
  let url = devUrl;
  if (!url) {
    const started = await serve(DIST);
    server = started.server;
    url = started.url;
  }

  window_ = new BrowserWindow({
    width: 480,
    height: 860,
    backgroundColor: '#000000',
    title: 'WaspOS Companion',
    autoHideMenuBar: true,
    webPreferences: {
      preload: join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  // Anything aimed elsewhere opens in the real browser, not in the app.
  window_.webContents.setWindowOpenHandler(({ url: target }) => {
    shell.openExternal(target);
    return { action: 'deny' };
  });

  await window_.loadURL(url);
}

function wireBluetooth() {
  ble = new WatchBle(emit);

  const handle = (channel, action) =>
    ipcMain.handle(channel, async (_event, ...args) => {
      try {
        return { ok: true, value: await action(...args) };
      } catch (error) {
        return { ok: false, error: String(error.message || error) };
      }
    });

  handle('ble:start-scan', () => ble.startScan());
  handle('ble:stop-scan', () => ble.stopScan());
  handle('ble:connect', (id) => ble.connect(id));
  handle('ble:disconnect', () => ble.disconnect());
  handle('ble:write', (text) => ble.write(text));
  handle('ble:gatt-has', (service, char) => ble.gattHas(service, char));
  handle('ble:gatt-read', (service, char) => ble.gattRead(service, char));
  handle('ble:gatt-write', (service, char, data, mode) =>
    ble.gattWrite(service, char, data, mode));
  handle('ble:gatt-subscribe', (service, char) => ble.gattSubscribe(service, char));
  handle('ble:gatt-unsubscribe', (service, char) => ble.gattUnsubscribe(service, char));
  handle('ble:reconnect', (id) => ble.reconnect(id));
}

app.whenReady().then(async () => {
  wireBluetooth();
  await createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', async () => {
  await ble?.destroy().catch(() => undefined);
  server?.close();
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
