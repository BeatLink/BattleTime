// Android intents for remote buttons, started by the native module in modules/phone-bridge; Expo Go lacks it.
import { requireOptionalNativeModule } from 'expo';

import type { AndroidIntents } from './remote-actions';

type NativeIntents = Partial<AndroidIntents>;

// The module's three intent functions, or null when the module or any of them is missing.
export function nativeIntents(native: NativeIntents | null): AndroidIntents | null {
  if (!native?.openUri || !native.startActivity || !native.sendBroadcast) {
    return null;
  }
  return {
    openUri: (uri) => native.openUri!(uri),
    startActivity: (packageName, className, action, extras) =>
      native.startActivity!(packageName, className, action, extras),
    sendBroadcast: (action, packageName, extras) => native.sendBroadcast!(action, packageName, extras),
  };
}

export const androidIntents: AndroidIntents | null = nativeIntents(requireOptionalNativeModule<NativeIntents>('PhoneBridge'));
