import type { BleManager, Device, Subscription } from 'react-native-ble-plx';

import type { DfuLink, WriteMode } from '@/dfu/link';

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
    return this.connectTo(id, true);
  }

  // A bootloader has no UART service, so the subscription to it is skipped
  // while a firmware update is running.
  private async connectTo(id: string, uart: boolean) {
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

      if (!uart) {
        this.setState('connected');
        return;
      }

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

  dfuLink(): DfuLink | null {
    const device = this.device;
    if (!device) {
      return null;
    }
    return new BleDfuLink(this.manager, device.id, (id) => this.connectTo(id, false));
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

// Raw GATT over react-native-ble-plx, which takes and returns base64.
class BleDfuLink implements DfuLink {
  constructor(
    private readonly manager: BleManager,
    private id: string,
    private readonly reconnectTo: (id: string) => Promise<void>,
  ) {}

  get deviceId() {
    return this.id;
  }

  async hasCharacteristic(service: string, characteristic: string): Promise<boolean> {
    try {
      const found = await this.manager.characteristicsForDevice(this.id, service);
      return found.some((item) => item.uuid.toLowerCase() === characteristic.toLowerCase());
    } catch {
      return false;
    }
  }

  async read(service: string, characteristic: string): Promise<Uint8Array> {
    const value = await this.manager.readCharacteristicForDevice(this.id, service, characteristic);
    return value.value ? base64ToBytes(value.value) : new Uint8Array(0);
  }

  async write(service: string, characteristic: string, data: Uint8Array, mode: WriteMode) {
    const payload = bytesToBase64(data);
    if (mode === 'request') {
      await this.manager.writeCharacteristicWithResponseForDevice(
        this.id,
        service,
        characteristic,
        payload,
      );
      return;
    }
    await this.manager.writeCharacteristicWithoutResponseForDevice(
      this.id,
      service,
      characteristic,
      payload,
    );
  }

  async subscribe(
    service: string,
    characteristic: string,
    onValue: (value: Uint8Array) => void,
  ): Promise<() => void> {
    const subscription = this.manager.monitorCharacteristicForDevice(
      this.id,
      service,
      characteristic,
      (_error, item) => {
        if (item?.value) {
          onValue(base64ToBytes(item.value));
        }
      },
    );
    return () => subscription.remove();
  }

  async reconnect(deviceId: string): Promise<void> {
    await this.manager.cancelDeviceConnection(this.id).catch(() => undefined);
    this.id = deviceId;
    await this.reconnectTo(deviceId);
  }
}
