// The files every watch needs that are not apps, such as each accelerometer's configuration, the shared library's
// icons, the text font and the native modules, which the firmware does not carry. The companion puts any a watch lacks
// or holds in another version; core/src/sync/system_files.rs does the same for the app, and the two must agree.
//
// The file service lists only names and sizes, so after putting them the companion writes `SYSTEM_MANIFEST_PATH`,
// a line per file with its CRC-32, size and path, and a file counts as held only while the manifest names it with the
// bundle's CRC and the listing with its size. A watch without the manifest gets every file once.

import { base64ToBytes } from '@/ble/encoding';
import { SYSTEM_FILES, type BundledSystemFile } from '@/bundles/system-files';

import { crc32 } from './crc32';
import type { WatchFile } from './file-service';

export const SYSTEM_MANIFEST_PATH = '/system/manifest';

export type SystemFileWriter = {
  list(dir: string): Promise<WatchFile[]>;
  read(path: string): Promise<Uint8Array>;
  write(path: string, data: Uint8Array): Promise<void>;
};

// The manifest's line for one file: its CRC-32 in hex, its size and its path.
function manifestLine(path: string, contents: Uint8Array): string {
  return `${crc32(contents).toString(16).padStart(8, '0')} ${contents.length} ${path}`;
}

// Writes each system file the watch lacks or holds in another version, then the manifest; gives the paths it wrote.
export async function putMissingSystemFiles(
  files: SystemFileWriter,
  bundle: BundledSystemFile[] = SYSTEM_FILES,
): Promise<string[]> {
  const onWatch = await files
    .read(SYSTEM_MANIFEST_PATH)
    .then((bytes) => new TextDecoder().decode(bytes))
    .catch(() => '');
  const held = new Set(onWatch.split('\n').filter(Boolean));
  const written: string[] = [];
  const lines: string[] = [];
  for (const file of bundle) {
    const contents = base64ToBytes(file.base64);
    const line = manifestLine(file.path, contents);
    lines.push(line);
    const slash = file.path.lastIndexOf('/');
    const folder = file.path.slice(0, slash) || '/';
    const name = file.path.slice(slash + 1);
    // A folder the watch has never had lists as missing, which means the file is too.
    const entries = await files.list(folder).catch(() => [] as WatchFile[]);
    const listed = entries.some((entry) => entry.name === name && !entry.directory && entry.size === contents.length);
    if (!listed || !held.has(line)) {
      await files.write(file.path, contents);
      written.push(file.path);
    }
  }
  const manifest = `${lines.join('\n')}\n`;
  if (manifest !== onWatch) {
    await files.write(SYSTEM_MANIFEST_PATH, new TextEncoder().encode(manifest));
  }
  return written;
}
