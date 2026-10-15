// The native side of web requests: the app's Rust core, through window.battletime.web on the desktop and Android.
import type { NativeWebBridge } from './web-request';

export function nativeWebBridge(): NativeWebBridge | null {
  return (globalThis as { battletime?: { web?: NativeWebBridge } }).battletime?.web ?? null;
}
