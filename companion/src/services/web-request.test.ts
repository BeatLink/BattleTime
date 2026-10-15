import { afterEach, describe, expect, it, jest } from '@jest/globals';

import {
  accessHeaders,
  bridgeRequester,
  fetchRequester,
  nativeRequest,
  normalizeFingerprint,
  readWebAccess,
  webRequester,
  type NativeWebRequest,
  type WebRequest,
} from './web-request';

const FINGERPRINT = 'ab'.repeat(32);

const request = (overrides: Partial<WebRequest> = {}): WebRequest => ({
  method: 'GET',
  url: 'https://nas.local/note',
  access: {},
  timeoutMs: 1000,
  ...overrides,
});

afterEach(() => {
  delete (globalThis as { battletime?: unknown }).battletime;
});

// ============================================================================
// Access settings
// ============================================================================

describe('access settings', () => {
  it('reads a fingerprint with or without colons, and refuses a malformed one', () => {
    expect(normalizeFingerprint('AB:'.repeat(31) + 'AB')).toBe(FINGERPRINT);
    expect(normalizeFingerprint(` ${FINGERPRINT.toUpperCase()} `)).toBe(FINGERPRINT);
    expect(normalizeFingerprint('abc')).toBeNull();
  });

  it('keeps only well-formed stored settings', () => {
    expect(readWebAccess(null)).toEqual({});
    expect(
      readWebAccess({
        headers: { 'X-Key': '1', Bad: 2 },
        username: 'me',
        password: '',
        certificate: 'pinned',
        certificateSha256: FINGERPRINT.toUpperCase(),
      }),
    ).toEqual({ headers: { 'X-Key': '1' }, username: 'me', certificate: 'pinned', certificateSha256: FINGERPRINT });
    // A pin without its fingerprint falls back to the usual check rather than trusting anything.
    expect(readWebAccess({ certificate: 'pinned' })).toEqual({});
    expect(readWebAccess({ certificate: 'any' })).toEqual({ certificate: 'any' });
  });

  it('adds a basic-auth header for a login, in UTF-8', () => {
    expect(accessHeaders({ headers: { 'X-Key': '1' } })).toEqual({ 'X-Key': '1' });
    expect(accessHeaders({ username: 'me', password: 'pässword' }).Authorization).toBe(
      `Basic ${Buffer.from('me:pässword', 'utf8').toString('base64')}`,
    );
  });

  it("gives the native side the request with the access settings' headers winning", () => {
    expect(
      nativeRequest(
        request({
          headers: { Accept: 'text/plain', 'X-Key': 'request' },
          body: 'hi',
          access: { headers: { 'X-Key': 'access' }, certificate: 'pinned', certificateSha256: FINGERPRINT },
        }),
      ),
    ).toEqual({
      method: 'GET',
      url: 'https://nas.local/note',
      headers: { Accept: 'text/plain', 'X-Key': 'access' },
      body: 'hi',
      timeoutMs: 1000,
      certificate: 'pinned',
      certificateSha256: FINGERPRINT,
    });
    expect(() => nativeRequest(request({ url: 'ftp://nas.local' }))).toThrow('is not an http or https URL');
    expect(() => nativeRequest(request({ access: { certificate: 'pinned' } }))).toThrow('SHA-256 fingerprint');
  });
});

// ============================================================================
// Requesters
// ============================================================================

describe('requesters', () => {
  it('uses the native bridge, and reports its failure', async () => {
    const sent: NativeWebRequest[] = [];
    const requester = bridgeRequester({
      async request(native) {
        sent.push(native);
        return native.url.endsWith('/fail')
          ? { ok: false, error: 'certificate mismatch' }
          : { ok: true, value: { status: 200, statusText: 'OK', body: [104, 105] } };
      },
    });

    const reply = await requester(request({ access: { certificate: 'any' } }));

    expect(reply).toEqual({ status: 200, statusText: 'OK', body: new Uint8Array([104, 105]) });
    expect(sent[0].certificate).toBe('any');
    await expect(requester(request({ url: 'https://nas.local/fail' }))).rejects.toThrow('certificate mismatch');
  });

  it('falls back to fetch only for the usual certificate check, and gives up on a silent server', async () => {
    const fetcher = jest.fn(async (_url: string, _init?: RequestInit) => new Response('ok', { status: 201 }));
    const requester = fetchRequester(fetcher as unknown as typeof fetch);

    await expect(requester(request({ access: { username: 'me' } }))).resolves.toMatchObject({ status: 201 });
    expect(fetcher.mock.calls[0][1]?.headers).toEqual({ Authorization: `Basic ${btoa('me:')}` });
    await expect(requester(request({ access: { certificate: 'any' } }))).rejects.toThrow('needs the desktop app');

    jest.useFakeTimers();
    try {
      fetcher.mockImplementationOnce(
        (_url, init) =>
          new Promise((_resolve, reject) => init?.signal?.addEventListener('abort', () => reject(new Error('aborted')))),
      );
      const pending = requester(request());
      jest.advanceTimersByTime(1000);
      await expect(pending).rejects.toThrow('No answer within 1 s.');
    } finally {
      jest.useRealTimers();
    }
  });

  it("picks the desktop app's bridge when the window has one", async () => {
    const web = { request: jest.fn(async () => ({ ok: true, value: { status: 200, statusText: '', body: [] } })) };
    (globalThis as { battletime?: unknown }).battletime = { web };

    await webRequester()(request({ access: { certificate: 'any' } }));

    expect(web.request).toHaveBeenCalled();
  });
});
