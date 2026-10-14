// The only bridge between the window and the desktop app, as `window.neotime`.
//
// It offers the API src/ble/electron-transport.ts, src/services/phone/desktop.ts, note-file.ts and
// remote-platform.ts read, and turns each call into a Tauri command answered by src/main.rs.

(() => {
  // Tauri's own scripts may run after this one, so its globals are looked up at call time.
  const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);

  // Every call gives { ok, value } or { ok: false, error }, as the Electron preload did.
  const call = (command, args) =>
    invoke(command, args).then(
      (value) => ({ ok: true, value }),
      (error) => ({ ok: false, error: String(error && error.message ? error.message : error) }),
    );

  // One Tauri listener per event name, started the first time anything listens.
  const listenerSets = new Map();
  const listenOnce = (eventName, onPayload) => {
    let listeners = listenerSets.get(eventName);
    if (!listeners) {
      listeners = new Set();
      listenerSets.set(eventName, listeners);
      window.__TAURI__.event.listen(eventName, (event) => {
        for (const listener of listeners) {
          onPayload(listener, event.payload);
        }
      });
    }
    return listeners;
  };
  const subscribe = (eventName, listener, onPayload) => {
    const listeners = listenOnce(eventName, onPayload);
    listeners.add(listener);
    return () => listeners.delete(listener);
  };

  // Bluetooth events arrive as { name, payload } and go to the listeners for that name.
  const bleListeners = { device: new Set(), line: new Set(), state: new Set(), error: new Set(), notify: new Set(), mode: new Set() };
  let bleFollowed = false;
  const followBle = () => {
    if (bleFollowed) return;
    bleFollowed = true;
    window.__TAURI__.event.listen('ble:event', (event) => {
      for (const listener of bleListeners[event.payload.name] ?? []) {
        listener(event.payload.payload);
      }
    });
  };

  const desktop = {
    hasNotificationAccess: () => call('desktop_notification_access'),
    isDoNotDisturb: () => call('desktop_do_not_disturb'),
    mediaCommand: (player, command) => call('desktop_media_command', { player, command }),
    locate: () => call('desktop_locate'),
    onEvent: (listener) => {
      const stop = subscribe('desktop:event', listener, (each, payload) => each(payload));
      void call('desktop_players');
      return stop;
    },
  };

  const notes = {
    pick: () => call('notes_pick'),
    read: (path) => call('notes_read', { path }),
    watch: (path) => call('notes_watch', { path }),
    unwatch: (id) => call('notes_unwatch', { id }),
    onChange: (listener) => subscribe('notes:changed', listener, (each, payload) => each(payload)),
  };

  const remote = {
    runCommand: (command) => call('remote_run_command', { command }),
  };

  window.neotime = {
    desktop,
    notes,
    remote,
    startScan: () => call('ble_start_scan'),
    stopScan: () => call('ble_stop_scan'),
    connect: (id) => call('ble_connect', { id }),
    disconnect: () => call('ble_disconnect'),
    write: (text) => call('ble_write', { text }),
    writeBytes: (data) => call('ble_write_bytes', { data }),
    gattHas: (service, characteristic) => call('ble_gatt_has', { service, characteristic }),
    gattRead: (service, characteristic) => call('ble_gatt_read', { service, characteristic }),
    gattWrite: (service, characteristic, data, mode) => call('ble_gatt_write', { service, characteristic, data, mode }),
    gattSubscribe: (service, characteristic) => call('ble_gatt_subscribe', { service, characteristic }),
    gattUnsubscribe: (service, characteristic) => call('ble_gatt_unsubscribe', { service, characteristic }),
    reconnect: (id) => call('ble_reconnect', { id }),
    on: (name, listener) => {
      followBle();
      bleListeners[name]?.add(listener);
      return () => bleListeners[name]?.delete(listener);
    },
  };
})();
