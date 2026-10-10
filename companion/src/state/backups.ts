// Watch backups kept on the phone, newest first.

import AsyncStorage from '@react-native-async-storage/async-storage';

import type { Backup } from '@/protocol/backup';

const STORAGE_KEY = 'waspos.backups.v1';

// Older backups are dropped beyond this many.
const KEEP = 10;

export type StoredBackup = { id: string; watch: string; backup: Backup };

export async function loadBackups(): Promise<StoredBackup[]> {
  try {
    const raw = await AsyncStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as StoredBackup[]) : [];
  } catch {
    return [];
  }
}

async function store(list: StoredBackup[]): Promise<StoredBackup[]> {
  const kept = list.slice(0, KEEP);
  await AsyncStorage.setItem(STORAGE_KEY, JSON.stringify(kept));
  return kept;
}

export async function addBackup(watch: string, backup: Backup): Promise<StoredBackup[]> {
  const entry = { id: backup.created, watch, backup };
  return store([entry, ...(await loadBackups())]);
}

export async function removeBackup(id: string): Promise<StoredBackup[]> {
  return store((await loadBackups()).filter((entry) => entry.id !== id));
}
