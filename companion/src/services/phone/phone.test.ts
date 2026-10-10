import { describe, expect, it, jest } from '@jest/globals';

import { toPhoneEvent } from './events';
import { phoneBridge } from './index';
import { unavailablePhoneBridge } from './stub';

describe('phone bridge stub', () => {
  it('is what platforms without the native module get', () => {
    expect(phoneBridge).toBe(unavailablePhoneBridge);
    expect(phoneBridge.available).toBe(false);
  });

  it('answers queries with false', async () => {
    await expect(phoneBridge.hasNotificationAccess()).resolves.toBe(false);
    await expect(phoneBridge.requestCallPermissions()).resolves.toBe(false);
    await expect(phoneBridge.isDoNotDisturb()).resolves.toBe(false);
  });

  it('rejects actions, except stopping a keep-alive that never started', async () => {
    await expect(phoneBridge.answerCall()).rejects.toThrow('only available on Android');
    await expect(phoneBridge.mediaCommand('play')).rejects.toThrow();
    await expect(phoneBridge.startKeepAlive('PineTime')).rejects.toThrow();
    await expect(phoneBridge.stopKeepAlive()).resolves.toBeUndefined();
  });

  it('subscribes without ever calling the listener', () => {
    const listener = jest.fn();
    const unsubscribe = phoneBridge.subscribe(listener);
    unsubscribe();
    expect(listener).not.toHaveBeenCalled();
  });
});

describe('toPhoneEvent', () => {
  it('fills in missing notification text', () => {
    expect(toPhoneEvent({ type: 'notification', id: 7, app: 'Signal' })).toEqual({
      type: 'notification',
      id: 7,
      app: 'Signal',
      title: '',
      body: '',
    });
  });

  it('leaves absent optional fields out', () => {
    expect(toPhoneEvent({ type: 'call', state: 'incoming', number: '123' })).toEqual({
      type: 'call',
      state: 'incoming',
      number: '123',
    });
    expect(toPhoneEvent({ type: 'media', playing: false })).toEqual({ type: 'media', playing: false });
  });

  it('drops events it does not understand', () => {
    expect(toPhoneEvent(null)).toBeNull();
    expect(toPhoneEvent({ type: 'call', state: 'ringing' })).toBeNull();
    expect(toPhoneEvent({ type: 'notificationRemoved' })).toBeNull();
    expect(toPhoneEvent({ type: 'battery' })).toBeNull();
  });
});
