// The app on Android reads the phone through its plugin, and on the desktop reads the computer through its bridge;
// a plain browser has neither.
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
