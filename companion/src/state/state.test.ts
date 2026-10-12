import { beforeEach, describe, expect, it, jest } from '@jest/globals';
import AsyncStorage from '@react-native-async-storage/async-storage';

import type { Backup } from '@/protocol/backup';

import { addBackup, loadBackups, removeBackup } from './backups';
import { defaultSettings, loadSettings, saveSettings } from './settings';

jest.mock('@react-native-async-storage/async-storage', () =>
  jest.requireActual('@react-native-async-storage/async-storage/jest/async-storage-mock'),
);

function backupAt(created: string): Backup {
  return { format: 'neotime-backup', version: 1, created, files: {} };
}

beforeEach(async () => {
  await AsyncStorage.clear();
});

describe('backups', () => {
  it('starts with no backups', async () => {
    await expect(loadBackups()).resolves.toEqual([]);
  });

  it('keeps backups newest first, named by when they were made', async () => {
    await addBackup('PineTime', backupAt('2026-01-01'));
    const list = await addBackup('PineTime', backupAt('2026-02-01'));
    expect(list.map((entry) => entry.id)).toEqual(['2026-02-01', '2026-01-01']);
    expect(list[0]).toEqual({ id: '2026-02-01', watch: 'PineTime', backup: backupAt('2026-02-01') });
    await expect(loadBackups()).resolves.toEqual(list);
  });

  it('drops the oldest backups beyond ten', async () => {
    for (let day = 1; day <= 12; day++) {
      await addBackup('PineTime', backupAt(`2026-01-${String(day).padStart(2, '0')}`));
    }
    const list = await loadBackups();
    expect(list).toHaveLength(10);
    expect(list[0].id).toBe('2026-01-12');
    expect(list[9].id).toBe('2026-01-03');
  });

  it('removes one backup by its id and leaves the rest', async () => {
    await addBackup('PineTime', backupAt('a'));
    await addBackup('PineTime', backupAt('b'));
    const list = await removeBackup('a');
    expect(list.map((entry) => entry.id)).toEqual(['b']);
    await expect(loadBackups()).resolves.toEqual(list);
  });

  it('treats unreadable storage as no backups', async () => {
    await AsyncStorage.setItem('neotime.backups.v1', '{not json');
    await expect(loadBackups()).resolves.toEqual([]);
    jest.mocked(AsyncStorage.getItem).mockRejectedValueOnce(new Error('disk gone'));
    await expect(loadBackups()).resolves.toEqual([]);
  });
});

describe('settings', () => {
  it('gives the defaults when nothing is saved', async () => {
    await expect(loadSettings()).resolves.toEqual(defaultSettings);
  });

  it('reads back what was saved', async () => {
    const settings = { ...defaultSettings, lastWatchName: 'PineTime', forwardWeather: true };
    await saveSettings(settings);
    await expect(loadSettings()).resolves.toEqual(settings);
  });

  it('fills settings added since the blob was saved from the defaults', async () => {
    await AsyncStorage.setItem('neotime.settings.v1', JSON.stringify({ autoReconnect: false }));
    await expect(loadSettings()).resolves.toEqual({ ...defaultSettings, autoReconnect: false });
  });

  it('falls back to the defaults when the blob is unreadable', async () => {
    await AsyncStorage.setItem('neotime.settings.v1', 'garbage');
    await expect(loadSettings()).resolves.toEqual(defaultSettings);
  });
});
