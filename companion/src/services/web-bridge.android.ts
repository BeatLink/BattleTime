// The native side of web requests on Android: WebRequest.kt in modules/phone-bridge; Expo Go lacks it.
import { requireOptionalNativeModule } from 'expo';

import type { NativeWebBridge, NativeWebRequest } from './web-request';

type NativeReply = { status: number; statusText: string; bodyBase64: string };
type NativeWeb = { httpRequest?: (request: NativeWebRequest) => Promise<NativeReply> };

function fromBase64(text: string): Uint8Array {
  const binary = atob(text);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

// The module's request function as a bridge, or null when the module or the function is missing.
export function androidWebBridge(native: NativeWeb | null): NativeWebBridge | null {
  const httpRequest = native?.httpRequest;
  if (!httpRequest) {
    return null;
  }
  return {
    async request(request) {
      try {
        const reply = await httpRequest(request);
        return { ok: true, value: { status: reply.status, statusText: reply.statusText, body: fromBase64(reply.bodyBase64) } };
      } catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
      }
    },
  };
}

const bridge = androidWebBridge(requireOptionalNativeModule<NativeWeb>('PhoneBridge'));

export function nativeWebBridge(): NativeWebBridge | null {
  return bridge;
}
