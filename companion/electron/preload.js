// The only bridge between the window and the main process.
//
// The window gets a small, named API rather than any part of Node, which is
// why context isolation stays on.

const { contextBridge, ipcRenderer } = require('electron');

const listeners = {
  device: new Set(),
  line: new Set(),
  state: new Set(),
  error: new Set(),
  notify: new Set(),
  mode: new Set(),
};

ipcRenderer.on('ble:event', (_event, name, payload) => {
  for (const listener of listeners[name] ?? []) {
    listener(payload);
  }
});

const desktopListeners = new Set();

ipcRenderer.on('desktop:event', (_event, payload) => {
  for (const listener of desktopListeners) {
    listener(payload);
  }
});

// What this computer offers in place of a phone, read by src/services/phone/desktop.ts.
const desktop = {
  hasNotificationAccess: () => ipcRenderer.invoke('desktop:notification-access'),
  isDoNotDisturb: () => ipcRenderer.invoke('desktop:do-not-disturb'),
  mediaCommand: (player, command) => ipcRenderer.invoke('desktop:media-command', player, command),
  locate: () => ipcRenderer.invoke('desktop:locate'),
  onEvent: (listener) => {
    desktopListeners.add(listener);
    void ipcRenderer.invoke('desktop:players');
    return () => desktopListeners.delete(listener);
  },
};

const noteListeners = new Set();

ipcRenderer.on('notes:changed', (_event, id) => {
  for (const listener of noteListeners) {
    listener(id);
  }
});

// The note shown on the watch, read by src/services/note-file.ts.
const notes = {
  pick: () => ipcRenderer.invoke('notes:pick'),
  read: (path) => ipcRenderer.invoke('notes:read', path),
  watch: (path) => ipcRenderer.invoke('notes:watch', path),
  unwatch: (id) => ipcRenderer.invoke('notes:unwatch', id),
  onChange: (listener) => {
    noteListeners.add(listener);
    return () => noteListeners.delete(listener);
  },
};

// Shell commands for remote buttons, run by src/services/remote-platform.ts.
const remote = {
  runCommand: (command) => ipcRenderer.invoke('remote:run-command', command),
};

contextBridge.exposeInMainWorld('neotime', {
  desktop,
  notes,
  remote,
  startScan: () => ipcRenderer.invoke('ble:start-scan'),
  stopScan: () => ipcRenderer.invoke('ble:stop-scan'),
  connect: (id) => ipcRenderer.invoke('ble:connect', id),
  disconnect: () => ipcRenderer.invoke('ble:disconnect'),
  write: (text) => ipcRenderer.invoke('ble:write', text),
  writeBytes: (data) => ipcRenderer.invoke('ble:write-bytes', data),
  gattHas: (service, characteristic) =>
    ipcRenderer.invoke('ble:gatt-has', service, characteristic),
  gattRead: (service, characteristic) =>
    ipcRenderer.invoke('ble:gatt-read', service, characteristic),
  gattWrite: (service, characteristic, data, mode) =>
    ipcRenderer.invoke('ble:gatt-write', service, characteristic, data, mode),
  gattSubscribe: (service, characteristic) =>
    ipcRenderer.invoke('ble:gatt-subscribe', service, characteristic),
  gattUnsubscribe: (service, characteristic) =>
    ipcRenderer.invoke('ble:gatt-unsubscribe', service, characteristic),
  reconnect: (id) => ipcRenderer.invoke('ble:reconnect', id),
  on: (name, listener) => {
    listeners[name]?.add(listener);
    return () => listeners[name]?.delete(listener);
  },
});
