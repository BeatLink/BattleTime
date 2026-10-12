// The computer's side of what a phone offers the watch: desktop notifications, media players, Do Not Disturb and location.
//
// Runs in the Electron main process and speaks D-Bus on Linux. It only gathers raw facts and carries out commands;
// src/services/phone/desktop.ts decides what reaches the watch.

const { execFile } = require('node:child_process');
const dbus = require('dbus-next');

// ============================================================================
// Names on the bus
// ============================================================================

const DBUS_NAME = 'org.freedesktop.DBus';
const DBUS_PATH = '/org/freedesktop/DBus';
const PROPERTIES = 'org.freedesktop.DBus.Properties';

const NOTIFICATIONS_NAME = 'org.freedesktop.Notifications';
const NOTIFICATIONS_PATH = '/org/freedesktop/Notifications';

const MPRIS_PREFIX = 'org.mpris.MediaPlayer2.';
const MPRIS_PATH = '/org/mpris/MediaPlayer2';
const MPRIS_PLAYER = 'org.mpris.MediaPlayer2.Player';

const GEOCLUE_NAME = 'org.freedesktop.GeoClue2';
const GEOCLUE_MANAGER_PATH = '/org/freedesktop/GeoClue2/Manager';

// GeoClue lets an app in only under the name of its desktop file.
const DESKTOP_ID = 'neotime-companion';

// ============================================================================
// Timing
// ============================================================================

// How long to wait for GeoClue to find where the computer is.
const LOCATE_TIMEOUT_MS = 20000;

// How long a Do Not Disturb check may take before it counts as off.
const COMMAND_TIMEOUT_MS = 3000;

// ============================================================================
// Helpers
// ============================================================================

function withTimeout(promise, what, ms) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_resolve, reject) => {
      timer = setTimeout(() => reject(new Error(`Timed out ${what}.`)), ms);
    }),
  ]).finally(() => clearTimeout(timer));
}

// Turns a D-Bus value into plain JSON that can cross to the window.
function plain(value) {
  if (value instanceof dbus.Variant) {
    return plain(value.value);
  }
  if (typeof value === 'bigint') {
    return Number(value);
  }
  if (Array.isArray(value)) {
    return value.map(plain);
  }
  if (value && typeof value === 'object' && !Buffer.isBuffer(value)) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, plain(item)]));
  }
  return value;
}

function runCommand(command, args) {
  return new Promise((resolve) => {
    execFile(command, args, { timeout: COMMAND_TIMEOUT_MS }, (error, stdout) =>
      resolve(error ? null : String(stdout).trim()));
  });
}

// ============================================================================
// Desktop integration
// ============================================================================

class DesktopIntegration {
  constructor(emit) {
    this.emit = emit;
    this.bus = null;
    this.monitor = null;
    this.notificationsWatched = false;
    this.players = new Map();
  }

  async start() {
    this.bus = dbus.sessionBus();
    this.bus.on('error', () => undefined);
    await Promise.allSettled([this.watchNotifications(), this.watchPlayers()]);
  }

  async stop() {
    this.monitor?.disconnect();
    this.bus?.disconnect();
    this.monitor = null;
    this.bus = null;
    this.players.clear();
  }

  async dbusCall(destination, path, iface, member, signature = '', body = []) {
    const reply = await this.bus.call(new dbus.Message({ destination, path, interface: iface, member, signature, body }));
    return reply.body;
  }

  // ==========================================================================
  // Notifications
  // ==========================================================================

