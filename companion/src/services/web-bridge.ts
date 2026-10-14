// The native side of web requests: the desktop app's bridge here, the PhoneBridge module in web-bridge.android.ts.
import type { NativeWebBridge } from './web-request';

export function nativeWebBridge(): NativeWebBridge | null {
  return (globalThis as { neotime?: { web?: NativeWebBridge } }).neotime?.web ?? null;
}
