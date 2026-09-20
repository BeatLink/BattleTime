import type { BleManager, Device, Subscription } from 'react-native-ble-plx';

import { base64ToBytes, bytesToBase64, utf8Decode, utf8Encode } from './encoding';
import type { ConnectionState, DiscoveredWatch, TransportListener, WatchTransport } from './transport';
import { ATT_OVERHEAD, DEFAULT_MTU, NUS_RX, NUS_SERVICE, NUS_TX } from './uuids';

// Talks to a real watch over the Nordic UART Service using react-native-ble-plx.
export class BleTransport implements WatchTransport {
  readonly kind = 'ble' as const;

  private listener: TransportListener = {};
  private device: Device | null = null;
  private txSubscription: Subscription | null = null;
  private disconnectSubscription: Subscription | null = null;
  private mtu = DEFAULT_MTU;

  constructor(private readonly manager: BleManager) {}

  async startScan(onFound: (watch: DiscoveredWatch) => void) {
    await this.manager.startDeviceScan([NUS_SERVICE], { allowDuplicates: false }, (error, device) => {
      if (error) {
        this.listener.onError?.(error);
        return;
      }
      if (device) {
        onFound({ id: device.id, name: device.name ?? device.localName ?? 'Unknown watch', rssi: device.rssi });
      }
    });
  }

  async stopScan() {
    await this.manager.stopDeviceScan();
  }

  async connect(id: string) {
    this.setState('connecting');
    try {
      const device = await this.manager.connectToDevice(id, { requestMTU: 185 });
      await device.discoverAllServicesAndCharacteristics();
      this.device = device;
      this.mtu = device.mtu > 0 ? device.mtu : DEFAULT_MTU;

      this.disconnectSubscription = this.manager.onDeviceDisconnected(id, () => {
        this.cleanup();
        this.setState('disconnected');
      });

      this.txSubscription = this.manager.monitorCharacteristicForDevice(
        id,
        NUS_SERVICE,
        NUS_TX,
        (error, characteristic) => {
          if (error) {
            this.listener.onError?.(error);
            return;
          }
          if (characteristic?.value) {
            this.listener.onLine?.(utf8Decode(base64ToBytes(characteristic.value)));
          }
        },
      );

      this.setState('connected');
    } catch (error) {
      this.cleanup();
      this.setState('disconnected');
      throw error;
    }
  }

  async disconnect() {
    const id = this.device?.id;
    this.cleanup();
    if (id) {
      await this.manager.cancelDeviceConnection(id).catch(() => undefined);
    }
    this.setState('disconnected');
  }

  async write(text: string) {
    const device = this.device;
    if (!device) {
      throw new Error('Not connected');
    }
    const bytes = utf8Encode(text);
    const chunkSize = Math.max(1, this.mtu - ATT_OVERHEAD);
    for (let offset = 0; offset < bytes.length; offset += chunkSize) {
      const chunk = bytes.subarray(offset, offset + chunkSize);
      await this.manager.writeCharacteristicWithoutResponseForDevice(
        device.id,
        NUS_SERVICE,
        NUS_RX,
        bytesToBase64(chunk),
      );
    }
  }

  setListener(listener: TransportListener) {
    this.listener = listener;
  }

  destroy() {
    this.cleanup();
    void this.manager.destroy();
  }

  private cleanup() {
    this.txSubscription?.remove();
    this.txSubscription = null;
    this.disconnectSubscription?.remove();
    this.disconnectSubscription = null;
    this.device = null;
    this.mtu = DEFAULT_MTU;
  }

  private setState(state: ConnectionState) {
    this.listener.onState?.(state);
  }
}
