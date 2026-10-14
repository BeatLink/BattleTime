// Web requests the user sets up (the notes URL, remote buttons), with extra headers, a basic-auth login and a
// choice of which TLS certificate to trust.
//
// Checking a certificate any other way than the usual one is out of reach of a window's or a phone's own
// fetch, so the desktop app and the Android app carry such requests out natively (node/web-request.js,
// core/src/web_request.rs, the PhoneBridge module) and only a request that checks certificates as usual
// falls back to fetch.

import { nativeWebBridge } from './web-bridge';

// ============================================================================
// How to reach an address
// ============================================================================

// Which certificate an https address must show: one the system trusts, the one with a given SHA-256
// fingerprint (for a self-signed certificate), or any at all.
export const CERTIFICATE_TRUSTS = ['system', 'pinned', 'any'] as const;
export type CertificateTrust = (typeof CERTIFICATE_TRUSTS)[number];

// Everything about reaching an address besides the address itself; each part is left out when unused.
export type WebAccess = {
  headers?: Record<string, string>;
  username?: string;
  password?: string;
  certificate?: CertificateTrust;
  // The pinned certificate's SHA-256 fingerprint, as 64 lowercase hex digits.
  certificateSha256?: string;
};

const isText = (value: unknown): value is string => typeof value === 'string';

// The fingerprint as 64 lowercase hex digits, whether written with colons, spaces or neither; null when malformed.
export function normalizeFingerprint(text: string): string | null {
  const digits = text.replace(/[\s:]/g, '').toLowerCase();
  return /^[0-9a-f]{64}$/.test(digits) ? digits : null;
}

// The access settings in stored data, dropping anything malformed.
export function readWebAccess(value: unknown): WebAccess {
  if (!value || typeof value !== 'object') {
    return {};
  }
  const stored = value as Record<string, unknown>;
  const access: WebAccess = {};
  if (stored.headers && typeof stored.headers === 'object' && !Array.isArray(stored.headers)) {
    const headers = Object.entries(stored.headers).filter(([, item]) => isText(item)) as [string, string][];
    if (headers.length) access.headers = Object.fromEntries(headers);
  }
  if (isText(stored.username) && stored.username) access.username = stored.username;
  if (isText(stored.password) && stored.password) access.password = stored.password;
  const fingerprint = isText(stored.certificateSha256) ? normalizeFingerprint(stored.certificateSha256) : null;
  if (stored.certificate === 'any') {
    access.certificate = 'any';
  } else if (stored.certificate === 'pinned' && fingerprint) {
    access.certificate = 'pinned';
    access.certificateSha256 = fingerprint;
  }
  return access;
}

function base64(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

// The headers a request sends: the user's own, plus an Authorization header when a login is set.
export function accessHeaders(access: WebAccess): Record<string, string> {
  const headers = { ...access.headers };
  if (access.username || access.password) {
    const login = new TextEncoder().encode(`${access.username ?? ''}:${access.password ?? ''}`);
    headers.Authorization = `Basic ${base64(login)}`;
  }
  return headers;
}

// ============================================================================
// Requests
// ============================================================================

export type WebRequest = {
  method: string;
  url: string;
  // Headers this request needs on top of the access settings' own, which win where both name one.
  headers?: Record<string, string>;
  body?: string;
  access: WebAccess;
  timeoutMs: number;
};

export type WebReply = { status: number; statusText: string; body: Uint8Array };

export type WebRequester = (request: WebRequest) => Promise<WebReply>;

// What the desktop app's and the Android app's native side take and give back.
export type NativeWebRequest = {
  method: string;
  url: string;
  headers: Record<string, string>;
  body: string | null;
  timeoutMs: number;
  certificate: CertificateTrust;
  certificateSha256: string | null;
};

type BridgeResult<Value> = { ok: boolean; error?: string; value?: Value };

export type NativeWebReply = { status: number; statusText: string; body: Uint8Array | number[] };

export type NativeWebBridge = { request(request: NativeWebRequest): Promise<BridgeResult<NativeWebReply>> };

// Checks the address and gives the request in the form the native side takes.
export function nativeRequest(request: WebRequest): NativeWebRequest {
  if (!/^https?:\/\//i.test(request.url)) {
    throw new Error(`${request.url} is not an http or https URL.`);
  }
  const certificate = request.access.certificate ?? 'system';
  const certificateSha256 = certificate === 'pinned' ? (request.access.certificateSha256 ?? null) : null;
  if (certificate === 'pinned' && certificateSha256 === null) {
    throw new Error('Give the SHA-256 fingerprint of the certificate to trust.');
  }
  return {
    method: request.method,
    url: request.url,
    headers: { ...request.headers, ...accessHeaders(request.access) },
    body: request.body ?? null,
    timeoutMs: request.timeoutMs,
    certificate,
    certificateSha256,
  };
}

// Requests through the desktop app's or the Android app's native side.
export function bridgeRequester(bridge: NativeWebBridge): WebRequester {
  return async (request) => {
    const result = await bridge.request(nativeRequest(request));
    if (!result.ok || !result.value) {
      throw new Error(result.error ?? 'The request could not be made.');
    }
    const { status, statusText, body } = result.value;
    return { status, statusText, body: new Uint8Array(body) };
  };
}

// Requests through fetch, which can only check certificates the usual way.
export function fetchRequester(fetcher: typeof fetch = fetch): WebRequester {
  return async (request) => {
    const native = nativeRequest(request);
    if (native.certificate !== 'system') {
      throw new Error('Trusting a self-signed certificate needs the desktop app or the Android app.');
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), native.timeoutMs);
    try {
      const response = await fetcher(native.url, {
        method: native.method,
        headers: native.headers,
        body: native.body ?? undefined,
        signal: controller.signal,
      });
      return {
        status: response.status,
        statusText: response.statusText,
        body: new Uint8Array(await response.arrayBuffer()),
      };
    } catch (error) {
      if (controller.signal.aborted) {
        throw new Error(`No answer within ${native.timeoutMs / 1000} s.`);
      }
      throw error;
    } finally {
      clearTimeout(timer);
    }
  };
}

// The way this device makes requests: natively where it can, otherwise through fetch.
export function webRequester(): WebRequester {
  const bridge = nativeWebBridge();
  return bridge ? bridgeRequester(bridge) : fetchRequester();
}

export const replyText = (reply: WebReply) => new TextDecoder().decode(reply.body);
