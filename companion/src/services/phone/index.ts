// The desktop app reads the computer through its preload bridge; iOS and browsers have no bridge; Android uses index.android.ts.
import { createDesktopPhoneBridge, desktopDeviceBridge } from './desktop';
import { unavailablePhoneBridge } from './stub';
import type { PhoneBridge } from './types';

export type { BridgeKind, DeviceLocation, MediaCommand, PhoneBridge, PhoneEvent } from './types';

const desktop = desktopDeviceBridge();

export const phoneBridge: PhoneBridge = desktop ? createDesktopPhoneBridge(desktop) : unavailablePhoneBridge;
