// The phone bridge: what the phone, or the computer running the desktop app, tells the app about notifications, calls and media.

export type MediaCommand = 'play' | 'pause' | 'next' | 'previous' | 'volumeup' | 'volumedown';

export type PhoneEvent =
  | { type: 'notification'; id: number; app: string; title: string; body: string }
  | { type: 'notificationRemoved'; id: number }
  | { type: 'call'; state: 'incoming' | 'outgoing' | 'start' | 'end'; name?: string; number?: string }
  | {
      type: 'media';
      playing: boolean;
      artist?: string;
      album?: string;
      track?: string;
      durationSec?: number;
      positionSec?: number;
    };

// Which device plays the phone's part: an Android phone, the computer under the desktop app, or nothing.
export type BridgeKind = 'phone' | 'computer' | 'none';

export type DeviceLocation = { latitude: number; longitude: number; name: string };

export interface PhoneBridge {
  /** False where there is nothing to read: iOS, a browser, and Expo Go, where the native module is missing. */
  readonly available: boolean;
  readonly kind: BridgeKind;
  /** Whether calls can be followed and answered here; a computer has none. */
  readonly supportsCalls: boolean;
  /** Whether `locate` can find where the device is, for the weather. */
  readonly supportsLocation: boolean;
  hasNotificationAccess(): Promise<boolean>;
  openNotificationAccessSettings(): Promise<void>;
  /** Resolves true when call state can be read and calls answered; the number and name need the call log and contacts too. */
  requestCallPermissions(): Promise<boolean>;
  /** True whenever the interruption filter is anything but "all". */
  isDoNotDisturb(): Promise<boolean>;
  subscribe(listener: (event: PhoneEvent) => void): () => void;
  answerCall(): Promise<void>;
  rejectCall(): Promise<void>;
  mediaCommand(command: MediaCommand): Promise<void>;
  /** Starts a foreground service so the Bluetooth link survives the app going to the background. */
  startKeepAlive(watchName: string): Promise<void>;
  stopKeepAlive(): Promise<void>;
  locate(): Promise<DeviceLocation>;
}
