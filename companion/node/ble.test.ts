import { describe, expect, it, jest } from '@jest/globals';
import { EventEmitter } from 'node:events';

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { watchForDisconnect } = require('./ble');

// A node-ble device: its own events, and the D-Bus helper that relays BlueZ's property changes.
function fakeDevice() {
  const device = new EventEmitter() as EventEmitter & { helper: EventEmitter };
  device.helper = new EventEmitter();
  return device;
}

describe('watchForDisconnect', () => {
  it('sees a link BlueZ already held drop, though node-ble never made the connection', () => {
    const device = fakeDevice();
    const dropped = jest.fn();
    watchForDisconnect(device, dropped);

    device.helper.emit('PropertiesChanged', { RSSI: { value: -70 } });
    device.helper.emit('PropertiesChanged', { Connected: { value: true } });
    expect(dropped).not.toHaveBeenCalled();

    device.helper.emit('PropertiesChanged', { Connected: { value: false } });
    expect(dropped).toHaveBeenCalledTimes(1);
  });

  it('reports a drop once, whichever way it is heard', () => {
    const device = fakeDevice();
    const dropped = jest.fn();
    watchForDisconnect(device, dropped);

    device.emit('disconnect', { connected: false });
    device.helper.emit('PropertiesChanged', { Connected: { value: false } });

    expect(dropped).toHaveBeenCalledTimes(1);
  });
});
