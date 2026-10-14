// Small UTF-8 and base64 helpers so the transport does not depend on Node globals.

const BASE64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

export function utf8Encode(text: string): Uint8Array {
  const escaped = unescape(encodeURIComponent(text));
  const bytes = new Uint8Array(escaped.length);
  for (let i = 0; i < escaped.length; i++) {
    bytes[i] = escaped.charCodeAt(i);
  }
  return bytes;
}

export function utf8Decode(bytes: Uint8Array): string {
  let escaped = '';
  for (const byte of bytes) {
    escaped += String.fromCharCode(byte);
  }
  try {
    return decodeURIComponent(escape(escaped));
  } catch {
    return escaped;
  }
}

export function bytesToBase64(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i];
    const b = i + 1 < bytes.length ? bytes[i + 1] : 0;
    const c = i + 2 < bytes.length ? bytes[i + 2] : 0;
    const triple = (a << 16) | (b << 8) | c;
    out += BASE64[(triple >> 18) & 63];
    out += BASE64[(triple >> 12) & 63];
    out += i + 1 < bytes.length ? BASE64[(triple >> 6) & 63] : '=';
    out += i + 2 < bytes.length ? BASE64[triple & 63] : '=';
  }
  return out;
}

export function base64ToBytes(base64: string): Uint8Array {
  const clean = base64.replace(/[^A-Za-z0-9+/]/g, '');
  const bytes: number[] = [];
  for (let i = 0; i < clean.length; i += 4) {
    const chunk = [0, 1, 2, 3].map((offset) => BASE64.indexOf(clean[i + offset] ?? 'A'));
    const triple = (chunk[0] << 18) | (chunk[1] << 12) | (chunk[2] << 6) | chunk[3];
    bytes.push((triple >> 16) & 255);
    if (i + 2 < clean.length) bytes.push((triple >> 8) & 255);
    if (i + 3 < clean.length) bytes.push(triple & 255);
  }
  return Uint8Array.from(bytes);
}
