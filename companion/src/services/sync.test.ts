import { describe, expect, it, jest } from '@jest/globals';

import { shareSyncInterval, syncBridge, syncIntervalMs, type SyncBridge } from './sync';

function bridge(intervalSeconds: number) {
  const setSettings = jest.fn(async () => ({ ok: true }));
  const fake = {
    settings: async () => ({ ok: true, value: { intervalSeconds } }),
    setSettings,
  } as unknown as SyncBridge;
  return { fake, setSettings };
}

describe('sync interval', () => {
  it('keeps a stored interval in range and falls back to five minutes', () => {
    expect(syncIntervalMs(900)).toBe(900_000);
    expect(syncIntervalMs(10)).toBe(300_000);
    expect(syncIntervalMs(2.5)).toBe(300_000);
    expect(syncIntervalMs(7 * 24 * 3600)).toBe(300_000);
  });

  it('tells the core only when the interval changed', async () => {
    const same = bridge(300);
    await shareSyncInterval(same.fake, 300);
    expect(same.setSettings).not.toHaveBeenCalled();
    const other = bridge(300);
    await shareSyncInterval(other.fake, 900);
    expect(other.setSettings).toHaveBeenCalledWith({ intervalSeconds: 900 });
  });

  it('passes on a refusal from the core', async () => {
    const refusing = {
      settings: async () => ({ ok: false, error: 'no' }),
    } as unknown as SyncBridge;
    await expect(shareSyncInterval(refusing, 60)).rejects.toThrow('no');
  });

  it('finds no bridge outside the desktop app', () => {
    expect(syncBridge()).toBeNull();
  });
});
