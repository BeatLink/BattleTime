import { describe, expect, it, jest } from '@jest/globals';

import type { NativeWebRequest } from './web-request';

jest.mock('expo', () => ({ requireOptionalNativeModule: () => null }));

// eslint-disable-next-line import/first
import { androidWebBridge, nativeWebBridge } from './web-bridge.android';

const REQUEST: NativeWebRequest = {
  method: 'GET',
  url: 'https://nas.local/note',
  headers: {},
  body: null,
  timeoutMs: 1000,
  certificate: 'any',
  certificateSha256: null,
};

describe('the Android web bridge', () => {
  it('is missing without the native module, as in Expo Go', () => {
    expect(nativeWebBridge()).toBeNull();
    expect(androidWebBridge({})).toBeNull();
  });

  it("decodes the module's base64 body and passes its failure on", async () => {
    const bridge = androidWebBridge({
      httpRequest: async (request) => {
        if (request.url.endsWith('/fail')) throw new Error('not the pinned certificate');
        return { status: 200, statusText: 'OK', bodyBase64: btoa('Milk') };
      },
    })!;

    await expect(bridge.request(REQUEST)).resolves.toEqual({
      ok: true,
      value: { status: 200, statusText: 'OK', body: new TextEncoder().encode('Milk') },
    });
    await expect(bridge.request({ ...REQUEST, url: 'https://nas.local/fail' })).resolves.toEqual({
      ok: false,
      error: 'not the pinned certificate',
    });
  });
});
