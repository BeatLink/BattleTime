// The editor's fields for reaching a web address (headers, a login, which certificate to trust), shared by
// the notes URL and remote buttons, and back into checked access settings.

import { normalizeFingerprint, type CertificateTrust, type WebAccess } from '@/services/web-request';

export type WebAccessForm = {
  // One "Name: value" per line.
  headers: string;
  username: string;
  password: string;
  certificate: CertificateTrust;
  certificateSha256: string;
};

export const EMPTY_WEB_ACCESS_FORM: WebAccessForm = {
  headers: '',
  username: '',
  password: '',
  certificate: 'system',
  certificateSha256: '',
};

export const joinPairs = (pairs: Record<string, string> | undefined, separator: string) =>
  Object.entries(pairs ?? {})
    .map(([key, value]) => `${key}${separator}${value}`)
    .join('\n');

// Reads lines of key and value split at the first `separator`, ignoring blank lines.
export function parsePairs(text: string, separator: string, what: string): Record<string, string> {
  const pairs: Record<string, string> = {};
  for (const line of text.split('\n')) {
    if (line.trim() === '') continue;
    const at = line.indexOf(separator);
    const key = at > 0 ? line.slice(0, at).trim() : '';
    if (!key) {
      throw new Error(`Write each ${what} as name${separator === ':' ? ': ' : '='}value, not "${line.trim()}".`);
    }
    pairs[key] = line.slice(at + 1).trim();
  }
  return pairs;
}

// The fields for stored access settings.
export function webAccessForm(access: WebAccess): WebAccessForm {
  return {
    headers: joinPairs(access.headers, ': '),
    username: access.username ?? '',
    password: access.password ?? '',
    certificate: access.certificate ?? 'system',
    certificateSha256: access.certificateSha256 ?? '',
  };
}

// The access settings the fields describe; throws with a sentence for the screen when one is malformed.
export function webAccessFromForm(form: WebAccessForm): WebAccess {
  const access: WebAccess = {};
  const headers = parsePairs(form.headers, ':', 'header');
  if (Object.keys(headers).length) access.headers = headers;
  if (form.username) access.username = form.username;
  if (form.password) access.password = form.password;
  if (form.certificate === 'pinned') {
    const fingerprint = normalizeFingerprint(form.certificateSha256);
    if (!fingerprint) {
      throw new Error("The certificate's SHA-256 fingerprint is 64 hex digits, with or without colons.");
    }
    access.certificate = 'pinned';
    access.certificateSha256 = fingerprint;
  } else if (form.certificate === 'any') {
    access.certificate = 'any';
  }
  return access;
}
