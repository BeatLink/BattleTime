// The Tauri app on Android reads the phone through its plugin, the desktop app reads the computer through its bridge;
// iOS and browsers have neither, and the Expo Android app uses index.android.ts.
import { createDesktopPhoneBridge, desktopDeviceBridge } from './desktop';
import { unavailablePhoneBridge } from './stub';
import { createTauriPhoneBridge, tauriPhoneDevice } from './tauri-android';
import type { PhoneBridge } from './types';

export type { BridgeKind, DeviceLocation, MediaCommand, PhoneBridge, PhoneEvent } from './types';

function chooseBridge(): PhoneBridge {
  const phone = tauriPhoneDevice();
  if (phone) {
    return createTauriPhoneBridge(phone);
  }
  const desktop = desktopDeviceBridge();
  return desktop ? createDesktopPhoneBridge(desktop) : unavailablePhoneBridge;
}

export const phoneBridge: PhoneBridge = chooseBridge();
