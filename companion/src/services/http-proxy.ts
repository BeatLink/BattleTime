// Fetches URLs for watch apps, but only from hosts the user has allowed.

import type { HttpRequestMessage, HttpResponseMessage } from '@/protocol/gadgetbridge';

// Longest answer sent back, since the watch has only a few kilobytes of heap.
export const MAX_RESPONSE = 1024;

type Fetch = typeof fetch;

// A host is allowed when it, or a domain it belongs to, is on the list.
export function isHostAllowed(host: string, allowed: string[]): boolean {
  const name = host.toLowerCase();
  return allowed.some((entry) => {
    const rule = entry.trim().toLowerCase();
    return rule !== '' && (name === rule || name.endsWith(`.${rule}`));
  });
}

export function parseHostList(text: string): string[] {
  return text
    .split(/[\s,]+/)
    .map((host) => host.trim())
    .filter(Boolean);
}

export async function answerHttpRequest(
  request: HttpRequestMessage,
  allowed: string[],
  fetcher: Fetch = fetch,
): Promise<HttpResponseMessage> {
  let url: URL;
  try {
    url = new URL(request.url);
  } catch {
    return { t: 'http', id: request.id, err: 'bad url' };
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') {
    return { t: 'http', id: request.id, err: 'only http and https' };
  }
  if (!isHostAllowed(url.hostname, allowed)) {
    return { t: 'http', id: request.id, err: `${url.hostname} is not allowed` };
  }
  try {
    const response = await fetcher(url.toString());
    if (!response.ok) {
      return { t: 'http', id: request.id, err: `HTTP ${response.status}` };
    }
    const text = await response.text();
    return { t: 'http', id: request.id, resp: text.slice(0, MAX_RESPONSE) };
  } catch (error) {
    return { t: 'http', id: request.id, err: (error as Error).message || 'fetch failed' };
  }
}
