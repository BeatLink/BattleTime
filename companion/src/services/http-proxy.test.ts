import { describe, expect, it } from '@jest/globals';

import { answerHttpRequest, isHostAllowed, parseHostList } from './http-proxy';

const allowed = ['example.com'];

function failingFetch(error: Error): typeof fetch {
  return (async () => {
    throw error;
  }) as typeof fetch;
}

describe('isHostAllowed', () => {
  it('ignores case and blank entries', () => {
    expect(isHostAllowed('API.Example.COM', [' Example.com '])).toBe(true);
    expect(isHostAllowed('example.com', ['', '   '])).toBe(false);
    expect(isHostAllowed('example.com', [])).toBe(false);
  });
});

describe('parseHostList', () => {
  it('gives an empty list for empty text', () => {
    expect(parseHostList('')).toEqual([]);
    expect(parseHostList(' ,\n, ')).toEqual([]);
  });
});

describe('answerHttpRequest', () => {
  it('refuses a URL that does not parse', async () => {
    await expect(answerHttpRequest({ t: 'http', id: '1', url: 'not a url' }, allowed)).resolves.toEqual({
      t: 'http',
      id: '1',
      err: 'bad url',
    });
  });

  it('allows plain http as well as https', async () => {
    const fetcher = (async () => new Response('plain')) as typeof fetch;
    const reply = await answerHttpRequest({ t: 'http', id: '1', url: 'http://example.com/' }, allowed, fetcher);
    expect(reply).toEqual({ t: 'http', id: '1', resp: 'plain' });
  });

  it('sends no body with a HEAD', async () => {
    let sent: RequestInit | undefined;
    const fetcher = (async (_url: RequestInfo | URL, init?: RequestInit) => {
      sent = init;
      return new Response(null);
    }) as typeof fetch;
    const reply = await answerHttpRequest(
      { t: 'http', id: '1', url: 'https://example.com/', method: 'head', body: 'dropped' },
      allowed,
      fetcher,
    );
    expect(sent).toEqual({ method: 'HEAD', body: undefined });
    expect(reply.resp).toBe('');
  });

  it('passes on an error status from the server', async () => {
    const fetcher = (async () => new Response('gone', { status: 404 })) as typeof fetch;
    const reply = await answerHttpRequest({ t: 'http', id: '7', url: 'https://example.com/x' }, allowed, fetcher);
    expect(reply).toEqual({ t: 'http', id: '7', err: 'HTTP 404' });
  });

  it('passes on why the fetch failed, or a stock message when it gives none', async () => {
    const offline = await answerHttpRequest(
      { t: 'http', id: '1', url: 'https://example.com/' },
      allowed,
      failingFetch(new TypeError('Network request failed')),
    );
    expect(offline.err).toBe('Network request failed');
    const silent = await answerHttpRequest(
      { t: 'http', id: '2', url: 'https://example.com/' },
      allowed,
      failingFetch(new Error('')),
    );
    expect(silent.err).toBe('fetch failed');
  });
});
