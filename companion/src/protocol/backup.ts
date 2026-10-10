// Copies the watch's own data off it and back again: settings, alarms, app
// settings and, if asked, the step logs. Firmware and app code are not
// included, since both can be installed again.

import { base64ToBytes, bytesToBase64 } from '@/ble/encoding';

import { listDirectory, readFile, type WatchFile } from './files';
import { sendFile, type PackageChannel, type Progress } from './transfer';
import type { AbiInfo } from './packages';

export type Backup = {
  format: 'waspos-backup';
  version: 1;
  created: string;
  // File contents keyed by their path on the watch, base64 encoded.
  files: Record<string, string>;
};

// Files at the top of the flash that hold what the user set up on the watch.
const TOP_LEVEL = ['settings.json', 'alarms.txt', 'Morse.txt'];

async function listOrEmpty(channel: PackageChannel, dir: string): Promise<WatchFile[]> {
  try {
    return await listDirectory(channel, dir);
  } catch {
    // A directory that does not exist yet has nothing to back up.
    return [];
  }
}

async function walk(channel: PackageChannel, dir: string): Promise<WatchFile[]> {
  const files: WatchFile[] = [];
  for (const entry of await listOrEmpty(channel, dir)) {
    if (entry.directory) {
      files.push(...(await walk(channel, entry.path)));
    } else {
      files.push(entry);
    }
  }
  return files;
}

// Work out which files a backup takes, without reading them.
export async function backupPlan(
  channel: PackageChannel,
  options: { stepLogs?: boolean } = {},
): Promise<WatchFile[]> {
  const top = await listOrEmpty(channel, '');
  const plan = top.filter((entry) => !entry.directory && TOP_LEVEL.includes(entry.name));

  for (const pkg of await listOrEmpty(channel, 'pkg')) {
    if (!pkg.directory) {
      continue;
    }
    const config = (await listOrEmpty(channel, pkg.path)).find((file) => file.name === 'config.json');
    if (config) {
      plan.push(config);
    }
  }

  if (options.stepLogs) {
    plan.push(...(await walk(channel, 'logs')));
  }
  return plan;
}

export async function createBackup(
  channel: PackageChannel,
  options: { stepLogs?: boolean; onProgress?: (done: number, total: number, file: string) => void } = {},
): Promise<Backup> {
  const plan = await backupPlan(channel, options);
  const total = plan.reduce((sum, file) => sum + file.size, 0);
  const files: Record<string, string> = {};
  let done = 0;

  for (const file of plan) {
    const data = await readFile(channel, file.path, (progress: Progress) =>
      options.onProgress?.(done + progress.sent, total, file.path),
    );
    files[file.path] = bytesToBase64(data);
    done += data.length;
    options.onProgress?.(done, total, file.path);
  }

  return { format: 'waspos-backup', version: 1, created: new Date().toISOString(), files };
}

export function parseBackup(text: string): Backup {
  let parsed: Partial<Backup>;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error('That file is not a watch backup.');
  }
  if (parsed.format !== 'waspos-backup' || parsed.version !== 1 || typeof parsed.files !== 'object') {
    throw new Error('That file is not a watch backup.');
  }
  return parsed as Backup;
}

export function backupSize(backup: Backup): number {
  return Object.values(backup.files).reduce((sum, data) => sum + base64ToBytes(data).length, 0);
}

// Write every file in a backup back to the watch. The watch reads its settings
// and alarms when it starts, so it needs a restart afterwards.
export async function restoreBackup(
  channel: PackageChannel,
  backup: Backup,
  options: { abi?: AbiInfo | null; onProgress?: (done: number, total: number, file: string) => void } = {},
): Promise<void> {
  const entries = Object.entries(backup.files).map(([path, data]) => ({
    path,
    data: base64ToBytes(data),
  }));
  const total = entries.reduce((sum, entry) => sum + entry.data.length, 0);
  let done = 0;

  for (const entry of entries) {
    await sendFile(channel, entry.path, entry.data, {
      abi: options.abi,
      onProgress: (progress) => options.onProgress?.(done + progress.sent, total, entry.path),
    });
    done += entry.data.length;
    options.onProgress?.(done, total, entry.path);
  }
}