  // Listens in on what apps send the notification server, which is the only way to read them on Linux.
  async watchNotifications() {
    const [server] = await this.dbusCall(DBUS_NAME, DBUS_PATH, DBUS_NAME, 'GetNameOwner', 's', [NOTIFICATIONS_NAME]);

    this.monitor = dbus.sessionBus();
    this.monitor.on('error', () => undefined);
    // A monitor must never answer, or the bus drops it, so every call it overhears counts as handled.
    this.monitor.addMethodHandler(() => true);
    this.monitor.on('message', (message) => this.onMonitoredMessage(message, server));

    const rules = [
      `type='method_call',interface='${NOTIFICATIONS_NAME}',member='Notify'`,
      `type='method_call',interface='${NOTIFICATIONS_NAME}',member='CloseNotification'`,
      `type='method_return',sender='${server}'`,
      `type='signal',interface='${NOTIFICATIONS_NAME}',member='NotificationClosed'`,
    ];
    await this.monitor.call(new dbus.Message({
      destination: DBUS_NAME,
      path: DBUS_PATH,
      interface: 'org.freedesktop.DBus.Monitoring',
      member: 'BecomeMonitor',
      signature: 'asu',
      body: [rules, 0],
    }));
    this.notificationsWatched = true;
  }

  onMonitoredMessage(message, server) {
    const { MessageType } = dbus;
    if (message.type === MessageType.METHOD_CALL && message.member === 'Notify') {
      const [appName, replacesId, , summary, body, , hints] = message.body;
      this.emit('desktop', {
        kind: 'notify',
        sender: message.sender,
        serial: message.serial,
        appName,
        replacesId,
        summary,
        body,
        hints: plain(hints),
      });
    } else if (message.type === MessageType.METHOD_RETURN && message.sender === server && message.signature === 'u') {
      this.emit('desktop', {
        kind: 'notify-reply',
        destination: message.destination,
        replySerial: message.replySerial,
        id: message.body[0],
      });
    } else if (message.type === MessageType.SIGNAL && message.member === 'NotificationClosed') {
      this.emit('desktop', { kind: 'closed', id: message.body[0] });
    } else if (message.type === MessageType.METHOD_CALL && message.member === 'CloseNotification') {
      this.emit('desktop', { kind: 'closed', id: message.body[0] });
    }
  }

  // True once notifications are being read.
  hasNotificationAccess() {
    return this.notificationsWatched;
  }

  // ==========================================================================
  // Media players
  // ==========================================================================

  // Follows every MPRIS player, and tells the window whenever one changes.
  async watchPlayers() {
    const [names] = await this.dbusCall(DBUS_NAME, DBUS_PATH, DBUS_NAME, 'ListNames');
    await Promise.allSettled(names.filter((name) => name.startsWith(MPRIS_PREFIX)).map((name) => this.addPlayer(name)));

    const dbusObject = await this.bus.getProxyObject(DBUS_NAME, DBUS_PATH);
    dbusObject.getInterface(DBUS_NAME).on('NameOwnerChanged', (name, _oldOwner, newOwner) => {
      if (!name.startsWith(MPRIS_PREFIX)) {
        return;
      }
      if (newOwner) {
        void this.addPlayer(name).catch(() => undefined);
      } else {
        this.players.delete(name);
        this.emitPlayers();
      }
    });
    this.emitPlayers();
  }

  async addPlayer(name) {
    const object = await this.bus.getProxyObject(name, MPRIS_PATH);
    const properties = object.getInterface(PROPERTIES);
    const player = { name, properties, state: {} };
    this.players.set(name, player);

    properties.on('PropertiesChanged', (iface, changed) => {
      if (iface !== MPRIS_PLAYER) {
        return;
      }
      Object.assign(player.state, plain(changed));
      void this.refreshPosition(player).finally(() => this.emitPlayers());
    });

    player.state = plain(await properties.GetAll(MPRIS_PLAYER));
    this.emitPlayers();
  }

  // Players do not announce the position as it moves, so it is read whenever anything else changes.
  async refreshPosition(player) {
    try {
      player.state.Position = plain(await player.properties.Get(MPRIS_PLAYER, 'Position'));
    } catch {
      // Some players do not report a position.
    }
  }

  emitPlayers() {
    this.emit('desktop', { kind: 'players', players: this.playerList() });
  }

  playerList() {
    return [...this.players.values()].map(({ name, state }) => ({
      name,
      status: state.PlaybackStatus ?? 'Stopped',
      metadata: state.Metadata ?? {},
      position: state.Position ?? null,
      volume: state.Volume ?? null,
    }));
  }

