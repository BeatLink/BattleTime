// The NeoTime Companion desktop app.
//
// It runs the same interface as the phone app, served from the web export,
// with Bluetooth handled here in the main process.

const { app, BrowserWindow, dialog, ipcMain, shell } = require('electron');
const { basename, join } = require('node:path');

const { WatchBle } = require('../node/ble');
const { DesktopIntegration } = require('../node/desktop');
const { NoteFileWatches, readNoteFile } = require('../node/note-file');
const { runShellCommand } = require('../node/remote-command');
const { webRequest } = require('../node/web-request');
const { serve } = require('./server');

const DIST = join(__dirname, '..', 'dist');

let window_ = null;
let ble = null;
let server = null;
let desktop = null;
let noteWatches = null;

function sendToWindow(channel, ...args) {
  if (window_ && !window_.isDestroyed()) {
    window_.webContents.send(channel, ...args);
  }
}

function emit(name, payload) {
  sendToWindow('ble:event', name, payload);
}

// Runs a request from the window and hands back its result or its error as plain data.
function handle(channel, action) {
  ipcMain.handle(channel, async (_event, ...args) => {
    try {
      return { ok: true, value: await action(...args) };
    } catch (error) {
      return { ok: false, error: String(error.message || error) };
    }
  });
}

async function createWindow() {
  // In development the Expo dev server is used, so the interface reloads.
  const devUrl = process.env.NEOTIME_DEV_URL;
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
    title: 'NeoTime Companion',
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

  handle('ble:start-scan', () => ble.startScan());
  handle('ble:stop-scan', () => ble.stopScan());
  handle('ble:connect', (id) => ble.connect(id));
  handle('ble:disconnect', () => ble.disconnect());
  handle('ble:write', (text) => ble.write(text));
  handle('ble:write-bytes', (data) => ble.writeBytes(data));
  handle('ble:gatt-has', (service, char) => ble.gattHas(service, char));
  handle('ble:gatt-read', (service, char) => ble.gattRead(service, char));
  handle('ble:gatt-write', (service, char, data, mode) =>
    ble.gattWrite(service, char, data, mode));
  handle('ble:gatt-subscribe', (service, char) => ble.gattSubscribe(service, char));
  handle('ble:gatt-unsubscribe', (service, char) => ble.gattUnsubscribe(service, char));
  handle('ble:reconnect', (id) => ble.reconnect(id));
}

// Notifications, media players, Do Not Disturb and location from this computer, in place of a phone's.
function wireDesktop() {
  desktop = new DesktopIntegration((name, payload) => sendToWindow('desktop:event', payload));
  void desktop.start().catch(() => undefined);

  handle('desktop:notification-access', () => desktop.hasNotificationAccess());
  handle('desktop:do-not-disturb', () => desktop.isDoNotDisturb());
  handle('desktop:media-command', (player, command) => desktop.mediaCommand(player, command));
  handle('desktop:locate', () => desktop.locate());
  // The window asks for the players once it is listening, since it may have missed the first report.
  handle('desktop:players', () => desktop.emitPlayers());
}

// The text file shown on the watch's notes app: chosen in a file dialog, read from disk and watched for saves.
function wireNotes() {
  noteWatches = new NoteFileWatches((id) => sendToWindow('notes:changed', id));

  handle('notes:pick', async () => {
    const chosen = await dialog.showOpenDialog(window_, {
      title: 'Choose a note for the watch',
      properties: ['openFile'],
      filters: [
        { name: 'Text', extensions: ['txt', 'md', 'text'] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    const path = chosen.canceled ? null : chosen.filePaths[0];
    return path ? { name: basename(path), uri: path } : null;
  });
  handle('notes:read', (path) => readNoteFile(path));
  handle('notes:watch', (path) => noteWatches.start(path));
  handle('notes:unwatch', (id) => noteWatches.stop(id));
}

// Shell commands that remote buttons run; the window passes the command from the user's own button settings.
function wireRemote() {
  handle('remote:run-command', (command) => {
    if (typeof command !== 'string' || command.trim() === '') {
      throw new Error('Give a command to run.');
    }
    return runShellCommand(command);
  });
}

// Web requests from the user's own notes URL and remote buttons, which may trust a self-signed certificate.
function wireWeb() {
  handle('web:request', (request) => webRequest(request));
}

// The desktop's location service and notification server know the app by its desktop file.
app.setName('NeoTime Companion');
if (process.platform === 'linux') {
  app.setDesktopName('neotime-companion.desktop');
}

app.whenReady().then(async () => {
  wireBluetooth();
  wireDesktop();
  wireNotes();
  wireRemote();
  wireWeb();
  await createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', async () => {
  await ble?.destroy().catch(() => undefined);
  await desktop?.stop().catch(() => undefined);
  noteWatches?.stopAll();
  server?.close();
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
