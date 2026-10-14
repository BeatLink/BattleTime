import { describe, expect, it } from '@jest/globals';

import { EMPTY_WEB_ACCESS_FORM, webAccessForm, webAccessFromForm } from './web-access-form';

describe('the web access fields', () => {
  it('turns stored settings into fields and back', () => {
    const access = {
      headers: { 'X-Key': '1', Accept: 'text/plain' },
      username: 'me',
      password: 'secret',
      certificate: 'pinned' as const,
      certificateSha256: 'ab'.repeat(32),
    };

    const form = webAccessForm(access);

    expect(form.headers).toBe('X-Key: 1\nAccept: text/plain');
    expect(webAccessFromForm(form)).toEqual(access);
    expect(webAccessFromForm(EMPTY_WEB_ACCESS_FORM)).toEqual({});
  });

  it('takes a fingerprint written with colons, and refuses a malformed one or a malformed header', () => {
    const pinned = { ...EMPTY_WEB_ACCESS_FORM, certificate: 'pinned' as const };

    expect(webAccessFromForm({ ...pinned, certificateSha256: 'AB:'.repeat(31) + 'AB' }).certificateSha256).toBe(
      'ab'.repeat(32),
    );
    expect(() => webAccessFromForm({ ...pinned, certificateSha256: 'abc' })).toThrow('64 hex digits');
    expect(() => webAccessFromForm({ ...EMPTY_WEB_ACCESS_FORM, headers: 'no colon' })).toThrow('name: value');
  });

  it('keeps the trust-any choice without a fingerprint', () => {
    expect(webAccessFromForm({ ...EMPTY_WEB_ACCESS_FORM, certificate: 'any', certificateSha256: 'stale' })).toEqual({
      certificate: 'any',
    });
  });
});
