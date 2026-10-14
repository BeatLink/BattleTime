import { afterAll, beforeAll, describe, expect, it } from '@jest/globals';
import { readFileSync } from 'node:fs';
import type { IncomingMessage } from 'node:http';
import { createServer, type Server } from 'node:https';
import type { AddressInfo } from 'node:net';
import { join } from 'node:path';

import { accessHeaders, nativeRequest, type WebAccess } from '@/services/web-request';

import { webRequest } from './web-request';

// ============================================================================
// A server with a self-signed certificate
// ============================================================================

// The test certificate in testdata/, made with openssl for localhost and 127.0.0.1.
const PINNED = 'cea33b3710ae855abf9478448811ed363a0135e0210947e3d027b96118554160';

let server: Server;
let base = '';
const seen: { method?: string; authorization?: string; custom?: string; body: string }[] = [];

beforeAll(async () => {
  server = createServer(
    {
      key: readFileSync(join(__dirname, 'testdata/self-signed.key')),
      cert: readFileSync(join(__dirname, 'testdata/self-signed.crt')),
    },
    (request: IncomingMessage, response) => {
      let body = '';
      request.on('data', (chunk: Buffer) => (body += chunk));
      request.on('end', () => {
        seen.push({
          method: request.method,
          authorization: request.headers.authorization,
          custom: request.headers['x-custom'] as string | undefined,
          body,
        });
        if (request.url === '/moved') {
          response.writeHead(302, { Location: '/note' });
          response.end();
          return;
        }
        response.writeHead(200, { 'Content-Type': 'text/plain' });
        response.end('Milk and eggs');
      });
    },
  );
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  base = `https://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(() => {
  server.close();
});

const send = (path: string, access: WebAccess, method = 'GET', body?: string) =>
  webRequest(nativeRequest({ method, url: `${base}${path}`, body, access, timeoutMs: 5000 }));

const text = (body: Uint8Array | number[]) => new TextDecoder().decode(new Uint8Array(body));

// ============================================================================
// Certificates, logins and headers
// ============================================================================

describe('webRequest', () => {
  it('refuses a self-signed certificate unless it is pinned or any is trusted', async () => {
    await expect(send('/note', {})).rejects.toThrow(/self[- ]signed/i);

    const pinned = await send('/note', { certificate: 'pinned', certificateSha256: PINNED });
    expect(pinned.status).toBe(200);
    expect(text(pinned.body)).toBe('Milk and eggs');

    const any = await send('/note', { certificate: 'any' });
    expect(text(any.body)).toBe('Milk and eggs');
  });

  it('sends nothing to a server whose certificate is not the pinned one', async () => {
    seen.length = 0;
    await expect(
      send('/note', { certificate: 'pinned', certificateSha256: '0'.repeat(64), username: 'me', password: 'secret' }),
    ).rejects.toThrow(`showed a certificate with SHA-256 fingerprint ${PINNED}, not the pinned one`);
    expect(seen).toEqual([]);
  });

  it('sends the login, custom headers and the body, and follows a redirect', async () => {
    seen.length = 0;
    const access: WebAccess = {
      certificate: 'any',
      username: 'me',
      password: 'pässword',
      headers: { 'X-Custom': 'yes' },
    };

    await send('/note', access, 'POST', 'hello');
    const moved = await send('/moved', access);

    expect(seen[0]).toEqual({
      method: 'POST',
      authorization: accessHeaders(access).Authorization,
      custom: 'yes',
      body: 'hello',
    });
    expect(Buffer.from(seen[0].authorization!.slice('Basic '.length), 'base64').toString('utf8')).toBe('me:pässword');
    expect(text(moved.body)).toBe('Milk and eggs');
    expect(seen).toHaveLength(3);
  });
});
