// What a DFU controller needs from Bluetooth, which is more than the UART
// line the rest of the app uses: arbitrary characteristics, notifications and
// a way to follow the watch into its bootloader.

export type WriteMode = 'request' | 'command';

export interface DfuLink {
  // The address of the peer currently connected.
  readonly deviceId: string;
  hasCharacteristic(service: string, characteristic: string): Promise<boolean>;
  read(service: string, characteristic: string): Promise<Uint8Array>;
  write(service: string, characteristic: string, data: Uint8Array, mode: WriteMode): Promise<void>;
  // Subscribe to a characteristic, resolving to a function that unsubscribes.
  subscribe(
    service: string,
    characteristic: string,
    onValue: (value: Uint8Array) => void,
  ): Promise<() => void>;
  // Drop the link and connect to another peer, for the jump into the bootloader.
  reconnect(deviceId: string): Promise<void>;
}

export class DfuError extends Error {}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// Collects control point notifications so a controller can await the next one
// without racing the radio, which can answer before the await starts.
export class NotifyQueue {
  private readonly pending: Uint8Array[] = [];
  private waiting: ((value: Uint8Array) => void) | null = null;
  private failure: Error | null = null;
  private onFailure: ((error: Error) => void) | null = null;

  push(value: Uint8Array) {
    if (this.waiting) {
      const resolve = this.waiting;
      this.waiting = null;
      resolve(value);
      return;
    }
    this.pending.push(value);
  }

  // Fail whoever is waiting, and whoever waits next, after a lost link.
  fail(error: Error) {
    this.failure = error;
    this.onFailure?.(error);
  }

  next(timeoutMs = 60000): Promise<Uint8Array> {
    const queued = this.pending.shift();
    if (queued) {
      return Promise.resolve(queued);
    }
    if (this.failure) {
      return Promise.reject(this.failure);
    }
    return new Promise<Uint8Array>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting = null;
        this.onFailure = null;
        reject(new DfuError('The watch stopped answering.'));
      }, timeoutMs);
      this.waiting = (value) => {
        clearTimeout(timer);
        this.onFailure = null;
        resolve(value);
      };
      this.onFailure = (error) => {
        clearTimeout(timer);
        this.waiting = null;
        reject(error);
      };
    });
  }
}