  async mediaCommand(playerName, command) {
    const player = this.players.get(playerName);
    if (!player) {
      throw new Error('No media player is running.');
    }
    if (command === 'volumeup' || command === 'volumedown') {
      const current = Number(player.state.Volume ?? 0.5);
      const next = Math.min(1, Math.max(0, current + (command === 'volumeup' ? 0.1 : -0.1)));
      await player.properties.Set(MPRIS_PLAYER, 'Volume', new dbus.Variant('d', next));
      return;
    }
    const members = { play: 'Play', pause: 'Pause', next: 'Next', previous: 'Previous' };
    const member = members[command];
    if (!member) {
      throw new Error(`Unknown media command ${command}.`);
    }
    await this.dbusCall(playerName, MPRIS_PATH, MPRIS_PLAYER, member);
  }

  // ==========================================================================
  // Do Not Disturb
  // ==========================================================================

  // Each desktop keeps Do Not Disturb its own way, so ask every one that might be running.
  async isDoNotDisturb() {
    const inhibited = await this.notificationServerInhibited();
    if (inhibited !== null) {
      return inhibited;
    }
    const checks = [
      ['gsettings', ['get', 'org.cinnamon.desktop.notifications', 'display-notifications'], 'false'],
      ['gsettings', ['get', 'org.gnome.desktop.notifications', 'show-banners'], 'false'],
      ['dunstctl', ['is-paused'], 'true'],
    ];
    for (const [command, args, quietAnswer] of checks) {
      const answer = await runCommand(command, args);
      if (answer === quietAnswer) {
        return true;
      }
    }
    return false;
  }

  // KDE and some others say whether notifications are held back on the notification server itself.
  async notificationServerInhibited() {
    try {
      const [value] = await this.dbusCall(NOTIFICATIONS_NAME, NOTIFICATIONS_PATH, PROPERTIES, 'Get', 'ss', [
        NOTIFICATIONS_NAME,
        'Inhibited',
      ]);
      return plain(value) === true;
    } catch {
      return null;
    }
  }

  // ==========================================================================
  // Location
  // ==========================================================================

  // Asks GeoClue, the desktop's location service, where the computer is.
  async locate() {
    const bus = dbus.systemBus();
    try {
      const manager = (await bus.getProxyObject(GEOCLUE_NAME, GEOCLUE_MANAGER_PATH)).getInterface(
        'org.freedesktop.GeoClue2.Manager',
      );
      const clientPath = await manager.GetClient();
      const clientObject = await bus.getProxyObject(GEOCLUE_NAME, clientPath);
      const client = clientObject.getInterface('org.freedesktop.GeoClue2.Client');
      const clientProperties = clientObject.getInterface(PROPERTIES);
      await clientProperties.Set('org.freedesktop.GeoClue2.Client', 'DesktopId', new dbus.Variant('s', DESKTOP_ID));
      // City level is enough for the weather.
      await clientProperties.Set('org.freedesktop.GeoClue2.Client', 'RequestedAccuracyLevel', new dbus.Variant('u', 4));

      const found = new Promise((resolve) => client.once('LocationUpdated', (_old, newPath) => resolve(newPath)));
      await client.Start();
      const locationPath = await withTimeout(found, 'waiting for the location service', LOCATE_TIMEOUT_MS);
      const location = (await bus.getProxyObject(GEOCLUE_NAME, locationPath)).getInterface(PROPERTIES);
      const read = async (name) => plain(await location.Get('org.freedesktop.GeoClue2.Location', name));
      const result = {
        latitude: await read('Latitude'),
        longitude: await read('Longitude'),
        description: await read('Description'),
      };
      await client.Stop().catch(() => undefined);
      return result;
    } catch (error) {
      const message = String(error.message || error);
      if (/not activatable|ServiceUnknown|was not provided/.test(message)) {
        throw new Error('This computer has no location service (GeoClue) running; choose a place by name instead.');
      }
      throw new Error(`The location service did not answer: ${message}`);
    } finally {
      bus.disconnect();
    }
  }
}

module.exports = { DesktopIntegration };
