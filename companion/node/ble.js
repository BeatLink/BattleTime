// Bluetooth on a computer, spoken to BlueZ over D-Bus by node-ble.
//
// Shared by the desktop app, where it runs in the Electron main process
// behind the preload bridge, and by the command line tool.

const { createBluetooth } = require('node-ble');

const NUS_SERVICE = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';
const NUS_RX = '6e400002-b5a3-f393-e0a9-e50e24dcca9e';
const NUS_TX = '6e400003-b5a3-f393-e0a9-e50e24dcca9e';

// A watch sitting in its bootloader advertises one of these instead.
const DFU_SERVICES = [
  '00001530-1212-efde-1523-785feabcd123',
  '0000fe59-0000-1000-8000-00805f9b34fb',
];

// BlueZ splits a write itself only up to the negotiated MTU, so stay at the
// size every connection supports.
const CHUNK = 20;

// How often to look for newly discovered devices while scanning.
const SCAN_POLL_MS = 1000;

// How long to wait for a watch that BlueZ has not seen yet.
const CONNECT_TIMEOUT_MS = 15000;

// How long to wait for the link and its services. BlueZ can report a stale
// connection whose services never resolve, so every step gives up rather than
// leaving the interface stuck on "connecting".
const STEP_TIMEOUT_MS = 20000;

// How long to wait for BlueZ to resolve a device's services before dropping
// the link and trying once more from scratch.
const SERVICES_TIMEOUT_MS = 12000;

// How often to ask whether the services have resolved. BlueZ announces it
// with a property change, but node-ble reads the property and only then
// subscribes, so a change in between is missed and the wait never ends.
const SERVICES_POLL_MS = 250;

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function withTimeout(promise, what, ms = STEP_TIMEOUT_MS) {
  return Promise.race([
    promise,
    new Promise((_resolve, reject) =>
      setTimeout(() => reject(new Error(`Timed out ${what}.`)), ms)),
  ]);
}

class WatchBle {
  constructor(emit) {
    this.emit = emit;
    this.session = null;
    this.adapter = null;
    this.device = null;
    this.rx = null;
    this.tx = null;
    this.gatt = null;
    this.pending = null;
    this.scanTimer = null;
    this.seen = new Set();
    this.characteristics = new Map();
    this.subscriptions = new Map();
  }

  async adapterOrThrow() {
    if (!this.session) {
      this.session = createBluetooth();
    }
    if (!this.adapter) {
      this.adapter = await this.session.bluetooth.defaultAdapter();
    }
    if (!(await this.adapter.isPowered())) {
      throw new Error('The Bluetooth adapter is powered off.');
    }
    return this.adapter;
  }

  async startScan() {
    const adapter = await this.adapterOrThrow();
    this.seen.clear();

    if (!(await adapter.isDiscovering())) {
      await adapter.startDiscovery();
    }

    // node-ble reports no discovery events, so poll the device list.
    const poll = async () => {
      try {
        for (const address of await adapter.devices()) {
          if (this.seen.has(address)) {
            continue;
          }
          this.seen.add(address);
          const device = await adapter.getDevice(address);
          const name = await device.getName().catch(() => null);
          if (!name) {
            continue;
          }
          const rssi = await device.getRSSI().catch(() => null);
          // BlueZ lists the services a device advertises, which is how a
          // watch is told apart from everything else in range. It is empty
          // for a device BlueZ has not looked at yet.
          const uuids = (await device.helper.prop('UUIDs').catch(() => []) || [])
            .map((uuid) => String(uuid).toLowerCase());
          this.emit('device', {
            id: address,
            name,
            rssi: rssi === null ? null : Number(rssi),
            uuids,
            bootloader:
              !uuids.includes(NUS_SERVICE) && uuids.some((uuid) => DFU_SERVICES.includes(uuid)),
          });
        }
      } catch (error) {
        this.emit('error', String(error.message || error));
      }
    };

    await poll();
    this.scanTimer = setInterval(poll, SCAN_POLL_MS);
  }

  async stopScan() {
    if (this.scanTimer) {
      clearInterval(this.scanTimer);
      this.scanTimer = null;
    }
    if (this.adapter && (await this.adapter.isDiscovering().catch(() => false))) {
      await this.adapter.stopDiscovery().catch(() => undefined);
    }
  }

  async connect(id, uart = true) {
    try {
      await this.connectInner(id, uart);
    } catch (error) {
      // Drop the link as well as our own references. A half-open connection
      // left behind makes every later attempt skip connect() and then wait
      // for services that will never resolve.
      await this.dropLink();
      this.emit('state', 'disconnected');
      throw error;
    }
  }

  // Let go of whatever BlueZ is still holding, without hanging on a link that
  // is already wedged.
  async dropLink() {
    const device = this.device || this.pending;
    this.forget();
    if (device) {
      await withTimeout(device.disconnect(), 'disconnecting', 5000).catch(() => undefined);
    }
  }

  // Wait for BlueZ to resolve the services, by asking rather than by waiting
  // for the one announcement, then hand back the GATT server.
  async servicesFor(device) {
    // Asking for the property needs node-ble's D-Bus helper. Without it, fall
    // back to node-ble's own wait rather than never resolving at all.
    if (!device.helper || typeof device.helper.prop !== 'function') {
      return withTimeout(device.gatt(), 'discovering services', SERVICES_TIMEOUT_MS).catch(
        () => null,
      );
    }

    const deadline = Date.now() + SERVICES_TIMEOUT_MS;
    while (Date.now() < deadline) {
      const resolved = await device.helper.prop('ServicesResolved').catch(() => false);
      if (resolved) {
        // The property is true, so node-ble returns without waiting.
        return withTimeout(device.gatt(), 'reading the services', STEP_TIMEOUT_MS);
      }
      await sleep(SERVICES_POLL_MS);
    }
    return null;
  }

