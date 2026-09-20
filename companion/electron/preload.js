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
};

ipcRenderer.on('ble:event', (_event, name, payload) => {
  for (const listener of listeners[name] ?? []) {
    listener(payload);
  }
});

contextBridge.exposeInMainWorld('neotime', {
  startScan: () => ipcRenderer.invoke('ble:start-scan'),
  stopScan: () => ipcRenderer.invoke('ble:stop-scan'),
  connect: (id) => ipcRenderer.invoke('ble:connect', id),
  disconnect: () => ipcRenderer.invoke('ble:disconnect'),
  write: (text) => ipcRenderer.invoke('ble:write', text),
  on: (name, listener) => {
    listeners[name]?.add(listener);
    return () => listeners[name]?.delete(listener);
  },
});
