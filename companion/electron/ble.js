// Bluetooth for the desktop app, spoken to BlueZ over D-Bus by node-ble.
//
// This runs in the Electron main process. The window never touches it
// directly; it goes through the preload bridge.

const { createBluetooth } = require('node-ble');

const NUS_SERVICE = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';
const NUS_RX = '6e400002-b5a3-f393-e0a9-e50e24dcca9e';
const NUS_TX = '6e400003-b5a3-f393-e0a9-e50e24dcca9e';

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

function withTimeout(promise, what, ms = STEP_TIMEOUT_MS) {
  return Promise.race([
    promise,
    new Promise((_resolve, reject) =>
      setTimeout(() => reject(new Error(`Timed out ${what}.`)), ms)),
  ]);
}

class DesktopBle {
  constructor(emit) {
    this.emit = emit;
    this.session = null;
    this.adapter = null;
    this.device = null;
    this.rx = null;
    this.tx = null;
    this.gatt = null;
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
          this.emit('device', { id: address, name, rssi: rssi === null ? null : Number(rssi) });
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
      this.forget();
      this.emit('state', 'disconnected');
      throw error;
    }
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

    this.gatt = await withTimeout(device.gatt(), 'discovering services');

    // A bootloader has no UART service, so a firmware update connects
    // without one and reaches the DFU characteristics directly.
    if (uart) {
      const service = await withTimeout(
        this.gatt.getPrimaryService(NUS_SERVICE), 'looking for the UART service');
      this.rx = await service.getCharacteristic(NUS_RX);
      this.tx = await service.getCharacteristic(NUS_TX);

      await withTimeout(this.tx.startNotifications(), 'subscribing to the watch');
      this.tx.on('valuechanged', (buffer) => {
        this.emit('line', buffer.toString('utf8'));
      });
    }

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
    const device = this.device;
    this.forget();
    if (device) {
      await device.disconnect().catch(() => undefined);
    }
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

module.exports = { DesktopBle };
