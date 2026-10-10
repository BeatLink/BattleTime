// The phone bridge: what the phone tells the app about notifications, calls and media, and what the app can ask of it.

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

export interface PhoneBridge {
  /** False off Android, and in Expo Go, where the native module is missing. */
  readonly available: boolean;
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
}
