// The only bridge between the window and the app, as `window.neotime`.
//
// It offers the API src/ble/electron-transport.ts, src/services/phone/desktop.ts, note-file.ts,
// remote-platform.ts, web-request.ts and plugins.ts read, and turns each call into a Tauri command answered by src/lib.rs.
// On Android it offers the phone (src/services/phone/tauri-android.ts) in place of the desktop and remote commands.

(() => {
  // src/lib.rs sets this before the script runs.
  const onAndroid = window.__NEOTIME_ANDROID__ === true;

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

  // The phone bridge plugin's Kotlin, which answers its commands directly.
  const phoneCall = (command, args) => call(`plugin:phone-bridge|${command}`, args);
  const phoneListeners = new Set();
  let phoneFollowed = false;
  const phone = {
    hasNotificationAccess: () => phoneCall('has_notification_access'),
    openNotificationAccessSettings: () => phoneCall('open_notification_access_settings'),
    requestCallPermissions: () => phoneCall('request_call_permissions'),
    isDoNotDisturb: () => phoneCall('is_do_not_disturb'),
    answerCall: () => phoneCall('answer_call'),
    rejectCall: () => phoneCall('reject_call'),
    silenceCall: () => phoneCall('silence_call'),
    mediaCommand: (command) => phoneCall('media_command', { command }),
    startKeepAlive: (watchName) => phoneCall('start_keep_alive', { watchName }),
    stopKeepAlive: () => phoneCall('stop_keep_alive'),
    locate: () => phoneCall('locate'),
    openUri: (uri) => phoneCall('open_uri', { uri }),
    startActivity: (packageName, className, action, extras) =>
      phoneCall('start_activity', { packageName, className, action, extras }),
    sendBroadcast: (action, packageName, extras) => phoneCall('send_broadcast', { action, packageName, extras }),
    onEvent: (listener) => {
      phoneListeners.add(listener);
      if (!phoneFollowed) {
        phoneFollowed = true;
        void window.__TAURI__.core.addPluginListener('phone-bridge', 'phone', (payload) => {
          for (const each of phoneListeners) {
            each(payload);
          }
        });
      }
      return () => phoneListeners.delete(listener);
    },
  };

  // Web requests for the notes URL and remote buttons, made by src/services/web-request.ts.
  const web = {
    request: (request) => call('web_request', { request }),
  };

  // Plugins installed in the app, read by src/services/plugins.ts.
  const plugins = {
    list: () => call('plugins_list'),
    installPick: () => call('plugins_install_pick'),
    install: (data) => call('plugins_install', { data }),
    remove: (id) => call('plugins_remove', { id }),
    setEnabled: (id, enabled) => call('plugins_set_enabled', { id, enabled }),
    watchApps: (id) => call('plugins_watch_apps', { id }),
    installOnWatch: (id) => call('plugins_install_on_watch', { id }),
    uiMessage: (id, message) => call('plugins_ui_message', { id, message }),
    onUi: (listener) => subscribe('plugins:ui', listener, (each, payload) => each(payload)),
    onLog: (listener) => subscribe('plugins:log', listener, (each, payload) => each(payload)),
    // Android's webview reaches the app's plugin:// scheme at this address.
    pageBase: onAndroid ? 'http://plugin.localhost/' : 'plugin://',
  };

  // The core's copy of the watch's settings and records, read by src/services/sync.ts.
  const sync = {
    settings: () => call('sync_settings'),
    setSettings: (settings) => call('sync_set_settings', { settings }),
    now: () => call('sync_now'),
    read: (source) => call('sync_read', { source }),
    list: (prefix) => call('sync_list', { prefix }),
    writeAppState: (id, changes) => call('sync_write_app_state', { id, changes }),
    writeSettings: (changes) => call('sync_write_settings', { changes }),
    onReport: (listener) => subscribe('sync:report', listener, (each, payload) => each(payload)),
  };

  window.neotime = {
    ...(onAndroid ? { phone } : { desktop, remote }),
    notes,
    web,
    plugins,
    sync,
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
    // The file service is shared with the core's sync, so the window takes a turn for each operation.
    filesBegin: () => call('files_begin'),
    filesEnd: (lease) => call('files_end', { lease }),
    on: (name, listener) => {
      followBle();
      bleListeners[name]?.add(listener);
      return () => bleListeners[name]?.delete(listener);
    },
  };
})();