  // Some links come up without ever resolving their services, which is common
  // for a watch sitting in its bootloader. Dropping that link and connecting
  // again clears it, so try that once before giving up.
  async resolveServices(device) {
    const first = await this.servicesFor(device);
    if (first) {
      return first;
    }

    await withTimeout(device.disconnect(), 'disconnecting', 5000).catch(() => undefined);
    await sleep(1500);
    await withTimeout(device.connect(), 'connecting to the watch');

    const second = await this.servicesFor(device);
    if (second) {
      return second;
    }
    throw new Error(
      'Connected, but the watch never offered its services. Move it closer, or restart it.',
    );
  }

  async connectInner(id, uart) {
    const adapter = await this.adapterOrThrow();
    this.emit('state', 'connecting');

    // A device seen during the scan is already known to BlueZ. Only wait for
    // one that is not, and give up rather than hanging, because waitDevice
    // needs discovery running to ever succeed.
    let device;
    try {
      device = await adapter.getDevice(id);
    } catch {
      device = await adapter.waitDevice(id, CONNECT_TIMEOUT_MS);
    }

    // Discovery competes with the connection, so it stops once we have the
    // device rather than before we look for it.
    await this.stopScan();

    // BlueZ may already hold the link, from a previous run or another tool.
    // Calling connect then waits for a state change that never comes.
    if (!(await device.isConnected().catch(() => false))) {
      await withTimeout(device.connect(), 'connecting to the watch');
    }

    this.pending = device;
    this.gatt = await this.resolveServices(device);

    // A bootloader has no UART service. When one is asked for and is not
    // there, the watch is in its bootloader: keep the link, because that is
    // what a firmware update needs, and say so.
    let mode = 'bootloader';
    if (uart) {
      const service = await this.gatt.getPrimaryService(NUS_SERVICE).catch(() => null);
      if (service) {
        this.rx = await service.getCharacteristic(NUS_RX);
        this.tx = await service.getCharacteristic(NUS_TX);

        await withTimeout(this.tx.startNotifications(), 'subscribing to the watch');
        this.tx.on('valuechanged', (buffer) => {
          this.emit('line', buffer.toString('utf8'));
        });
        mode = 'application';
      }
    }
    this.emit('mode', mode);

    device.on('disconnect', () => {
      this.forget();
      this.emit('state', 'disconnected');
    });

    this.device = device;
    this.emit('state', 'connected');
  }

  // --- Raw GATT, which a firmware update needs and the UART line does not.

  async characteristic(service, characteristic) {
    if (!this.gatt) {
      throw new Error('Not connected');
    }
    const key = `${service}/${characteristic}`;
    let found = this.characteristics.get(key);
    if (!found) {
      const primary = await this.gatt.getPrimaryService(service);
      found = await primary.getCharacteristic(characteristic);
      this.characteristics.set(key, found);
    }
    return found;
  }

  async gattHas(service, characteristic) {
    try {
      await this.characteristic(service, characteristic);
      return true;
    } catch {
      return false;
    }
  }

  async gattRead(service, characteristic) {
    const found = await this.characteristic(service, characteristic);
    return Array.from(await found.readValue());
  }

  async gattWrite(service, characteristic, data, mode) {
    const found = await this.characteristic(service, characteristic);
    const buffer = Buffer.from(data);
    if (mode === 'request') {
      await found.writeValueWithResponse(buffer);
      return;
    }
    await found.writeValueWithoutResponse(buffer);
  }

  async gattSubscribe(service, characteristic) {
    const key = `${service}/${characteristic}`;
    if (this.subscriptions.has(key)) {
      return;
    }
    const found = await this.characteristic(service, characteristic);
    const onValue = (buffer) => this.emit('notify', { key, value: Array.from(buffer) });
    found.on('valuechanged', onValue);
    await found.startNotifications();
    this.subscriptions.set(key, { found, onValue });
  }

  async gattUnsubscribe(service, characteristic) {
    const key = `${service}/${characteristic}`;
    const entry = this.subscriptions.get(key);
    if (!entry) {
      return;
    }
    this.subscriptions.delete(key);
    entry.found.off('valuechanged', entry.onValue);
    await entry.found.stopNotifications().catch(() => undefined);
  }

  // Drop the link and come back to the given address without the UART
  // service, which is how a firmware update follows the watch into its
  // bootloader.
  async reconnect(id) {
    await this.disconnect().catch(() => undefined);
    await new Promise((resolve) => setTimeout(resolve, 1000));
    await this.connect(id, false);
  }

  async disconnect() {
    // A wedged link makes BlueZ's disconnect hang, so give up on it rather
    // than leaving the interface stuck.
    await this.dropLink();
    this.emit('state', 'disconnected');
  }

  async write(text) {
    if (!this.rx) {
      throw new Error('Not connected');
    }
    const bytes = Buffer.from(text, 'utf8');
    for (let offset = 0; offset < bytes.length; offset += CHUNK) {
      await this.rx.writeValueWithoutResponse(bytes.subarray(offset, offset + CHUNK));
    }
  }

  forget() {
    this.device = null;
    this.pending = null;
    this.gatt = null;
    this.rx = null;
    this.tx = null;
    this.characteristics.clear();
    this.subscriptions.clear();
  }

  async destroy() {
    await this.stopScan().catch(() => undefined);
    await this.disconnect().catch(() => undefined);
    if (this.session) {
      this.session.destroy();
      this.session = null;
      this.adapter = null;
    }
  }
}

module.exports = { WatchBle };
