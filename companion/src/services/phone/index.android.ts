// Android talks to the native module in modules/phone-bridge; Expo Go lacks it and gets the stub.
import { requireOptionalNativeModule } from 'expo';

import { toPhoneEvent } from './events';
import { unavailablePhoneBridge } from './stub';
import type { MediaCommand, PhoneBridge, PhoneEvent } from './types';

export type { MediaCommand, PhoneBridge, PhoneEvent } from './types';

interface NativePhoneBridge {
  hasNotificationAccess(): Promise<boolean>;
  openNotificationAccessSettings(): Promise<void>;
  requestCallPermissions(): Promise<boolean>;
  isDoNotDisturb(): Promise<boolean>;
  answerCall(): Promise<void>;
  rejectCall(): Promise<void>;
  mediaCommand(command: MediaCommand): Promise<void>;
  startKeepAlive(watchName: string): Promise<void>;
  stopKeepAlive(): Promise<void>;
  addListener(eventName: 'onPhoneEvent', listener: (event: unknown) => void): { remove(): void };
}

function nativeBridge(native: NativePhoneBridge): PhoneBridge {
  return {
    available: true,
    hasNotificationAccess: () => native.hasNotificationAccess(),
    openNotificationAccessSettings: () => native.openNotificationAccessSettings(),
    requestCallPermissions: () => native.requestCallPermissions(),
    isDoNotDisturb: () => native.isDoNotDisturb(),
    subscribe(listener: (event: PhoneEvent) => void) {
      const subscription = native.addListener('onPhoneEvent', (raw) => {
        const event = toPhoneEvent(raw);
        if (event) listener(event);
      });
      return () => subscription.remove();
    },
    answerCall: () => native.answerCall(),
    rejectCall: () => native.rejectCall(),
    mediaCommand: (command) => native.mediaCommand(command),
    startKeepAlive: (watchName) => native.startKeepAlive(watchName),
    stopKeepAlive: () => native.stopKeepAlive(),
  };
}

const native = requireOptionalNativeModule<NativePhoneBridge>('PhoneBridge');

export const phoneBridge: PhoneBridge = native ? nativeBridge(native) : unavailablePhoneBridge;
