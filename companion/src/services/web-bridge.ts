// The native side of web requests: the app's Rust core, through window.neotime.web on the desktop and Android.
import type { NativeWebBridge } from './web-request';

export function nativeWebBridge(): NativeWebBridge | null {
  return (globalThis as { neotime?: { web?: NativeWebBridge } }).neotime?.web ?? null;
}
