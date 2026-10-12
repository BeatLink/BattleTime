import { describe, expect, it } from '@jest/globals';

import { toPhoneEvent } from './events';

describe('toPhoneEvent', () => {
  it('rejects values that are not objects', () => {
    expect(toPhoneEvent(undefined)).toBeNull();
    expect(toPhoneEvent('notification')).toBeNull();
    expect(toPhoneEvent(42)).toBeNull();
  });

  it('needs a finite numeric id on a notification', () => {
    expect(toPhoneEvent({ type: 'notification', title: 'Hi' })).toBeNull();
    expect(toPhoneEvent({ type: 'notification', id: '7' })).toBeNull();
    expect(toPhoneEvent({ type: 'notification', id: Number.NaN })).toBeNull();
    expect(toPhoneEvent({ type: 'notificationRemoved', id: Infinity })).toBeNull();
  });

  it('keeps notification text and ignores text that is not a string', () => {
    expect(toPhoneEvent({ type: 'notification', id: 1, app: 'Mail', title: 'Hello', body: 5 })).toEqual({
      type: 'notification',
      id: 1,
      app: 'Mail',
      title: 'Hello',
      body: '',
    });
    expect(toPhoneEvent({ type: 'notification', id: 2, body: 'Body only' })).toEqual({
      type: 'notification',
      id: 2,
      app: '',
      title: '',
      body: 'Body only',
    });
  });

  it('reads a removed notification by its id', () => {
    expect(toPhoneEvent({ type: 'notificationRemoved', id: 9 })).toEqual({ type: 'notificationRemoved', id: 9 });
  });

  it('keeps the caller name and number when present', () => {
    expect(toPhoneEvent({ type: 'call', state: 'end', name: 'Ann', number: '555' })).toEqual({
      type: 'call',
      state: 'end',
      name: 'Ann',
      number: '555',
    });
    expect(toPhoneEvent({ type: 'call', state: 'start' })).toEqual({ type: 'call', state: 'start' });
  });

  it('reads media details, and counts only a true playing flag as playing', () => {
    expect(
      toPhoneEvent({
        type: 'media',
        playing: true,
        artist: 'A',
        album: 'B',
        track: 'C',
        durationSec: 200,
        positionSec: 12,
      }),
    ).toEqual({ type: 'media', playing: true, artist: 'A', album: 'B', track: 'C', durationSec: 200, positionSec: 12 });
    expect(toPhoneEvent({ type: 'media', playing: 'yes', durationSec: Number.NaN })).toEqual({
      type: 'media',
      playing: false,
    });
  });
});
