// Watch backups kept on this device, newest first.

import type { Backup } from '@/protocol/backup';
import type { KeyValueStorage } from '@/services/app-store';
import { appStorage } from '@/state/storage';

const STORAGE_KEY = 'battletime.backups.v1';

// Older backups are dropped beyond this many.
const KEEP = 10;

export type StoredBackup = { id: string; watch: string; backup: Backup };

export async function loadBackups(storage: KeyValueStorage = appStorage): Promise<StoredBackup[]> {
  try {
    const raw = await storage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as StoredBackup[]) : [];
  } catch {
    return [];
  }
}

async function store(list: StoredBackup[], storage: KeyValueStorage): Promise<StoredBackup[]> {
  const kept = list.slice(0, KEEP);
  await storage.setItem(STORAGE_KEY, JSON.stringify(kept));
  return kept;
}

export async function addBackup(
  watch: string,
  backup: Backup,
  storage: KeyValueStorage = appStorage,
): Promise<StoredBackup[]> {
  const entry = { id: backup.created, watch, backup };
  return store([entry, ...(await loadBackups(storage))], storage);
}

export async function removeBackup(id: string, storage: KeyValueStorage = appStorage): Promise<StoredBackup[]> {
  return store(
    (await loadBackups(storage)).filter((entry) => entry.id !== id),
    storage,
  );
}
