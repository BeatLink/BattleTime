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

contextBridge.exposeInMainWorld('waspos', {
  startScan: () => ipcRenderer.invoke('ble:start-scan'),
  stopScan: () => ipcRenderer.invoke('ble:stop-scan'),
  connect: (id) => ipcRenderer.invoke('ble:connect', id),
  disconnect: () => ipcRenderer.invoke('ble:disconnect'),
  write: (text) => ipcRenderer.invoke('ble:write', text),
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
