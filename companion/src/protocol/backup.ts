// Copies the watch's own data off it and back again: the system settings and each app's saved state.
// Firmware and app images are not included, since both can be installed again.

import { base64ToBytes, bytesToBase64 } from '@/ble/encoding';

import type { FileService, Progress, WatchFile } from './file-service';

export type Backup = {
  format: 'neotime-backup';
  version: 1;
  created: string;
  // File contents keyed by their path on the watch, base64 encoded.
  files: Record<string, string>;
};

// The file each app keeps its saved state in, inside its own folder.
const APP_STATE = 'state';

async function listOrEmpty(service: FileService, dir: string): Promise<WatchFile[]> {
  try {
    return await service.list(dir);
  } catch {
    // A folder that does not exist yet has nothing to back up.
    return [];
  }
}

async function walk(service: FileService, dir: string): Promise<WatchFile[]> {
  const files: WatchFile[] = [];
  for (const entry of await listOrEmpty(service, dir)) {
    if (entry.directory) {
      files.push(...(await walk(service, entry.path)));
    } else {
      files.push(entry);
    }
  }
  return files;
}

// Work out which files a backup takes, without reading them.
export async function backupPlan(service: FileService): Promise<WatchFile[]> {
  const plan = await walk(service, '/settings');
  for (const app of await listOrEmpty(service, '/apps')) {
    if (!app.directory) {
      continue;
    }
    const state = (await listOrEmpty(service, app.path)).find((file) => file.name === APP_STATE);
    if (state) {
      plan.push(state);
    }
  }
  return plan;
}

export async function createBackup(
  service: FileService,
  options: { onProgress?: (done: number, total: number, file: string) => void } = {},
): Promise<Backup> {
  const plan = await backupPlan(service);
  const total = plan.reduce((sum, file) => sum + file.size, 0);
  const files: Record<string, string> = {};
  let done = 0;

  for (const file of plan) {
    const data = await service.read(file.path, (progress: Progress) =>
      options.onProgress?.(done + progress.sent, total, file.path),
    );
    files[file.path] = bytesToBase64(data);
    done += data.length;
    options.onProgress?.(done, total, file.path);
  }

  return { format: 'neotime-backup', version: 1, created: new Date().toISOString(), files };
}

export function parseBackup(text: string): Backup {
  let parsed: Partial<Backup>;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error('That file is not a watch backup.');
  }
  if (parsed.format !== 'neotime-backup' || parsed.version !== 1 || typeof parsed.files !== 'object') {
    throw new Error('That file is not a watch backup.');
  }
  return parsed as Backup;
}

export function backupSize(backup: Backup): number {
  return Object.values(backup.files).reduce((sum, data) => sum + base64ToBytes(data).length, 0);
}

// Write every file in a backup back to the watch. The watch reads its settings when it starts, so it
// needs a restart afterwards.
export async function restoreBackup(
  service: FileService,
  backup: Backup,
  options: { onProgress?: (done: number, total: number, file: string) => void } = {},
): Promise<void> {
  const entries = Object.entries(backup.files).map(([path, data]) => ({
    path,
    data: base64ToBytes(data),
  }));
  const total = entries.reduce((sum, entry) => sum + entry.data.length, 0);
  let done = 0;

  for (const entry of entries) {
    await service.write(entry.path, entry.data, (progress) =>
      options.onProgress?.(done + progress.sent, total, entry.path),
    );
    done += entry.data.length;
    options.onProgress?.(done, total, entry.path);
  }
}
